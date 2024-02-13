pub mod prelude {
    pub use super::{builder::Builder as PlayerBuilder, Player, PlayerError, PlayerResult};
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

// TODO: break into multiple files

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
    pub fn new(decoder: D, effects: E, audio: A, options: DeviceOptions) -> (Self, Handle) {
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
        let (packet_sender, packet_receiver) = crossbeam_channel::bounded(2);
        let (audio_control, audio_control_reciever) = crossbeam_channel::bounded(2);

        // TODO: bring device and decode and such into state pls
        let source = PacketPlayer::new(&self, packet_receiver, audio_control_reciever);
        let mut device = self.audio.start(self.options.clone(), source)?;
        // device starts paused (stopped)
        device.pause()?;
        let decoder = PlayerDecoder::new(&self, packet_sender);

        let mut state = State {
            play_state: PlayState::Stopped,
            device,
            decoder,
            audio_control,
        };

        loop {
            let res = self.tick(&mut state);
            match res {
                Ok(ControlFlow::Continue(())) => (),
                Ok(ControlFlow::Break(())) => break,
                Err(err) => {
                    return Err(err);
                }
            }
        }

        Ok(())
    }

    fn tick(&self, state: &mut State<D>) -> PlayerResult<ControlFlow<()>> {
        match self.handle.try_recv() {
            Ok(message) => state.handle(message)?,
            Err(TryRecvError::Empty) => (),
            // if all handles have hung up, then break
            Err(TryRecvError::Disconnected) => return Ok(ControlFlow::Break(())),
        }

        // NOTE: this blocks until the packet is sent
        // if it doesn't send (and thus returns false),
        // then it blocks on the message reciever
        if !(state.play_state.is_playing() && state.decoder.send_next_packet()?) {
            let Ok(message) = self.handle.recv() else {
                // if all handles have hung up, then break
                return Ok(ControlFlow::Break(()));
            };
            state.handle(message)?;
        }

        Ok(ControlFlow::Continue(()))
    }
}

impl Player<crate::decoder::Default, crate::effect::Default, crate::audio::Default> {
    #[must_use]
    pub fn default_builder() -> Builder<DefaultDecoder, DefaultEffect, DefaultAudio> {
        Builder::default()
    }
}

#[allow(clippy::struct_field_names)] // "play state" is its own thing
struct State<'a, D: Decoder> {
    play_state: PlayState,
    device: Box<dyn Device>,
    decoder: PlayerDecoder<'a, D>,
    audio_control: Sender<AudioControl>,
}

impl<'a, D: Decoder> State<'a, D> {
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
        let stopped_before = self.play_state.is_stopped();
        let stopped_after = new.is_stopped();
        match (stopped_before, stopped_after) {
            (true, false) => self.device.resume()?,
            (false, true) => {
                self.seek_to(Duration::ZERO)?;
                self.device.pause()?;
            }
            // no need to update
            _ => (),
        }
        self.play_state = new;
        Ok(())
    }

    fn seek_to(&mut self, duration: Duration) -> PlayerResult<()> {
        self.decoder
            .modify_stream(|stream| stream.seek_to(duration))?;
        self.flush_packets()?;
        Ok(())
    }

    fn flush_packets(&mut self) -> PlayerResult<()> {
        self.audio_control
            .send(AudioControl::Flush)
            .map_err(|_| PlayerError::AudioDisconnected)
    }
}

#[derive(Clone, Copy, Debug)]
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

#[derive(Debug)]
enum Message {
    Play(MediaSource),
    SetState(PlayState),
}

#[derive(Debug)]
enum AudioControl {
    Flush,
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
