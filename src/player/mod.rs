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
//! handle.play("../test/test_file.flac")?;
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

/// Useful types for interacting with a [`Player`]
pub mod prelude {
    pub use super::{
        builder::Builder as PlayerBuilder, on_file_end, on_file_end::BoxedPlayer, Generic as _,
        Handle as PlayerHandle, PlayState, Player, PlayerError, PlayerResult,
    };
    pub use crate::audio::DeviceOptions;
    pub use crate::data::prelude::*;
    pub use crate::decoder::Direction;
    pub use crate::effect::prelude::*;
    pub use std::time::Duration;
}

use std::convert::Infallible;
use std::ops::ControlFlow;
use std::sync::{Arc, RwLock, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam::atomic::AtomicCell;
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use log::error;
use thiserror::Error;

use crate::audio::prelude::*;
use crate::decoder::{prelude::*, Direction};
use crate::effect::prelude::*;

use self::audio::PacketPlayer;
use self::builder::{Builder, DefaultAudio, DefaultDecoder, DefaultEffect};
use self::decoder::{NoPacket, PlayerDecoder};
use self::on_file_end::OnFileEnd;

mod audio;
pub mod builder;
mod decoder;
pub mod on_file_end;

#[derive(Debug)]
enum Message {
    Play(MediaSource),
    SetState(PlayState),
    SetVolume(f64),
    SeekTo(Duration),
    SeekBy(Duration, Direction),
}

#[derive(Debug)]
enum AudioControl {
    Flush,
    SetState(PlayState),
    SetVolume(f64),
}

/// A generic form of a reference to a [`Player`]
///
/// [`Handle`] is a notable implementor of this, and [`OnFileEnd`] gives this as a parameter.
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

    /// Change the [`Player`]'s [`PlayState`] to `play_state`
    ///
    /// # Errors
    ///
    /// - [`Player`]
    ///     - When [stopping](PlayState::Stopped):
    ///         - If [pausing the device](Device::pause) fails
    ///         - If [seeking the stream](AudioStream::seek_to) fails
    ///     - If the audio [`Device`] has an error
    /// - [`Handle`]
    ///     - If the player disconnected
    fn set_state(&mut self, play_state: PlayState) -> Result<(), Self::ModifyError>;

    /// Change the [`Player`]'s [`PlayState`] to [`Paused`](PlayState::Paused)
    ///
    /// # Errors
    ///
    /// - See [`Self::set_state`]
    fn pause(&mut self) -> Result<(), Self::ModifyError> {
        self.set_state(PlayState::Paused)
    }

    /// Change the [`Player`]'s [`PlayState`] to [`Playing`](PlayState::Playing)
    ///
    /// # Errors
    ///
    /// - See [`Self::set_state`]
    fn resume(&mut self) -> Result<(), Self::ModifyError> {
        self.set_state(PlayState::Playing)
    }

    /// Change the [`Player`]'s [`PlayState`] to [`Stopped`](PlayState::Stopped)
    ///
    /// # Errors
    ///
    /// - See [`Self::set_state`]
    fn stop(&mut self) -> Result<(), Self::ModifyError> {
        self.set_state(PlayState::Stopped)
    }

    /// Change the [`Player`]'s volume to `volume`
    ///
    /// # Errors
    ///
    /// - [`Player`]
    ///     - If the audio [`Device`] has an error
    /// - [`Handle`]
    ///     - If the player disconnected
    fn set_volume(&mut self, volume: f64) -> Result<(), Self::ModifyError>;

    /// Seek the underlying [`AudioStream`] to `duration` from the start
    ///
    /// # Errors
    ///
    /// - [`Player`]
    ///     - If the audio [`Device`] has an error
    ///     - If [`AudioStream::seek_to`] has an error
    /// - [`Handle`]
    ///     - If the player disconnected
    fn seek_to(&mut self, duration: Duration) -> Result<(), Self::ModifyError>;

    /// Seek the underlying [`AudioStream`] a certain `duration` in `direction` from the current position
    ///
    /// # Errors
    ///
    /// - [`Player`]
    ///     - If the audio [`Device`] has an error
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

    /// Get the [position](Self::position) and [duration](Self::duration) of the current
    /// [`AudioStream`] in a tuple, or [`None`] if there is no stream playing.
    ///
    /// It is laid out as `(position, duration)`
    ///
    /// # Errors
    ///
    /// - [`Handle`]: If the player disconnected
    fn times(&self) -> Result<Option<(Duration, Duration)>, Self::GetError>;
}

impl<T: ?Sized + Generic> Generic for &mut T {
    type GetError = T::GetError;
    type ModifyError = T::ModifyError;

    fn play(&mut self, source: &MediaSource) -> Result<(), Self::ModifyError> {
        (**self).play(source)
    }

    fn set_state(&mut self, play_state: PlayState) -> Result<(), Self::ModifyError> {
        (**self).set_state(play_state)
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

    fn times(&self) -> Result<Option<(Duration, Duration)>, Self::GetError> {
        (**self).times()
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
    times: RwLock<Option<Box<dyn StreamTimes>>>,
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
/// The player routes sound packets obtained through the [`Decoder`] to the output [`Audio`],
/// applying an [`Effect`] if given. The player may also run a custom callback for when a file ends
/// through [`OnFileEnd`].
///
/// To obtain a [`Player`], see [`Builder`].
///
/// The player automatically exits when every [`Handle`] goes out of scope
#[must_use = "Player doesn't do anything unless it's run"]
pub struct Player<D: Decoder, E: Effect, A: Audio, O: OnFileEnd> {
    handle: Receiver<Message>,
    decoder: D,
    effects: E,
    audio: A,
    on_end: O,
    options: DeviceOptions,
    shared: Arc<Shared>,
}

impl
    Player<
        crate::decoder::Default,
        crate::effect::Default,
        crate::audio::Default,
        on_file_end::Default,
    >
{
    /// Construct a [`Builder`] filled with defaults.
    #[must_use]
    pub fn builder() -> Builder<DefaultDecoder, DefaultEffect, DefaultAudio, on_file_end::Default> {
        Builder::default()
    }
}

impl<D: Decoder, E: Effect, A: Audio, O: OnFileEnd> Player<D, E, A, O> {
    fn new(
        decoder: D,
        effects: E,
        audio: A,
        on_end: O,
        options: DeviceOptions,
        volume: f64,
    ) -> (Self, Handle) {
        let (sender, receiver) = crossbeam_channel::unbounded();
        let shared: Arc<Shared> = Arc::new(Shared::new(volume));
        let weak = Arc::downgrade(&shared);
        (
            Self {
                handle: receiver,
                decoder,
                effects,
                audio,
                options,
                shared,
                on_end,
            },
            Handle {
                handle: sender,
                shared: weak,
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
            let res = self.run_blocking();
            if let Err(err) = res {
                error!("player stopped due to error: {err}");
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
        // audio control is a rendevous to make sure that the decoder and audio player is on the
        // same page at all times
        let (audio_control, audio_control_reciever) = crossbeam_channel::bounded(0);

        let source = PacketPlayer::new(
            &self,
            packet_receiver,
            audio_control_reciever,
            self.shared.volume.load(),
        );
        let device = self.audio.start_paused(self.options.clone(), source)?;
        let decoder = PlayerDecoder::new(&self, packet_sender);

        let mut inner = Inner {
            play_state: PlayState::Stopped,
            device,
            decoder,
            audio_control,
            handle: &self.handle,
            shared: &self.shared,
            on_end: &self.on_end,
        };

        inner.run_blocking()
    }
}

macro_rules! recv_or_break {
    ($expr:expr => |$message:ident| $map:expr) => {
        match $expr.map_err(TryRecvError::from) {
            Ok($message) => $map?,
            Err(TryRecvError::Empty) => (),
            Err(TryRecvError::Disconnected) => return Ok(ControlFlow::Break(())),
        }
    };
}

#[allow(clippy::struct_field_names)] // "play state" is its own thing
struct Inner<'a, D: Decoder, O: OnFileEnd> {
    play_state: PlayState,
    device: Box<dyn Device>,
    decoder: PlayerDecoder<'a, D>,
    audio_control: Sender<AudioControl>,
    handle: &'a Receiver<Message>,
    shared: &'a Shared,
    on_end: &'a O,
}

impl<'a, D: Decoder, O: OnFileEnd> Inner<'a, D, O> {
    fn run_blocking(&mut self) -> PlayerResult<()> {
        while self.tick()?.is_continue() {}

        Ok(())
    }

    fn tick(&mut self) -> PlayerResult<ControlFlow<()>> {
        // if there's a message waiting, then handle it
        recv_or_break!(self.handle.try_recv() => |message| self.handle(message));

        if self.play_state.is_playing() {
            match self.decoder.send_next_packet()? {
                // no reason to block; it already blocked due to packet being sent
                Ok(()) => Ok(ControlFlow::Continue(())),
                // there wasn't a packet available, handle the reason
                Err(reason) => self.no_packet_available(&reason),
            }
        } else {
            self.block_until_message()
        }
    }

    fn no_packet_available(&mut self, reason: &NoPacket) -> PlayerResult<ControlFlow<()>> {
        // if the stream just ended, then run on_end
        if reason.is_stream_ended() {
            self.on_end.file_ended(&mut {
                // this dance is necessary so that rust knows to make a trait object here
                let obj: on_file_end::BoxedPlayer = Box::new(&mut *self);
                obj
            })?;
        }

        // no packet was sent, so we must block on a message
        // so that there isn't an infinite loop
        self.block_until_message()
    }

    fn block_until_message(&mut self) -> PlayerResult<ControlFlow<()>> {
        recv_or_break!(self.handle.recv() => |message| self.handle(message));
        Ok(ControlFlow::Continue(()))
    }

    fn handle(&mut self, message: Message) -> PlayerResult<()> {
        match message {
            Message::Play(source) => self.play(&source),
            Message::SetState(new) => self.set_state(new),
            Message::SetVolume(new) => self.set_volume(new),
            Message::SeekTo(pos) => self.seek_to(pos),
            Message::SeekBy(duration, direction) => self.seek_by(duration, direction),
        }
    }

    fn send_control(&self, message: AudioControl) -> PlayerResult<()> {
        self.audio_control
            .send(message)
            .map_err(|_| PlayerError::AudioDisconnected)
    }
}

impl<'a, D: Decoder, O: OnFileEnd> Generic for Inner<'a, D, O> {
    type ModifyError = PlayerError;
    type GetError = Infallible;

    fn play(&mut self, source: &MediaSource) -> PlayerResult<()> {
        // make sure it's playing
        self.set_state(PlayState::Playing)?;
        // flush the packets from the previous song
        self.send_control(AudioControl::Flush)?;
        // start playing a new one
        self.decoder.decode(source);
        // update the shared times
        if let Some(stream) = self.decoder.stream() {
            let mut times = (self.shared.times)
                .write()
                .map_err(|_| PlayerError::Disconnected)?;
            *times = Some(stream.times());
        }
        Ok(())
    }

    fn set_state(&mut self, new: PlayState) -> PlayerResult<()> {
        if self.play_state == new {
            return Ok(());
        }

        let playing_before = !self.play_state.is_stopped();
        let playing_after = !new.is_stopped();
        match (playing_before, playing_after) {
            (false, true) => self.device.resume()?,
            (true, false) => {
                self.device.pause()?;
                self.seek_to(Duration::ZERO)?;
            }
            // no need to update
            _ => (),
        }

        // update all the different play states
        self.play_state = new;
        // shared is a different play state so that this doesn't have to query it every packet
        self.shared.play_state.store(new);
        // the audio also has to know so it could send empty data
        self.send_control(AudioControl::SetState(new))?;
        Ok(())
    }

    fn set_volume(&mut self, new: f64) -> PlayerResult<()> {
        self.send_control(AudioControl::SetVolume(new))?;
        self.shared.volume.store(new);
        Ok(())
    }

    fn seek_to(&mut self, duration: Duration) -> PlayerResult<()> {
        self.decoder
            .modify_stream(|stream| stream.seek_to(duration))?;
        self.send_control(AudioControl::Flush)?;
        Ok(())
    }

    fn seek_by(&mut self, duration: Duration, direction: Direction) -> PlayerResult<()> {
        self.decoder
            .modify_stream(|stream| stream.seek_by(duration, direction))?;
        self.send_control(AudioControl::Flush)?;
        Ok(())
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

    fn times(&self) -> Result<Option<(Duration, Duration)>, Self::GetError> {
        Ok(self
            .decoder
            .stream()
            .map(|stream| (stream.position(), stream.duration())))
    }
}

impl<'a, D: Decoder, O: OnFileEnd> Drop for Inner<'a, D, O> {
    fn drop(&mut self) {
        // Stop the audio before dropping, since the sound source expects the audio_control sender
        // to never be disconnected. By stopping it, the sound source never looks at the sender.
        let _ = self
            .audio_control
            .send(AudioControl::SetState(PlayState::Stopped));
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
/// // it may take a bit for the player to recieve the message
/// sleep(Duration::from_millis(100));
/// // once the player starts playing, it changes to [`PlayState::Playing`]
/// assert_eq!(handle.play_state()?, PlayState::Playing);
///
/// // the handle can also pause and resume the player
/// handle.pause()?;
/// sleep(Duration::from_millis(100));
/// // and the play state changes as a result
/// assert_eq!(handle.play_state()?, PlayState::Paused);
/// # Ok(()) }
/// ```
#[derive(Clone)]
pub struct Handle {
    handle: Sender<Message>,
    // TODO: measure the cost of using Weak instead of Arc
    shared: Weak<Shared>,
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

    /// Change the [`Player`]'s [`PlayState`] to `play_state`
    pub fn set_state(&self, play_state: PlayState) -> Result<(), Disconnected> {
        self.send(Message::SetState(play_state))
    }

    /// Change the [`Player`]'s [`PlayState`] to [`Paused`](PlayState::Paused)
    pub fn pause(&self) -> Result<(), Disconnected> {
        self.set_state(PlayState::Paused)
    }

    /// Change the [`Player`]'s [`PlayState`] to [`Playing`](PlayState::Playing)
    pub fn resume(&self) -> Result<(), Disconnected> {
        self.set_state(PlayState::Playing)
    }

    /// Change the [`Player`]'s [`PlayState`] to [`Stopped`](PlayState::Stopped)
    pub fn stop(&self) -> Result<(), Disconnected> {
        self.set_state(PlayState::Stopped)
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
        func: impl FnOnce(&dyn StreamTimes) -> T,
    ) -> Result<Option<T>, Disconnected> {
        self.get(|shared| {
            (shared.times.read()).map_or(Err(Disconnected), |times_opt| {
                Ok(times_opt.as_deref().map(func))
            })
        })?
    }

    /// Returns `true` if the [`Player`] has disconnected
    #[must_use]
    pub fn disconnected(&self) -> bool {
        self.shared.strong_count() == 0
    }
}

impl Generic for Handle {
    type ModifyError = Disconnected;
    type GetError = Disconnected;

    fn play(&mut self, source: &MediaSource) -> Result<(), Self::ModifyError> {
        Self::play(self, source.clone())
    }

    fn set_state(&mut self, play_state: PlayState) -> Result<(), Self::ModifyError> {
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

    fn times(&self) -> Result<Option<(Duration, Duration)>, Self::GetError> {
        self.map_times(|times| (times.position(), times.duration()))
    }
}

/// The current state of playing audio in a [`Player`]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlayState {
    /// Audio is playing; packets are being decoded and sent to an audio device
    Playing,
    /// Audio is not playing, but the position of the player is still intact and the audio device
    /// is alive
    Paused,
    /// audio is not playing, the position of the player is reset to the beginning, and the audio
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
// see [`crate::audio::AudioError`] for justification
#[allow(clippy::module_name_repetitions)]
pub enum PlayerError {
    /// The player encountered an error when outputting audio
    #[error("while playing audio: {0}")]
    Audio(AudioError),
    /// The player encountered an error when decoding a file
    #[error("while decoding file: {0}")]
    Decoder(DecoderError),
    /// The audio thread disconnected before it should have
    #[error("audio player disconnected")]
    AudioDisconnected,
    /// The player has disconnected
    #[error("player disconnected")]
    Disconnected,
}

impl From<DecoderError> for PlayerError {
    fn from(v: DecoderError) -> Self {
        Self::Decoder(v)
    }
}

impl From<AudioError> for PlayerError {
    fn from(v: AudioError) -> Self {
        Self::Audio(v)
    }
}

/// A result of an operation on a [`Player`]
// see [`crate::audio::AudioError`] for justification
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
