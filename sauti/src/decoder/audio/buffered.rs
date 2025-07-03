//! Utilities to automatically buffer a [`Decoder`](super::Decoder) or [`AudioStream`](super::AudioStream).
//!
//! Through buffering, each are each ensured to send a consistent amount of frames per each packet.
//!
//! # Examples
//!
//! ```
//! use sauti::decoder::audio::prelude::*;
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
use crate::decoder::audio::prelude::*;

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

    fn supported_extensions(&self) -> ExtensionSet {
        self.decoder.supported_extensions()
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

    /// Get the next non-empty packet in the stream
    fn next_packet(&mut self) -> DecoderResult<Option<GenericPacket>> {
        loop {
            let Some(next) = self.inner.next_packet()? else {
                break Ok(None);
            };

            if next.frames() > 0 {
                break Ok(Some(next));
            }
        }
    }
}

impl super::AudioStream for AudioStream {
    fn next_packet(&mut self) -> DecoderResult<Option<GenericPacket>> {
        // get the current packet or take a new one
        let current: Option<DecoderResult<_>> =
            (self.current.take().map(Ok)).or_else(|| self.next_packet().transpose());
        // if there are no packets left in the stream, return [`None`]
        let Some(current) = current else {
            return Ok(None);
        };
        // return any errors if they exist
        let mut current: GenericPacket = current?;

        // get the current frame count or use the frames in the current packet
        let frames = *self.frames.get_or_insert_with(|| current.frames());

        loop {
            // if we already have enough frames, cut them out from the current packet
            if current.frames() >= frames {
                let (truncated, rest) = current.split(frames);
                self.current = (rest.frames() != 0).then_some(rest);
                return Ok(Some(truncated));
            }

            // otherwise, get a new packet from the stream
            let Some(new) = self.next_packet()? else {
                return Ok(Some(current));
            };

            // if the specs mismatch, just return the current packet and update the specs
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

    fn source(&self) -> &SourceName {
        self.inner.source()
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
