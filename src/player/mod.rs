//! A single-track audio player
//!
//! To create a player, use [`Player::builder`] to obtain a [`Builder`]. Set the options that are
//! needed, then call [`Builder::run`] or [`Builder::build`] then [`Player::run`]. After, use the
//! [`Handle`] to interact with the underlying [`Player`].
//!
//! Much of the important functionality is in the options of the player, which can be looked more
//! into depth in the [`builder`] documentation. [`Handle`]s are also a good resource.
//!
//! # Examples
//!
//! ## Usage
//!
//! ```
//! # fn main() -> Result<(), sauti::player::Disconnected> {
//! use sauti::player::prelude::*;
//!
//! // [`Builder`] can be used to create a [`Player`] term by term
//! let handle = Player::builder().volume(0.5).run();
//!
//! // A [`Handle`] can be used to play, pause, and resume a file
//! handle.play(sauti::test::file())?;
//! handle.pause()?;
//! handle.resume()?;
//!
//! // It can also query for information from the file
//! let _ = handle.play_state()?;
//! let _ = handle.duration()?;
//! # Ok(()) }
//! ```
//!
//! ## Cli Player
//!
//! ```no_run
//! # fn main() -> Result<(), sauti::player::Disconnected> {
//! use sauti::player::prelude::*;
//!
//! // [`Builder`] can be used to create a [`Player`] term by term
//! let handle = Player::builder().volume(0.5).run();
//!
//! // Get the file from executable arugments
//! let Some(path) = std::env::args().nth(1) else {
//!     println!("usage: {{command}} {{path}}");
//!     return Ok(());
//! };
//!
//! // Start playing the file
//! handle.play(path)?;
//!
//! // Wait for user input to exit
//! std::io::stdin()
//!     .read_line(&mut String::new())
//!     .expect("failed to read stdin");
//! # Ok(()) }
//! ```

// TODO: setting device options

/// Useful types for interacting with a [`Player`]
pub mod prelude {
    pub use super::{
        builder::Builder as PlayerBuilder, callback, callback::PlayerRef, Generic as _,
        Handle as PlayerHandle, PlayState, Player, PlayerError, PlayerResult,
    };
    pub use crate::data::prelude::*;
    pub use crate::decoder::Direction;
    pub use crate::effect::prelude::*;
    pub use crate::output::DeviceOptions;
    pub use crate::player;
    pub use std::time::Duration;
}

use std::convert::Infallible;
use std::ops::ControlFlow;
use std::sync::{Arc, RwLock, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam::atomic::AtomicCell;
use crossbeam::sync::WaitGroup;
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use log::error;
use thiserror::Error;

use crate::decoder::{prelude::*, Direction, ExtensionSet, SeekError};
use crate::effect::prelude::*;
use crate::output::prelude::*;
use callback::prelude::*;

use self::builder::{
    Builder, DecoderSupplier, DefaultDecoder, DefaultEffect, DefaultOutput, EffectSupplier,
    OutputSupplier,
};
use self::decoder::{NoPacket, PlayerDecoder};
use self::output::PacketPlayer;

pub mod builder;
pub mod callback;
mod decoder;
mod output;

#[derive(Debug)]
enum Message {
    Play(MediaSource),
    SetState(PlayState),
    SetVolume(f64),
    SeekTo(Duration),
    SeekBy(Duration, Direction),
    Synchronize(crossbeam::sync::WaitGroup),
}

#[derive(Debug)]
enum OutputControl {
    Flush,
    SetState(PlayState),
    SetVolume(f64),
}

/// A generic form of a reference to a [`Player`]
///
/// [`Handle`] is a notable implementor of this, and [`OnStreamEnd`] gives this as a parameter.
pub trait Generic {
    type ModifyError: Into<PlayerError>;
    type GetError: Into<PlayerError>;

    /// Make the player start playing `source`
    ///
    /// # Errors
    ///
    /// - [`Player`]
    ///     - If [resuming](Self::set_state) fails
    /// - [`Handle`]
    ///     - If the player disconnected
    fn play(&mut self, source: &MediaSource) -> Result<(), Self::ModifyError>;

    /// Change the [`Player`]'s [`PlayState`] to [`Paused`](PlayState::Paused)
    ///
    /// # Errors
    ///
    /// - See [`Self::set_state`]
    fn pause(&mut self) -> Result<bool, Self::ModifyError> {
        self.set_state(PlayState::Paused)
    }

    /// Change the [`Player`]'s [`PlayState`] to [`Playing`](PlayState::Playing)
    ///
    /// # Errors
    ///
    /// - See [`Self::set_state`]
    fn resume(&mut self) -> Result<bool, Self::ModifyError> {
        self.set_state(PlayState::Playing)
    }

    /// Change the [`Player`]'s [`PlayState`] to [`Stopped`](PlayState::Stopped)
    ///
    /// # Errors
    ///
    /// - See [`Self::set_state`]
    fn stop(&mut self) -> Result<(), Self::ModifyError> {
        self.set_state(PlayState::Stopped)?;
        Ok(())
    }

    /// Change the [`Player`]'s [`PlayState`] to `play_state`, returning `true` if it succeeds.
    ///
    /// Setting the state to [`PlayState::Playing`] or [`PlayState::Paused`] when it is
    /// [`PlayState::Stopped`] is disallowed, as the player doesn't have a song to pause or resume.
    /// As such, this function will return `false`. Use [`Self::play`] instead.
    ///
    /// # Errors
    ///
    /// - [`Player`]
    ///     - When [stopping](PlayState::Stopped):
    ///         - If [pausing the device](Device::pause) fails
    ///         - If [seeking the stream](AudioStream::seek_to) fails
    ///     - If the output [`Device`] has an error
    /// - [`Handle`]
    ///     - If the player disconnected
    fn set_state(&mut self, play_state: PlayState) -> Result<bool, Self::ModifyError>;

    /// Change the [`Player`]'s [`PlayState`] to `play_state`, ignoring the previous state.
    ///
    /// This bypasses the restrictions around stopping for [`Self::set_state`], and will always
    /// change the state.
    ///
    /// # Errors
    ///
    /// - See [`Self::set_state`]
    fn set_state_unchecked(&mut self, play_state: PlayState) -> Result<(), Self::ModifyError>;

    /// Change the [`Player`]'s volume to `volume`
    ///
    /// # Errors
    ///
    /// - [`Player`]
    ///     - If the output [`Device`] has an error
    /// - [`Handle`]
    ///     - If the player disconnected
    fn set_volume(&mut self, volume: f64) -> Result<(), Self::ModifyError>;

    /// Seek the underlying [`AudioStream`] to `duration` from the start
    ///
    /// # Errors
    ///
    /// - [`Player`]
    ///     - If the output [`Device`] has an error
    ///     - If [`AudioStream::seek_to`] has an error
    /// - [`Handle`]
    ///     - If the player disconnected
    fn seek_to(&mut self, duration: Duration) -> Result<(), Self::ModifyError>;

    /// Seek the underlying [`AudioStream`] a certain `duration` in `direction` from the current position
    ///
    /// # Errors
    ///
    /// - [`Player`]
    ///     - If the output [`Device`] has an error
    ///     - If [`AudioStream::seek_by`] has an error
    /// - [`Handle`]
    ///     - If the player disconnected
    fn seek_by(
        &mut self,
        duration: Duration,
        direction: Direction,
    ) -> Result<(), Self::ModifyError>;

    /// Get the current play state of the [`Player`]
    ///
    /// # Errors
    ///
    /// - [`Handle`]: If the player disconnected
    fn play_state(&self) -> Result<PlayState, Self::GetError>;

    /// Get the current volume of the [`Player`]
    ///
    /// # Errors
    ///
    /// - [`Handle`]: If the player disconnected
    fn volume(&self) -> Result<f64, Self::GetError>;

    /// Get the current [`Duration`] from the start of the playing [`AudioStream`], or [`None`] if
    /// there is no stream playing
    ///
    /// # Errors
    ///
    /// - [`Handle`]: If the player disconnected
    fn position(&self) -> Result<Option<Duration>, Self::GetError>;

    /// Get the length of the current [`AudioStream`], or [`None`] if there is no stream playing
    ///
    /// # Errors
    ///
    /// - [`Handle`]: If the player disconnected
    fn duration(&self) -> Result<Option<Duration>, Self::GetError>;

    /// Get the progress of the [`AudioStream`] to the end of the current file, or [`None`] if
    /// there is no stream playing.
    ///
    /// This is computed as `position / duration`. If the [duration](Self::duration) is 0, then 1.0
    /// is returned.
    ///
    /// # Errors
    ///
    /// - [`Handle`]: If the player disconnected
    fn progress(&self) -> Result<Option<f64>, Self::GetError>;

    /// Get a [synchronized](std::sync) reference to the **current** [`AudioStream`]'s position and
    /// duration. Note that this doesn't synchronize between different played files, so it should
    /// only be used when it's obtained.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn main() -> Result<(), sauti::player::Disconnected> {
    /// use sauti::player::prelude::*;
    ///
    /// let player = Player::builder().run();
    /// player.play(sauti::test::file())?;
    ///
    /// // note that the times won't automatically populate,
    /// // as the player (in another thread) needs to handle
    /// // the message first
    /// if let Some(times) = player.times()? {
    ///     println!(
    ///        "pos: {:?}, dur: {:?} ({:.1}%)",
    ///        times.position(),
    ///        times.duration(),
    ///        times.progress(),
    ///    );
    /// }
    /// # Ok(()) }
    /// ```
    ///
    /// # Errors
    ///
    /// - [`Handle`]: If the player disconnected
    fn times(&self) -> Result<Option<Arc<dyn StreamTimes>>, Self::GetError>;

    /// Block this thread until all previous messages have been recieved
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> Result<(), sauti::player::Disconnected> {
    /// # env_logger::init();
    /// use sauti::player::prelude::*;
    /// use sauti::test::prelude::*;
    ///
    /// // Create a sound packet with two frames and two channels
    /// let packet = SoundPacket::from_channels(&[[1, 2], [3, 4]], 44100);
    /// let generic = GenericPacket::from(packet);
    /// // Create the player that continuously repeats the packets
    /// let player = Player::builder()
    ///     .decoder(Provider::repeat(generic))
    ///     .run();
    ///
    /// player.play("")?;
    ///
    /// assert_eq!(player.play_state()?, PlayState::Stopped);
    /// player.synchronize();
    /// assert_eq!(player.play_state()?, PlayState::Playing);
    /// # Ok(()) }
    /// ```
    ///
    /// # Errors
    ///
    /// - [`Handle`]: If the player disconnected before being able to synchronize
    fn synchronize(&self) -> Result<(), Self::ModifyError>;

    /// Get a set of extensions that the player currently supports
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> Result<(), sauti::player::Disconnected> {
    /// use sauti::player::prelude::*;
    ///
    /// let player = Player::builder().run();
    /// let extensions = player.supported_extensions()?;
    ///
    /// assert!(extensions.contains("mp3"));
    /// assert!(!extensions.contains("txt"));
    /// # Ok(()) }
    /// ```
    ///
    /// # Errors
    ///
    /// - [`Handle`]: If the player disconnected
    fn supported_extensions(&self) -> Result<&ExtensionSet, Self::GetError>;
}

impl<T: ?Sized + Generic> Generic for &mut T {
    type GetError = T::GetError;
    type ModifyError = T::ModifyError;

    fn play(&mut self, source: &MediaSource) -> Result<(), Self::ModifyError> {
        (**self).play(source)
    }

    fn set_state(&mut self, play_state: PlayState) -> Result<bool, Self::ModifyError> {
        (**self).set_state(play_state)
    }

    fn set_state_unchecked(&mut self, play_state: PlayState) -> Result<(), Self::ModifyError> {
        (**self).set_state_unchecked(play_state)
    }

    fn set_volume(&mut self, volume: f64) -> Result<(), Self::ModifyError> {
        (**self).set_volume(volume)
    }

    fn seek_to(&mut self, duration: Duration) -> Result<(), Self::ModifyError> {
        (**self).seek_to(duration)
    }

    fn seek_by(
        &mut self,
        duration: Duration,
        direction: Direction,
    ) -> Result<(), Self::ModifyError> {
        (**self).seek_by(duration, direction)
    }

    fn play_state(&self) -> Result<PlayState, Self::GetError> {
        (**self).play_state()
    }

    fn volume(&self) -> Result<f64, Self::GetError> {
        (**self).volume()
    }

    fn position(&self) -> Result<Option<Duration>, Self::GetError> {
        (**self).position()
    }

    fn duration(&self) -> Result<Option<Duration>, Self::GetError> {
        (**self).duration()
    }

    fn times(&self) -> Result<Option<Arc<dyn StreamTimes>>, Self::GetError> {
        (**self).times()
    }

    fn progress(&self) -> Result<Option<f64>, Self::GetError> {
        (**self).progress()
    }

    fn synchronize(&self) -> Result<(), Self::ModifyError> {
        (**self).synchronize()
    }

    fn supported_extensions(&self) -> Result<&ExtensionSet, Self::GetError> {
        (**self).supported_extensions()
    }
}

struct Shared {
    play_state: AtomicCell<PlayState>,
    volume: AtomicCell<f64>,
    // NOTE: not sure if this is the best way to do it
    // Currently, some data is behind two atomics, but this is probably impossible to circumvent,
    // as some data isn't atomic in StreamTimes (like the samplerate in the symphonia implementation),
    // and it still has to be changed between different songs
    // It would also be great if this could be atomic rather than a RwLock,
    // but AtomicCell<Arc> wouldn't even work because Arc isn't Copy
    // and AtomicCell<Option<Box<dyn StreamTimes>>> doesn't work because StreamTimes can't be Copy
    // because none of the atomics are Copy
    times: RwLock<Option<Arc<dyn StreamTimes>>>,
    // TODO: make DeviceOptions shared too please
}

impl Shared {
    fn new(volume: f64) -> Self {
        Self {
            play_state: AtomicCell::default(),
            volume: AtomicCell::new(volume),
            times: RwLock::default(),
        }
    }
}

/// A single-track sound file player
///
/// The player routes sound packets obtained through the [`Decoder`] to the [`Output`] audio,
/// applying an [`Effect`] if given. The player may also run a custom callback for when a file ends
/// through [`OnStreamEnd`] or when an error occurs through [`OnError`].
///
/// To obtain a [`Player`], see [`Builder`].
///
/// The player automatically exits when every [`Handle`] goes out of scope
#[must_use = "Player doesn't do anything unless it's run"]
pub struct Player<O: Output, D: Decoder, E: Effect, OE: OnError, OSE: OnStreamEnd> {
    handle: Receiver<Message>,
    output: O,
    decoder: D,
    effects: E,
    on_error: OE,
    on_stream_end: OSE,
    options: DeviceOptions,
    shared: Arc<Shared>,
    start_playing: bool,
    supported_extensions: Arc<ExtensionSet>,
}

impl
    Player<
        crate::output::Default,
        crate::decoder::Default,
        crate::effect::Default,
        callback::error::Default,
        callback::stream_end::Default,
    >
{
    /// Construct a [`Builder`] filled with defaults.
    #[must_use]
    pub fn builder() -> Builder<
        DefaultOutput,
        DefaultDecoder,
        DefaultEffect,
        callback::error::Default,
        callback::stream_end::Default,
    > {
        Builder::default()
    }
}

impl<D: Decoder, E: Effect, O: Output, OE: OnError, OSE: OnStreamEnd> Player<O, D, E, OE, OSE> {
    fn new<
        OS: OutputSupplier<Out = O>,
        DS: DecoderSupplier<Out = D>,
        ES: EffectSupplier<Out = E>,
    >(
        builder: Builder<OS, DS, ES, OE, OSE>,
    ) -> (Self, Handle) {
        let (sender, receiver) = crossbeam_channel::unbounded();
        let shared: Arc<Shared> = Arc::new(Shared::new(builder.volume));
        let weak = Arc::downgrade(&shared);
        let decoder = builder.decoder.give();
        let supported_extensions = Arc::new(decoder.supported_extensions());
        (
            Self {
                handle: receiver,
                decoder,
                effects: builder.effects.give(),
                output: builder.output.give(),
                options: builder.options,
                shared,
                on_error: builder.on_error,
                on_stream_end: builder.on_stream_end,
                start_playing: builder.start_playing,
                supported_extensions: supported_extensions.clone(),
            },
            Handle {
                handle: sender,
                shared: weak,
                supported_extensions,
            },
        )
    }

    /// Run the player in another thread, returning a [`JoinHandle`] for that thread
    ///
    /// # Errors
    ///
    /// - If there was an issue starting audio output
    pub fn run(self) -> JoinHandle<()> {
        std::thread::spawn(move || {
            if let Err(err) = self.run_blocking() {
                error!("failed to start player: {err}");
            }
        })
    }

    /// Run the player in this thread, blocking until the player exits
    ///
    /// # Errors
    ///
    /// - If there was an issue starting audio output
    pub fn run_blocking(self) -> PlayerResult<()> {
        let (packet_sender, packet_receiver) = crossbeam_channel::bounded(8);
        // output control is a rendevous to make sure that the decoder and audio player is on the
        // same page at all times
        let (output_control, output_control_reciever) = crossbeam_channel::bounded(0);

        let source = PacketPlayer::new(
            &self,
            packet_receiver,
            output_control_reciever,
            self.shared.volume.load(),
        );
        let device = self.output.start_paused(self.options.clone(), source)?;
        let decoder = PlayerDecoder::new(&self, packet_sender);

        let mut inner = Inner {
            play_state: PlayState::Stopped,
            device,
            decoder,
            output_control,
            handle: &self.handle,
            shared: &self.shared,
            on_error: &self.on_error,
            on_stream_end: &self.on_stream_end,
            supported_extensions: &self.supported_extensions,
        };

        inner.run_blocking(self.start_playing);
        Ok(())
    }
}

#[allow(clippy::struct_field_names)] // "play state" is its own thing
struct Inner<'a, D: Decoder, OE: OnError, OSE: OnStreamEnd> {
    play_state: PlayState,
    device: Box<dyn Device>,
    decoder: PlayerDecoder<'a, D>,
    output_control: Sender<OutputControl>,
    handle: &'a Receiver<Message>,
    shared: &'a Shared,
    on_error: &'a OE,
    on_stream_end: &'a OSE,
    supported_extensions: &'a ExtensionSet,
}

impl<'a, D: Decoder, OE: OnError, OSE: OnStreamEnd> Inner<'a, D, OE, OSE> {
    fn run_blocking(&mut self, start_playing: bool) {
        let res = self.run_blocking_fallible(start_playing);
        assert!(res.is_break());
    }

    fn run_blocking_fallible(&mut self, start_playing: bool) -> ControlFlow<()> {
        if start_playing {
            // send a packet so that the output has something to play
            Self::try_recover(self.decoder.send_next_packet(), self)?;
            Self::try_recover(self.set_state_unchecked(PlayState::Playing), self)?;
        }

        loop {
            Self::try_recover(self.tick(), self)?;
        }
    }

    // these arguments are backwards so that self comes after the result,
    // making it so that there aren't two mutable borrows at the same time
    // i dunno why this is necessary, rust weirdness
    fn try_recover<T>(result: PlayerResult<T>, this: &mut Self) -> ControlFlow<()> {
        match result {
            Ok(_) => ControlFlow::Continue(()),
            Err(err) => this.handle_err(err),
        }
    }

    fn tick(&mut self) -> PlayerResult<()> {
        self.recv_or_exit(self.handle.try_recv())?;

        if self.play_state.is_playing() {
            match self.decoder.send_next_packet()? {
                // no reason to block; it already blocked due to packet being sent
                Ok(()) => Ok(()),
                // there wasn't a packet available, handle the reason
                Err(reason) => self.no_packet_available(reason),
            }
        } else {
            self.block_until_message()
        }
    }

    fn recv_or_exit<E>(&mut self, res: Result<Message, E>) -> PlayerResult<()>
    where
        TryRecvError: From<E>,
    {
        match res.map_err(TryRecvError::from) {
            Ok(message) => self.handle(message),
            Err(TryRecvError::Empty) => Ok(()),
            Err(TryRecvError::Disconnected) => Err(PlayerError::Exit),
        }
    }

    fn block_until_message(&mut self) -> PlayerResult<()> {
        self.recv_or_exit(self.handle.recv())
    }

    fn no_packet_available(&mut self, reason: NoPacket) -> PlayerResult<()> {
        // if the stream just ended, then run on_end
        if let Ok(source) = reason.try_into_stream_ended() {
            self.stream_ended(source, callback::stream_end::Reason::EndOfFile)?;
        }

        // no packet was sent, so we must block on a message
        // so that there isn't an infinite loop
        self.block_until_message()
    }

    fn handle(&mut self, message: Message) -> PlayerResult<()> {
        match message {
            Message::Play(source) => self.play(&source),
            // ignore whether or not setting the state worked, since [`Handle`] handles that
            // instead.
            Message::SetState(new) => self.set_state(new).map(drop),
            Message::SetVolume(new) => self.set_volume(new),
            Message::SeekTo(pos) => self.seek_to(pos),
            Message::SeekBy(duration, direction) => self.seek_by(duration, direction),
            Message::Synchronize(wait_group) => {
                // the WaitGroup just waits until all references are dropped
                drop(wait_group);
                Ok(())
            }
        }
    }

    fn send_control(&self, message: OutputControl) -> PlayerResult<()> {
        self.output_control
            .send(message)
            .map_err(|_| PlayerError::OutputDisconnected)
    }

    fn stop(&mut self) -> PlayerResult<()> {
        if let Some(stream) = self.decoder.stream() {
            // only emit StreamEnded if a file actually stopped being decoded
            self.stream_ended(stream.source().clone(), callback::stream_end::Reason::Stop)?;
        }
        self.device.pause()?;
        *(self.shared.times)
            .write()
            .expect("times should not be poisoned") = None;
        // stop the decoder by taking out the stream
        self.decoder.stop();
        Ok(())
    }

    fn stream_ended(
        &mut self,
        source: SourceName,
        reason: callback::stream_end::Reason,
    ) -> PlayerResult<()> {
        self.on_stream_end
            .stream_ended(callback::stream_end::Info {
                player: self.boxed(),
                source,
                reason,
            })?;
        Ok(())
    }

    fn handle_err(&mut self, err: PlayerError) -> ControlFlow<()> {
        let actions = self.on_error.handle(err, self.boxed()).into();
        for action in actions {
            self.handle_action(action)?;
        }
        ControlFlow::Continue(())
    }

    fn handle_action(&mut self, action: callback::Action) -> ControlFlow<()> {
        match action {
            callback::Action::Exit => ControlFlow::Break(()),
            callback::Action::Stop => self.run_and_handle_or_stop(Self::stop),
            // callback::Action::RestartOutput => todo!(),
        }
    }

    fn boxed(&mut self) -> callback::PlayerRef {
        Box::new(self)
    }

    fn run_and_handle_or_stop(
        &mut self,
        func: impl FnOnce(&mut Self) -> PlayerResult<()>,
    ) -> ControlFlow<()> {
        let res = func(self);
        if let Err(err) = res {
            let actions = self.on_error.handle(err, self.boxed()).into();
            if !actions.is_empty() {
                return ControlFlow::Break(());
            }
        }
        ControlFlow::Continue(())
    }
}

impl<'a, D: Decoder, OE: OnError, OSE: OnStreamEnd> Generic for Inner<'a, D, OE, OSE> {
    type ModifyError = PlayerError;
    type GetError = Infallible;

    fn play(&mut self, source: &MediaSource) -> PlayerResult<()> {
        // notify that we're replacing a song if it exists
        if let Some(stream) = self.decoder.stream() {
            self.stream_ended(
                stream.source().clone(),
                callback::stream_end::Reason::Replaced,
            )?;
        }
        // take out the previous song
        self.decoder.stop();
        // start playing a new one
        self.decoder.decode(source)?;
        // flush the packets from the previous song
        self.send_control(OutputControl::Flush)?;
        // make sure it's playing
        self.set_state_unchecked(PlayState::Playing)?;
        // update the shared times
        if let Some(stream) = self.decoder.stream() {
            let mut times = (self.shared.times)
                .write()
                .expect("times should not be poisoned");
            *times = Some(stream.times());
        }
        Ok(())
    }

    fn set_state_unchecked(&mut self, new: PlayState) -> PlayerResult<()> {
        if self.play_state == new {
            return Ok(());
        }

        let playing_before = !self.play_state.is_stopped();
        let playing_after = !new.is_stopped();
        match (playing_before, playing_after) {
            (false, true) => self.device.resume()?,
            (true, false) => self.stop()?,
            // no need to update
            _ => (),
        }

        // update all the different play states
        self.play_state = new;
        // shared is a different play state so that this doesn't have to query it every packet
        self.shared.play_state.store(new);
        // the output also has to know so it could send empty data when paused
        self.send_control(OutputControl::SetState(new))?;
        Ok(())
    }

    fn set_state(&mut self, play_state: PlayState) -> Result<bool, Self::ModifyError> {
        if self.play_state.is_stopped() && !play_state.is_stopped() {
            Ok(false)
        } else {
            self.set_state_unchecked(play_state)?;
            Ok(true)
        }
    }

    fn set_volume(&mut self, new: f64) -> PlayerResult<()> {
        self.send_control(OutputControl::SetVolume(new))?;
        self.shared.volume.store(new);
        Ok(())
    }

    fn seek_to(&mut self, duration: Duration) -> PlayerResult<()> {
        let res = self
            .decoder
            .modify_stream(|stream| stream.seek_to(duration));
        // if the seek was out of bounds, stop the player
        if let Err(PlayerError::Decoder(DecoderError::SeekError {
            reason: SeekError::OutOfBounds,
            source,
        })) = res
        {
            self.stream_ended(source, callback::stream_end::Reason::EndOfFile)
        } else {
            res?;
            self.send_control(OutputControl::Flush)
        }
    }

    fn seek_by(&mut self, duration: Duration, direction: Direction) -> PlayerResult<()> {
        let res = self
            .decoder
            .modify_stream(|stream| stream.seek_by(duration, direction));
        // if the seek was out of bounds, stop the player
        if let Err(PlayerError::Decoder(DecoderError::SeekError {
            reason: SeekError::OutOfBounds,
            source,
        })) = res
        {
            self.stream_ended(source, callback::stream_end::Reason::EndOfFile)
        } else {
            res?;
            self.send_control(OutputControl::Flush)
        }
    }

    fn play_state(&self) -> Result<PlayState, Self::GetError> {
        Ok(self.play_state)
    }

    fn volume(&self) -> Result<f64, Self::GetError> {
        Ok(self.shared.volume.load())
    }

    fn position(&self) -> Result<Option<Duration>, Self::GetError> {
        Ok(self.decoder.stream().map(AudioStream::position))
    }

    fn duration(&self) -> Result<Option<Duration>, Self::GetError> {
        Ok(self.decoder.stream().map(AudioStream::duration))
    }

    fn progress(&self) -> Result<Option<f64>, Self::GetError> {
        Ok(self.decoder.stream().map(AudioStream::progress))
    }

    fn times(&self) -> Result<Option<Arc<dyn StreamTimes>>, Self::GetError> {
        Ok(self.decoder.stream().map(AudioStream::times))
    }

    fn synchronize(&self) -> Result<(), Self::ModifyError> {
        // Since this is called on the inner, it is already guaranteed to be synchronized
        Ok(())
    }

    fn supported_extensions(&self) -> Result<&ExtensionSet, Self::GetError> {
        Ok(self.supported_extensions)
    }
}

impl<'a, D: Decoder, OE: OnError, OSE: OnStreamEnd> Drop for Inner<'a, D, OE, OSE> {
    fn drop(&mut self) {
        // Stop output before dropping, since the sound source expects the output_control sender
        // to never be disconnected. By stopping it, the sound source never looks at the sender.
        let _ = self
            .output_control
            .send(OutputControl::SetState(PlayState::Stopped));
    }
}

/// A handle to a [`Player`] that could control it or query its info
///
/// This implements [`Generic`], which is how info is queried, but it also provides methods for
/// controlling the player with an immutable reference (which is achieved using a channel).
///
/// # Errors
///
/// If the player disconnects, then all methods will return [`Err(Disconnected)`](Disconnected)
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), sauti::player::Disconnected> {
/// use sauti::player::prelude::*;
/// use sauti::test::prelude::*;
/// use std::time::Duration;
/// use std::thread::sleep;
///
/// // create a new player that ignores audio
/// let handle = Empty::player().run();
///
/// // the handle can be used to control the player, so start playing an imaginary file
/// // [`Empty`] ignores the [`MediaSource`], so just send an empty path
/// handle.play("")?;
/// // it may take a bit for the player to recieve the message, so wait for it
/// handle.synchronize();
/// // once the player starts playing, it changes to [`PlayState::Playing`]
/// assert_eq!(handle.play_state()?, PlayState::Playing);
///
/// // the handle can also pause and resume the player
/// handle.pause()?;
/// handle.synchronize();
/// // and the play state changes as a result
/// assert_eq!(handle.play_state()?, PlayState::Paused);
/// # Ok(()) }
/// ```
#[derive(Clone)]
pub struct Handle {
    handle: Sender<Message>,
    // TODO: measure the cost of using Weak instead of Arc
    shared: Weak<Shared>,
    supported_extensions: Arc<ExtensionSet>,
}

/// An error representing that a [`Handle`] could not connect to its respective [`Player`]
#[derive(Error, Debug)]
#[error("player was disconnected")]
pub struct Disconnected;

impl From<Disconnected> for PlayerError {
    fn from(_: Disconnected) -> Self {
        Self::Disconnected
    }
}

impl From<Infallible> for PlayerError {
    fn from(_: Infallible) -> Self {
        unreachable!("Infallible cannot be constructed")
    }
}

// error documentation done above
#[allow(clippy::missing_errors_doc)]
impl Handle {
    fn send(&self, message: Message) -> Result<(), Disconnected> {
        self.handle.send(message).map_err(|_| Disconnected)
    }

    /// Make the player start playing `source`
    pub fn play(&self, source: impl Into<MediaSource>) -> Result<(), Disconnected> {
        self.send(Message::Play(source.into()))
    }

    /// Change the [`Player`]'s [`PlayState`] to `play_state`, ignoring the previous state.
    ///
    /// See [`Generic::set_state_unchecked`] for more information.
    pub fn set_state_unchecked(&self, play_state: PlayState) -> Result<(), Disconnected> {
        self.send(Message::SetState(play_state))
    }

    /// Change the [`Player`]'s [`PlayState`] to `play_state`, returning if it succeeds.
    ///
    /// See [`Generic::set_state`] for more information.
    pub fn set_state(&self, play_state: PlayState) -> Result<bool, Disconnected> {
        if self.play_state()?.is_stopped() && !play_state.is_stopped() {
            Ok(false)
        } else {
            self.send(Message::SetState(play_state))?;
            Ok(true)
        }
    }

    /// Change the [`Player`]'s [`PlayState`] to [`Paused`](PlayState::Paused)
    pub fn pause(&self) -> Result<bool, Disconnected> {
        self.set_state(PlayState::Paused)
    }

    /// Change the [`Player`]'s [`PlayState`] to [`Playing`](PlayState::Playing)
    pub fn resume(&self) -> Result<bool, Disconnected> {
        self.set_state(PlayState::Playing)
    }

    /// Change the [`Player`]'s [`PlayState`] to [`Stopped`](PlayState::Stopped)
    pub fn stop(&self) -> Result<(), Disconnected> {
        // stopping will never fail
        self.send(Message::SetState(PlayState::Stopped))
    }

    /// Change the [`Player`]'s volume to `volume`
    pub fn set_volume(&self, volume: f64) -> Result<(), Disconnected> {
        self.send(Message::SetVolume(volume))
    }

    /// Get a value from the [`Shared`] reference,
    /// or return [`Disconnected`] if it's dropped
    fn get<T>(&self, func: impl FnOnce(&Shared) -> T) -> Result<T, Disconnected> {
        self.shared
            .upgrade()
            .map_or(Err(Disconnected), |shared| Ok(func(&shared)))
    }

    /// Seek the underlying [`AudioStream`] to `duration` from the start
    pub fn seek_to(&self, duration: Duration) -> Result<(), Disconnected> {
        self.send(Message::SeekTo(duration))
    }

    /// Seek the underlying [`AudioStream`] a certain `duration` in `direction` from the current position
    pub fn seek_by(&self, duration: Duration, direction: Direction) -> Result<(), Disconnected> {
        self.send(Message::SeekBy(duration, direction))
    }

    /// Read the shared [`StreamTimes`], mapping it using `func` if it exists or returning [`None`] otherwise
    fn map_times<T>(
        &self,
        func: impl FnOnce(&Arc<dyn StreamTimes>) -> T,
    ) -> Result<Option<T>, Disconnected> {
        self.get(|shared| {
            (shared.times.read()).map_or(Err(Disconnected), |times_opt| {
                Ok(times_opt.as_ref().map(func))
            })
        })?
    }

    /// Returns `true` if the [`Player`] has disconnected
    #[must_use]
    pub fn disconnected(&self) -> bool {
        self.shared.strong_count() == 0
    }

    /// Returns [`Err(Disconnected)`] if the [`Player`] has disconnected
    fn check_connection(&self) -> Result<(), Disconnected> {
        if self.disconnected() {
            Err(Disconnected)
        } else {
            Ok(())
        }
    }
}

impl Generic for Handle {
    type ModifyError = Disconnected;
    type GetError = Disconnected;

    fn play(&mut self, source: &MediaSource) -> Result<(), Self::ModifyError> {
        Self::play(self, source.clone())
    }

    fn set_state_unchecked(&mut self, play_state: PlayState) -> Result<(), Self::ModifyError> {
        Self::set_state_unchecked(self, play_state)
    }

    fn set_state(&mut self, play_state: PlayState) -> Result<bool, Self::ModifyError> {
        Self::set_state(self, play_state)
    }

    fn set_volume(&mut self, volume: f64) -> Result<(), Self::ModifyError> {
        Self::set_volume(self, volume)
    }

    fn seek_to(&mut self, duration: Duration) -> Result<(), Self::ModifyError> {
        Self::seek_to(self, duration)
    }

    fn seek_by(
        &mut self,
        duration: Duration,
        direction: Direction,
    ) -> Result<(), Self::ModifyError> {
        Self::seek_by(self, duration, direction)
    }

    fn play_state(&self) -> Result<PlayState, Self::GetError> {
        self.get(|shared| shared.play_state.load())
    }

    fn volume(&self) -> Result<f64, Self::GetError> {
        self.get(|shared| shared.volume.load())
    }

    fn position(&self) -> Result<Option<Duration>, Self::GetError> {
        self.map_times(|times| times.position())
    }

    fn duration(&self) -> Result<Option<Duration>, Self::GetError> {
        self.map_times(|times| times.duration())
    }

    fn times(&self) -> Result<Option<Arc<dyn StreamTimes>>, Self::GetError> {
        self.map_times(Arc::clone)
    }

    fn progress(&self) -> Result<Option<f64>, Self::GetError> {
        self.map_times(|times| times.progress())
    }

    fn synchronize(&self) -> Result<(), Self::ModifyError> {
        let wait_group = WaitGroup::new();
        self.send(Message::Synchronize(wait_group.clone()))?;
        wait_group.wait();
        // check if the player has disconnected, since it could've between sending and waiting
        self.check_connection()
    }

    fn supported_extensions(&self) -> Result<&ExtensionSet, Self::GetError> {
        self.check_connection()?;
        Ok(&self.supported_extensions)
    }
}

/// The current state of playing audio in a [`Player`]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PlayState {
    /// Audio is playing; packets are being decoded and sent to an output device
    Playing,
    /// Audio is not playing, but there is still a song in the player, and the output device is
    /// alive
    Paused,
    /// audio is not playing, there is no song stored in the player, and the output
    /// device is sleeping (if possible)
    Stopped,
}

impl PlayState {
    /// Returns `true` if the play state is [`Playing`].
    ///
    /// [`Playing`]: PlayState::Playing
    #[must_use]
    pub const fn is_playing(self) -> bool {
        matches!(self, Self::Playing)
    }

    /// Returns `true` if the play state is [`Paused`].
    ///
    /// [`Paused`]: PlayState::Paused
    #[must_use]
    pub const fn is_paused(self) -> bool {
        matches!(self, Self::Paused)
    }

    /// Returns `true` if the play state is [`Stopped`].
    ///
    /// [`Stopped`]: PlayState::Stopped
    #[must_use]
    pub const fn is_stopped(self) -> bool {
        matches!(self, Self::Stopped)
    }
}

impl Default for PlayState {
    fn default() -> Self {
        Self::Stopped
    }
}

/// Any error that can be occurred while the player is running
#[derive(Debug, Error)]
// see [`crate::output::OutputError`] for justification
#[allow(clippy::module_name_repetitions)]
pub enum PlayerError {
    /// The player encountered an error when outputting audio
    #[error("while playing audio: {0}")]
    Output(OutputError),
    /// The player encountered an error when decoding a file
    #[error("while decoding file: {0}")]
    Decoder(DecoderError),
    /// The output thread disconnected before it should have
    #[error("audio player disconnected")]
    OutputDisconnected,
    /// The player has disconnected
    #[error("player disconnected")]
    Disconnected,
    /// The player was told to exit
    #[error("the player was told to exit")]
    Exit,
}

impl PlayerError {
    #[must_use]
    pub const fn log_level(&self) -> log::Level {
        match self {
            Self::Output(out) => out.log_level(),
            Self::Decoder(decoder) => decoder.log_level(),
            Self::Exit => log::Level::Trace,
            Self::OutputDisconnected => log::Level::Warn,
            Self::Disconnected => log::Level::Error,
        }
    }
}

impl From<DecoderError> for PlayerError {
    fn from(v: DecoderError) -> Self {
        Self::Decoder(v)
    }
}

impl From<OutputError> for PlayerError {
    fn from(v: OutputError) -> Self {
        Self::Output(v)
    }
}

/// A result of an operation on a [`Player`]
// see [`crate::output::OutputError`] for justification
#[allow(clippy::module_name_repetitions)]
pub type PlayerResult<T> = Result<T, PlayerError>;

#[cfg(test)]
mod test {
    use crossbeam::atomic::AtomicCell;

    #[test]
    pub fn play_state_is_lock_free() {
        assert!(AtomicCell::<super::PlayState>::is_lock_free());
    }

    #[test]
    pub fn volume_is_lock_free() {
        assert!(AtomicCell::<f64>::is_lock_free());
    }

    #[test]
    pub fn duration_isnt_lock_free() {
        assert!(!AtomicCell::<super::Duration>::is_lock_free());
    }
}
