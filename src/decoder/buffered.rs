//! Utilities to automatically buffer a [`Decoder`](super::Decoder) or [`AudioStream`](super::AudioStream).
//!
//! Through buffering, each are each ensured to send a consistent amount of frames per each packet.
//!
//! # Examples
//!
//! ```
//! use sauti::decoder::prelude::*;
//! use sauti::test::prelude::*;
//! # fn main() -> DecoderResult<()> {
//!
//! // The stream of packets to input into the buffered decoder
//! // The buffer will normalize them to the same size
//! let packets = vec![
//!     SoundPacket::from_channels(&[&[1, 2, 3]], 44100),
//!     SoundPacket::from_channels(&[&[1, 2]], 44100),
//!     SoundPacket::from_channels(&[&[1, 2, 3, 4]], 44100),
//! ];
//! let packets = packets.into_iter().map(GenericPacket::from);
//!
//! // Provide the stream and wrap it in a buffered [`Decoder`]
//! let provider = Provider::provide(packets);
//! let buffered = buffered::Decoder::wrap(provider);
//!
//! // The provider ignores the [`MediaSource`], so send it an empty path
//! let mut stream = buffered.read(&MediaSource::from(""))?;
//! // Collect all the packets into a Vec
//! let packets: Vec<GenericPacket> = stream.iter().collect::<DecoderResult<_>>()?;
//! // Since the first packet had a length of three, all others are normalized to that length
//! // There are nine frames in total, so three packets of three frames each
//! assert!(
//!     packets.len() == 3 && packets.iter().all(|packet| packet.frames() == 3),
//!     "packets should all have three frames since the first packet had three"
//! );
//! # Ok(()) }
//! ```
use super::prelude::*;

/// A buffered [`Decoder`](super::Decoder)
///
/// Each [`AudioStream`](super::AudioStream) that is returned is [wrapped](AudioStream::wrap) in a
/// buffer.
///
/// See [`super`] for more information.
pub struct Decoder<D: super::Decoder> {
    decoder: D,
}

impl<D: super::Decoder> Decoder<D> {
    /// Wrap an unbuffered [`Decoder`](super::Decoder) in a buffer
    #[must_use]
    pub const fn wrap(decoder: D) -> Self {
        Self { decoder }
    }
}

impl<D: super::Decoder> super::Decoder for Decoder<D> {
    fn read(&self, source: &MediaSource) -> DecoderResult<Box<dyn super::AudioStream>> {
        Ok(Box::new(AudioStream::wrap(self.decoder.read(source)?)))
    }
}

/// A buffered [`AudioStream`](super::AudioStream)
///
/// See [`super`] for more information
pub struct AudioStream {
    inner: Box<dyn super::AudioStream>,
    current: Option<GenericPacket>,
    frames: Option<usize>,
}

impl AudioStream {
    /// Wrap an unbuffered [`AudioStream`](super::AudioStream) in a buffer
    #[must_use]
    pub const fn wrap(stream: Box<dyn super::AudioStream>) -> Self {
        Self {
            inner: stream,
            current: None,
            frames: None,
        }
    }

    fn extract_if_enough(
        current: &GenericPacket,
        frames: usize,
    ) -> Option<(GenericPacket, Option<GenericPacket>)> {
        (current.frames() >= frames).then(|| {
            let (truncated, rest) = current.split(frames);
            (truncated, (rest.frames() != 0).then_some(rest))
        })
    }

    fn next_non_empty_packet(&mut self) -> DecoderResult<Option<GenericPacket>> {
        loop {
            let Some(next) = self.inner.next_packet()? else {
                return Ok(None);
            };

            if next.frames() > 0 {
                break Ok(Some(next));
            }
        }
    }

    fn frames_or(&mut self, backup: usize) -> usize {
        if let Some(frames) = self.frames {
            frames
        } else {
            self.frames = Some(backup);
            backup
        }
    }
}

impl super::AudioStream for AudioStream {
    fn next_packet(&mut self) -> DecoderResult<Option<GenericPacket>> {
        // extract some from current if there's enough
        if let Some((extracted, rest)) = (self.current.as_ref().zip(self.frames.as_ref().copied()))
            .and_then(|(current, frames)| Self::extract_if_enough(current, frames))
        {
            self.current = rest;
            return Ok(Some(extracted));
        }

        // otherwise get the next non-empty packet from the stream
        let Some(packet) = self.next_non_empty_packet()? else {
            return Ok(self.current.take());
        };

        // find the wanted amount of frames
        let packet_frames = packet.frames();
        let frames = self.frames_or(packet_frames);

        // if the packet already matches the required frames, then just return it
        if self.current.is_none() && frames == packet_frames {
            return Ok(Some(packet));
        }

        // if the packet and current packets' specs collide, then replace
        if let Some(current) = &mut self.current {
            if current.spec() != packet.spec() {
                // might as well replace the frame count too since effects would have to be
                // restarted anyways
                self.frames = Some(packet.frames());
                let current = std::mem::replace(current, packet);
                return Ok(Some(current));
            }
        }

        // join the new packet onto the current one
        let mut current = (self.current.take())
            .map(|current| current.join(&packet))
            .unwrap_or(packet);

        // get enough frames to match the wanted amount
        loop {
            // if we have enough frames, cut them off from current and return them
            if let Some((extracted, rest)) = Self::extract_if_enough(&current, frames) {
                self.current = rest;
                return Ok(Some(extracted));
            }

            // get the next packet or give up
            let Some(new) = self.next_non_empty_packet()? else {
                return Ok(Some(current));
            };

            // if the specs mismatch, then just return the current packet
            if current.spec() != new.spec() {
                self.frames = Some(new.frames());
                self.current = Some(new);
                return Ok(Some(current));
            }

            // join the new packet onto the current and try again
            current = current.join(&new);
        }
    }

    fn seek_to(&mut self, duration: std::time::Duration) -> DecoderResult<()> {
        let res = self.inner.seek_to(duration);
        if res.is_ok() {
            self.current = None;
            self.frames = None;
            Ok(())
        } else {
            res
        }
    }

    fn position(&self) -> std::time::Duration {
        self.inner.position()
    }

    fn duration(&self) -> std::time::Duration {
        self.inner.duration()
    }

    fn times(&self) -> std::sync::Arc<dyn StreamTimes> {
        self.inner.times()
    }
}
