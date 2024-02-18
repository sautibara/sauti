use std::time::Duration;

use crate::decoder::prelude::*;

/// An implementation of [`Decoder`] that repeatedly provides a given packet
///
/// When [`Self::read`] is called, the given [`MediaSource`] will be ignored.
///
/// See the [higher module](super) for an example
#[derive(Clone)]
pub struct Provider {
    packet: GenericPacket,
}

impl Provider {
    #[must_use]
    pub fn provide(packet: impl Into<GenericPacket>) -> Self {
        let packet = packet.into();
        Self { packet }
    }
}

impl Decoder for Provider {
    fn read(&self, _source: &MediaSource) -> DecoderResult<Box<dyn AudioStream>> {
        Ok(Box::new(self.clone()))
    }
}

impl AudioStream for Provider {
    fn next_packet(&mut self) -> DecoderResult<Option<GenericPacket>> {
        Ok(Some(self.packet.clone()))
    }

    fn seek_to(&mut self, _duration: std::time::Duration) -> DecoderResult<()> {
        Ok(())
    }

    fn seek_by(
        &mut self,
        _duration: std::time::Duration,
        _direction: crate::decoder::Direction,
    ) -> DecoderResult<()> {
        Ok(())
    }

    fn position(&self) -> std::time::Duration {
        Duration::ZERO
    }

    fn duration(&self) -> Duration {
        let frames = self.packet.frames();
        let sample_rate = self.packet.spec().sample_rate;
        crate::decoder::frame_to_duration(frames, sample_rate)
    }
}
