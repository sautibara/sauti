use crossbeam_channel::Sender;
use log::error;

use super::{on_file_end::OnFileEnd, prelude::*};
use crate::{decoder::prelude::*, effect::Effect, output::Output};

pub enum NoPacket {
    NoStream,
    StreamEnded,
}

impl NoPacket {
    /// Returns `true` if the no packet is [`StreamEnded`].
    ///
    /// [`StreamEnded`]: NoPacket::StreamEnded
    #[must_use]
    pub const fn is_stream_ended(&self) -> bool {
        matches!(self, Self::StreamEnded)
    }
}

// it conflicts with the other decoder module
#[allow(clippy::module_name_repetitions)]
pub struct PlayerDecoder<'a, D: Decoder> {
    packet_sender: Sender<GenericPacket>,
    decoder: &'a D,
    current_stream: Option<Box<dyn AudioStream>>,
}

impl<'a, D: Decoder> PlayerDecoder<'a, D> {
    pub fn new<O: Output, E: Effect, C: OnFileEnd>(
        player: &'a Player<O, D, E, C>,
        packet_sender: Sender<GenericPacket>,
    ) -> Self {
        Self {
            decoder: &player.decoder,
            packet_sender,
            current_stream: None,
        }
    }

    pub fn modify_stream<E>(
        &mut self,
        func: impl FnOnce(&mut Box<dyn AudioStream>) -> Result<(), E>,
    ) -> PlayerResult<()>
    where
        PlayerError: From<E>,
    {
        if let Some(stream) = &mut self.current_stream {
            func(stream)?;
        }
        Ok(())
    }

    pub fn stream(&self) -> Option<&dyn AudioStream> {
        self.current_stream.as_deref()
    }

    pub fn send_next_packet(&mut self) -> PlayerResult<Result<(), NoPacket>> {
        let packet = match self.next_packet()? {
            Ok(packet) => packet,
            Err(no_packet) => return Ok(Err(no_packet)),
        };

        match self.packet_sender.send(packet) {
            // something was sent
            Ok(()) => Ok(Ok(())),
            Err(_) => Err(PlayerError::OutputDisconnected),
        }
    }

    fn next_packet(&mut self) -> PlayerResult<Result<GenericPacket, NoPacket>> {
        let Some(stream) = self.current_stream.as_mut() else {
            return Ok(Err(NoPacket::NoStream));
        };
        Ok(stream.next_packet()?.ok_or(NoPacket::StreamEnded))
    }

    pub fn decode(&mut self, source: &MediaSource) {
        let stream = self.decoder.read(source);
        let Ok(stream) = stream else {
            error!("failed to decode source: {source}");
            return;
        };

        self.current_stream = Some(stream);
    }
}
