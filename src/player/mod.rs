pub mod prelude {
    pub use super::{builder::Builder as PlayerBuilder, Player};
    pub use crate::data::prelude::*;
    pub use crate::effect::prelude::*;
}

use std::ops::ControlFlow;
use std::thread::JoinHandle;

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
                error!("failed to start player: {err}");
            }
        })
    }

    /// # Errors
    ///
    /// - If there was an issue starting audio output
    pub fn run_blocking(self) -> Result<(), AudioError> {
        let (packet_sender, packet_receiver) = crossbeam_channel::bounded(1);

        let source = PacketPlayer::new(&self, packet_receiver);
        let mut device = self.audio.start(self.options.clone(), source)?;
        let mut decode = PlayerDecoder::new(&self, packet_sender);

        let mut state = State::default();
        while self
            .tick(&mut device, &mut decode, &mut state)
            .is_continue()
        {}

        Ok(())
    }

    fn tick(
        &self,
        _device: &mut Box<dyn Device>,
        decoder: &mut PlayerDecoder<D>,
        state: &mut State,
    ) -> ControlFlow<()> {
        match self.handle.try_recv() {
            Ok(message) => Self::handle(message, decoder, state),
            Err(TryRecvError::Empty) => (),
            Err(TryRecvError::Disconnected) => return ControlFlow::Break(()),
        }

        // NOTE: this blocks until the packet is sent
        // if it doesn't send (and thus returns false),
        // then it blocks on the message reciever
        if !(state.playing && decoder.send_next_packet()) {
            let Ok(message) = self.handle.recv() else {
                return ControlFlow::Break(());
            };
            Self::handle(message, decoder, state);
        }

        ControlFlow::Continue(())
    }

    fn handle(message: Message, decoder: &mut PlayerDecoder<D>, state: &mut State) {
        match message {
            Message::Play(source) => decoder.decode(&source),
            Message::Pause => state.playing = false,
            Message::Resume => state.playing = true,
        }
    }
}

impl Player<crate::decoder::Default, crate::effect::Default, crate::audio::Default> {
    #[must_use]
    pub fn default_builder() -> Builder<DefaultDecoder, DefaultEffect, DefaultAudio> {
        Builder::default()
    }
}

struct State {
    playing: bool,
}

impl Default for State {
    fn default() -> Self {
        Self { playing: true }
    }
}

enum Message {
    Play(MediaSource),
    Pause,
    Resume,
}

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
        send!(self, Message::Pause)
    }

    /// # Errors
    ///
    /// - If the player is disconnected
    pub fn resume(&self) -> Result<(), Disconnected> {
        send!(self, Message::Resume)
    }
}
