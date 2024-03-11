use std::{
    iter::{once, repeat, Once, Repeat},
    sync::Arc,
    time::Duration,
};

use crate::decoder::prelude::*;

use super::Empty;

/// An implementation of [`Decoder`] that repeatedly provides a given packet
///
/// When [`Self::read`] is called, the given [`MediaSource`] will be ignored.
///
/// See the [higher module](super) for an example
#[derive(Clone)]
pub struct Provider<I: Iterator<Item = GenericPacket> + Clone + Send> {
    packets: I,
}

impl Provider<Once<GenericPacket>> {
    /// Provide this `packet` only once, returning None afterwards
    #[must_use]
    pub fn once(packet: impl Into<GenericPacket>) -> Self {
        Self {
            packets: once(packet.into()),
        }
    }
}

impl Provider<Repeat<GenericPacket>> {
    /// Repeatedly provide this `packet`, returning it every time
    #[must_use]
    pub fn repeat(packet: impl Into<GenericPacket>) -> Self {
        Self {
            packets: repeat(packet.into()),
        }
    }
}

impl<I: Iterator<Item = GenericPacket> + Clone + Send + 'static> Provider<I> {
    /// Provide an iterator of packets, ending when the iterator ends
    #[must_use]
    pub fn provide(iter: impl IntoIterator<Item = GenericPacket, IntoIter = I>) -> Self {
        Self {
            packets: iter.into_iter(),
        }
    }
}

impl<I: Iterator<Item = GenericPacket> + Clone + Send + 'static> Decoder for Provider<I> {
    fn read(&self, _source: &MediaSource) -> DecoderResult<Box<dyn AudioStream>> {
        Ok(Box::new(self.clone()))
    }
}

impl<I: Iterator<Item = GenericPacket> + Clone + Send + 'static> AudioStream for Provider<I> {
    fn next_packet(&mut self) -> DecoderResult<Option<GenericPacket>> {
        Ok(self.packets.next())
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
        Duration::ZERO
    }

    fn times(&self) -> Arc<dyn StreamTimes> {
        Arc::new(Empty)
    }
}
