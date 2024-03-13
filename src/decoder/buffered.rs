//! Utilities to automatically buffer a [`Decoder`](super::Decoder) or [`AudioStream`](super::AudioStream).
//!
//! Through buffering, each are each ensured to send a consistent amount of frames per each packet.

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
        let frames = self.frames.unwrap_or_else(|| {
            let current_frames = current.frames();
            self.frames = Some(current_frames);
            current_frames
        });

        loop {
            // if we already have enough frames, cut them out from the current packet
            if current.frames() >= frames {
                let (truncated, rest) = current.split(frames);
                // rest can have zero frames, but that isn't a problem anymore since the frame
                // count should already be initialized above
                self.current = Some(rest);
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
