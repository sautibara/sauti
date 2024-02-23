//! A single-track audio player

/// Useful types for interacting with a [`Player`]
pub mod prelude {
    pub use super::{
        builder::Builder as PlayerBuilder, PlayState, Player, PlayerError, PlayerResult,
    };
    pub use crate::audio::DeviceOptions;
    pub use crate::data::prelude::*;
    pub use crate::effect::prelude::*;
}

use std::ops::ControlFlow;
use std::sync::{Arc, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam::atomic::AtomicCell;
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use log::error;
use thiserror::Error;

use crate::audio::prelude::*;
use crate::decoder::prelude::*;
use crate::effect::prelude::*;

use self::audio::PacketPlayer;
use self::builder::{Builder, DefaultAudio, DefaultDecoder, DefaultEffect};
use self::decoder::PlayerDecoder;

mod audio;
pub mod builder;
mod decoder;

#[derive(Debug)]
enum Message {
    Play(MediaSource),
    SetState(PlayState),
    SetVolume(f64),
}

#[derive(Debug)]
enum AudioControl {
    Flush,
    SetState(PlayState),
    SetVolume(f64),
}

struct Shared {
    play_state: AtomicCell<PlayState>,
    volume: AtomicCell<f64>,
}

impl Shared {
    fn new(volume: f64) -> Self {
        Self {
            play_state: AtomicCell::default(),
            volume: AtomicCell::new(volume),
        }
    }
}

#[derive(Clone)]
#[must_use = "Player doesn't do anything unless it's run"]
pub struct Player<D: Decoder, E: Effect, A: Audio> {
    handle: Receiver<Message>,
    decoder: D,
    effects: E,
    audio: A,
    options: DeviceOptions,
    shared: Arc<Shared>,
}

impl Player<crate::decoder::Default, crate::effect::Default, crate::audio::Default> {
    #[must_use]
    pub fn default_builder() -> Builder<DefaultDecoder, DefaultEffect, DefaultAudio> {
        Builder::default()
    }
}

impl<D: Decoder, E: Effect, A: Audio> Player<D, E, A> {
    fn new(
        decoder: D,
        effects: E,
        audio: A,
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
            },
            Handle {
                handle: sender,
                shared: weak,
            },
        )
    }

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
struct Inner<'a, D: Decoder> {
    play_state: PlayState,
    device: Box<dyn Device>,
    decoder: PlayerDecoder<'a, D>,
    audio_control: Sender<AudioControl>,
    handle: &'a Receiver<Message>,
    shared: &'a Shared,
}

impl<'a, D: Decoder> Inner<'a, D> {
    fn run_blocking(&mut self) -> PlayerResult<()> {
        while self.tick()?.is_continue() {}

        Ok(())
    }

    fn tick(&mut self) -> PlayerResult<ControlFlow<()>> {
        // if there's a message waiting, then handle it
        recv_or_break!(self.handle.try_recv() => |message| self.handle(message));

        // NOTE: this blocks until the packet is sent
        // if it doesn't send (and thus returns false),
        // then it blocks on the message reciever instead
        // TODO: stop after file finishes
        if !(self.play_state.is_playing() && self.decoder.send_next_packet()?) {
            recv_or_break!(self.handle.recv() => |message| self.handle(message));
        }

        Ok(ControlFlow::Continue(()))
    }

    fn handle(&mut self, message: Message) -> PlayerResult<()> {
        match message {
            Message::Play(source) => {
                // make sure it's playing
                self.update_play_state(PlayState::Playing)?;
                // flush the packets from the previous song
                self.send_control(AudioControl::Flush)?;
                // start playing a new one
                self.decoder.decode(&source);
            }
            Message::SetState(new) => self.update_play_state(new)?,
            Message::SetVolume(new) => {
                self.send_control(AudioControl::SetVolume(new))?;
                self.shared.volume.store(new);
            }
        }
        Ok(())
    }

    fn update_play_state(&mut self, new: PlayState) -> PlayerResult<()> {
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

    fn seek_to(&mut self, duration: Duration) -> PlayerResult<()> {
        self.decoder
            .modify_stream(|stream| stream.seek_to(duration))?;
        self.send_control(AudioControl::Flush)?;
        Ok(())
    }

    fn send_control(&self, message: AudioControl) -> PlayerResult<()> {
        self.audio_control
            .send(message)
            .map_err(|_| PlayerError::AudioDisconnected)
    }
}

/// A handle to a [`Player`] that could control it or query its info
///
/// # Errors
///
/// If the player disconnects, then all methods will return [`Err(Disconnected)`](Disconnected)
///
/// # Examples
///
/// ```
/// use sauti::player::prelude::*;
/// use sauti::test::prelude::*;
/// use std::time::Duration;
/// use std::thread::sleep;
/// # fn main() -> Result<(), sauti::player::Disconnected> {
///
/// // create a new player that ignores audio
/// let handle = Empty::player().run();
///
/// // start playing an imaginary file
/// // [`Empty`] ignores the [`MediaSource`], so just send an empty path
/// handle.play("")?;
/// // it may take a bit for the player to recieve the message
/// sleep(Duration::from_millis(100));
/// // once the player starts playing, it changes to [`PlayState::Playing`]
/// assert_eq!(handle.play_state()?, PlayState::Playing);
///
/// // the handle can also pause
/// handle.pause()?;
/// sleep(Duration::from_millis(100));
/// assert_eq!(handle.play_state()?, PlayState::Paused);
///
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Handle {
    handle: Sender<Message>,
    // TODO: measure the cost of this
    shared: Weak<Shared>,
}

/// A [`Handle`] could not connect to its respective [`Player`]
#[derive(Error, Debug)]
#[error("player was disconnected")]
pub struct Disconnected;

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

    /// Get the current play state of the [`Player`]
    pub fn play_state(&self) -> Result<PlayState, Disconnected> {
        self.get(|shared| shared.play_state.load())
    }

    /// Get the current volume of the [`Player`]
    pub fn volume(&self) -> Result<f64, Disconnected> {
        self.get(|shared| shared.volume.load())
    }

    /// Returns `true` if the [`Player`] has disconnected
    #[must_use]
    pub fn disconnected(&self) -> bool {
        self.shared.strong_count() == 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlayState {
    Playing,
    Paused,
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

#[derive(Debug, Error)]
// see [`crate::audio::AudioError`] for justification
#[allow(clippy::module_name_repetitions)]
pub enum PlayerError {
    #[error("while playing audio: {0}")]
    Audio(AudioError),
    #[error("while decoding file: {0}")]
    Decoder(DecoderError),
    #[error("audio player disconnected")]
    AudioDisconnected,
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
}
