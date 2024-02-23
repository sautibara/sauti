pub mod prelude {
    pub use super::{
        builder::Builder as PlayerBuilder, PlayState, Player, PlayerError, PlayerResult,
    };
    pub use crate::audio::DeviceOptions;
    pub use crate::data::prelude::*;
    pub use crate::effect::prelude::*;
}

use std::ops::ControlFlow;
use std::thread::JoinHandle;
use std::time::Duration;

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
pub enum Message {
    Play(MediaSource),
    SetState(PlayState),
}

#[derive(Debug)]
pub enum AudioControl {
    Flush,
    SetState(PlayState),
}

#[derive(Clone)]
#[must_use = "Player doesn't do anything unless it's run"]
pub struct Player<D: Decoder, E: Effect, A: Audio> {
    handle: Receiver<Message>,
    decoder: D,
    effects: E,
    audio: A,
    options: DeviceOptions,
}

impl<D: Decoder, E: Effect, A: Audio> Player<D, E, A> {
    fn new(decoder: D, effects: E, audio: A, options: DeviceOptions) -> (Self, Handle) {
        let (sender, receiver) = crossbeam_channel::unbounded();
        (
            Self {
                handle: receiver,
                decoder,
                effects,
                audio,
                options,
            },
            Handle { handle: sender },
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

        let source = PacketPlayer::new(&self, packet_receiver, audio_control_reciever);
        let device = self.audio.start_paused(self.options.clone(), source)?;
        let decoder = PlayerDecoder::new(&self, packet_sender);

        let mut inner = Inner {
            play_state: PlayState::Stopped,
            device,
            decoder,
            audio_control,
            handle: &self.handle,
        };

        inner.run_blocking()
    }
}

impl Player<crate::decoder::Default, crate::effect::Default, crate::audio::Default> {
    #[must_use]
    pub fn default_builder() -> Builder<DefaultDecoder, DefaultEffect, DefaultAudio> {
        Builder::default()
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
                self.decoder.decode(&source);
                // make sure it's playing
                self.update_play_state(PlayState::Playing)?;
            }
            Message::SetState(new) => self.update_play_state(new)?,
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

        self.play_state = new;
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

#[derive(Clone)]
pub struct Handle {
    handle: Sender<Message>,
}

/// The handle could not send a message because it was disconnected
#[derive(Error, Debug)]
#[error("player was disconnected")]
pub struct Disconnected;

macro_rules! send {
    ($self:ident, $val:expr) => {
        $self.handle.send($val).map_err(|_| Disconnected)
    };
}

impl Handle {
    /// # Errors
    ///
    /// - If the player is disconnected
    pub fn play(&self, source: impl Into<MediaSource>) -> Result<(), Disconnected> {
        send!(self, Message::Play(source.into()))
    }

    /// # Errors
    ///
    /// - If the player is disconnected
    pub fn pause(&self) -> Result<(), Disconnected> {
        send!(self, Message::SetState(PlayState::Paused))
    }

    /// # Errors
    ///
    /// - If the player is disconnected
    pub fn resume(&self) -> Result<(), Disconnected> {
        send!(self, Message::SetState(PlayState::Playing))
    }

    /// # Errors
    ///
    /// - If the player is disconnected
    pub fn stop(&self) -> Result<(), Disconnected> {
        send!(self, Message::SetState(PlayState::Stopped))
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
