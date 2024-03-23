use crossbeam_channel::Sender;

use super::callback::prelude::*;
use super::prelude::*;
use crate::{decoder::prelude::*, effect::Effect, output::Output};

pub enum NoPacket {
    NoStream,
    StreamEnded(SourceName),
}

impl NoPacket {
    pub fn try_into_stream_ended(self) -> Result<SourceName, Self> {
        if let Self::StreamEnded(v) = self {
            Ok(v)
        } else {
            Err(self)
        }
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
    pub fn new<O: Output, E: Effect, OE: OnError, OSE: OnStreamEnd>(
        player: &'a Player<O, D, E, OE, OSE>,
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

    // TODO: make NoPacket store the stream?
    fn next_packet(&mut self) -> PlayerResult<Result<GenericPacket, NoPacket>> {
        let Some(stream) = self.current_stream.as_mut() else {
            return Ok(Err(NoPacket::NoStream));
        };
        Ok(stream
            .next_packet()?
            .ok_or_else(|| NoPacket::StreamEnded(stream.source().clone())))
    }

    pub fn decode(&mut self, source: &MediaSource) -> PlayerResult<()> {
        let stream = self.decoder.read(source);
        self.current_stream = Some(stream?);
        Ok(())
    }

    /// Stop decoding the current file and return it
    pub fn stop(&mut self) -> Option<Box<dyn AudioStream>> {
        self.current_stream.take()
    }
}
