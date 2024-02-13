use crossbeam_channel::Sender;
use log::error;

use super::prelude::*;
use crate::decoder::prelude::*;

// it conflicts with the other decoder module
#[allow(clippy::module_name_repetitions)]
pub struct PlayerDecoder<'a, D: Decoder> {
    packet_sender: Sender<GenericPacket>,
    decoder: &'a D,
    current_stream: Option<Box<dyn AudioStream>>,
}

impl<'a, D: Decoder> PlayerDecoder<'a, D> {
    pub fn new<E: crate::effect::Effect, A: crate::audio::Audio>(
        player: &'a Player<D, E, A>,
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

    pub fn send_next_packet(&mut self) -> PlayerResult<bool> {
        let Some(packet) = self.next_packet() else {
            // nothing was sent
            return Ok(false);
        };

        match self.packet_sender.send(packet) {
            // something was sent
            Ok(()) => Ok(true),
            Err(_) => Err(PlayerError::AudioDisconnected),
        }
    }

    fn next_packet(&mut self) -> Option<GenericPacket> {
        let stream = self.current_stream.as_mut()?;
        let res = stream.next_packet();
        if let Err(err) = &res {
            error!("error found while decoding: {err:?}");
        }
        res.ok()?
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
