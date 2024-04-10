//! Decoding of audio files
//!
//! To decode a file, use a [`Decoder`] to obtain an [`AudioStream`] with [`Decoder::read`].
//! This stream can then be queried for packets using [`next_packet`](AudioStream::next_packet),
//! or the entire stream can be decoded using [`decode_all`](AudioStreamExt::decode_all).
//! The stream can also be [seeked](AudioStream::seek_to) or queried for its
//! [position](AudioStream::position) or [duration](AudioStream::duration).
//! To get the default decoder, use [`self::default`].
//!
//! # Examples
//!
//! ```
//! use sauti::decoder::prelude::*;
//!
//! let decoder = sauti::decoder::default();
//!
//! // the test file holds a 22050hz square wave (switches every sample)
//! let source = MediaSource::copy_buf(include_bytes!("../test/test_file.flac"));
//! let file = (decoder.read(&source))
//!     .and_then(|mut file| file.decode_all())
//!     .expect("failed to decode file")
//!     .expect("file should have at least one packet");
//!
//! let (max, min) = (i8::MAX, i8::MIN);
//! assert_eq!(
//!     file.convert::<i8>(),
//!     SoundPacket::from_channels(&[&[max, min, max, min]], 44100)
//! );
//! ```

use std::{default::Default as _, fmt::Debug, sync::Arc, time::Duration};

use thiserror::Error;

use crate::data::GenericPacket;

pub mod buffered;
mod symphonia;

/// Useful types for interacting with a [`Decoder`]
pub mod prelude {
    pub use super::{
        buffered, AudioStream, AudioStreamExt, Decoder, DecoderError, DecoderResult, StreamTimes,
    };
    pub use crate::data::prelude::*;
}

/// A decoder implemented using [`symphonia`](::symphonia)
pub use symphonia::Symphonia;

use self::prelude::*;

/// Get the default [`Decoder`]
#[must_use]
pub fn default() -> self::Default {
    Symphonia::default()
}

/// The output type of [`default`]
pub type Default = Symphonia;

/// A type that can [`read`](Self::read) and decode a [`MediaSource`], returning [`AudioStream`]
///
/// # Implementors
///
/// [`Self::read`] and [`Self::read_fallible`] are reflexively defined in terms of each other. It's
/// expected for the implementor to implement one of them and let the default implementation handle
/// the other.
pub trait Decoder: Send + 'static {
    /// Try to decode and read this file, returning `Ok(None)` if the format isn't supported
    ///
    /// # Errors
    ///
    /// - If there is some error with IO
    /// - If there is a backend-specific error
    fn read_fallible(&self, source: &MediaSource) -> DecoderResult<Option<Box<dyn AudioStream>>> {
        let res = self.read(source);
        if matches!(res, Err(DecoderError::UnsupportedFormat { .. })) {
            Ok(None)
        } else {
            res.map(Some)
        }
    }

    /// Try to decode and read this file, returning `Err(UnsupportedFormat)` if the format isn't supported
    ///
    /// # Errors
    ///
    /// - If the format isn't supported
    /// - If there is an error with IO
    /// - If there is a backend-specific error
    fn read(&self, source: &MediaSource) -> DecoderResult<Box<dyn AudioStream>> {
        self.read_fallible(source)
            .transpose()
            .unwrap_or(Err(DecoderError::UnsupportedFormat {
                source: source.into(),
                reason: None,
            }))
    }
}

/// A decoded stream of audio
///
/// The next packet of the stream can be obtained using [`Self::next_packet`],
/// or the entire stream can be decoded into a single packet using [`AudioStreamExt::decode_all`].
///
/// There are also various utilities for modifying the stream
pub trait AudioStream {
    /// Get the next packet of data from the stream
    ///
    /// The packets are guaranteed to be:
    /// - Fairly small (so that [effects](crate::effect) aren't too intensive)
    /// - A consistent size ([for effects that need a consistent size](crate::effect::effects::Resample))
    ///
    /// # Errors
    ///
    /// - If there is an error found while decoding
    /// - If there is an error with IO
    /// - If there is a backend-specific error
    ///
    /// # Implementors
    ///
    /// If your implementation doesn't send a consistently sized packet, then wrap the stream in a
    /// [`buffered::AudioStream`]. Users of this crate should not be required to wrap every decoder
    /// in a [`buffered::Decoder`].
    fn next_packet(&mut self) -> DecoderResult<Option<GenericPacket>>;

    /// Seek the stream to a given `duration`, measured from the start
    ///
    /// # Errors
    ///
    /// - If the stream is unseekable
    /// - If the stream can only be seeked forward
    /// - If the duration is out of the bounds of the stream
    /// - If there's a backend-specific error
    fn seek_to(&mut self, duration: Duration) -> DecoderResult<()>;

    /// Seek the stream from the current position by a given `duration` in either `direction`
    ///
    /// # Errors
    ///
    /// - If the stream is unseekable
    /// - If the stream can only be seeked forward
    /// - If the duration is out of the bounds of the stream
    /// - If there's a backend-specific error
    fn seek_by(&mut self, duration: Duration, direction: Direction) -> DecoderResult<()> {
        let position = self.position();
        let new = match direction {
            Direction::Forward => position + duration,
            Direction::Backward => position.checked_sub(duration).unwrap_or_default(),
        };
        self.seek_to(new)
    }

    /// Returns the source that this decoder was created from
    fn source(&self) -> &SourceName;

    /// Measure the current position of the stream, in seconds
    ///
    /// The position is measured as the duration from the start of the stream
    /// to the end of the last given packet
    fn position(&self) -> Duration;
    /// Measure the full duration of the stream, in seconds
    fn duration(&self) -> Duration;
    /// Get the progress of the current stream from the start to the end
    ///
    /// This is implemented as `position / duration`, and returns 1.0 if the duration is 0.0
    fn progress(&self) -> f64 {
        duration_div(self.position(), self.duration())
    }

    /// Obtain an [atomic](std::sync::atomic) reference to the stream's position and duration
    ///
    /// If an atomic reference isn't necessary, then use [`Self::position`] and [`Self::duration`]
    /// instead, as they may be more efficient.
    ///
    /// See [`StreamTimes`] for more information
    fn times(&self) -> Arc<dyn StreamTimes>;
}

/// Methods specific for an [`AudioStream`] trait object ([`Box<dyn AudioStream>`])
pub trait AudioStreamExt {
    /// Get an iterator over every packet
    ///
    /// The iterator returns [`DecoderResult<GenericPacket>`]
    fn iter(&mut self) -> Iter<'_>;

    /// Decode every packet left, combining them into a single packet
    ///
    /// # Errors
    ///
    /// - If the [`StreamSpec`]s of the packets don't match ([`SpecMismatch`](DecoderError::SpecMismatch))
    /// - If there are other errors while decoding (see [`AudioStream::next_packet`])
    /// - Returns [`None`] if there are no more packets in the stream
    fn decode_all(&mut self) -> DecoderResult<Option<GenericPacket>> {
        self.iter()
            .collect::<Result<Option<Result<_, _>>, _>>()?
            .transpose()
            .map_err(|_| DecoderError::SpecMismatch)
    }
}

impl AudioStreamExt for Box<dyn AudioStream> {
    fn iter(&mut self) -> Iter<'_> {
        Iter { stream: self }
    }
}

/// A [synchronized](std::sync) reference to a stream's position and duration
///
/// This reference is synchronized to the stream, even if it moves to a different thread. This
/// allows a player's [`Handle`](crate::player::Handle) to give an exact position when queried,
/// even if the stream lags when playing.
pub trait StreamTimes: Send + Sync {
    /// Measure the current position of the stream, in seconds
    ///
    /// The position is measured as the duration from the start of the stream
    /// to the end of the last given packet
    fn position(&self) -> Duration;
    /// Measure the full duration of the stream, in seconds
    fn duration(&self) -> Duration;
    /// Get the progress of the current stream from the start to the end
    ///
    /// This is implemented as `position / duration`, and returns 1.0 if the duration is 0.0
    fn progress(&self) -> f64 {
        duration_div(self.position(), self.duration())
    }
}

// the durations would never get that large
#[allow(clippy::cast_precision_loss)]
fn duration_div(num: Duration, denom: Duration) -> f64 {
    let num = num.as_nanos() as f64;
    let denom = denom.as_nanos() as f64;
    if denom == 0.0 {
        1.0
    } else {
        num / denom
    }
}

/// An iterator over packets returned by an [`AudioStream`]
///
/// Obtained using [`AudioStreamExt::iter`]
pub struct Iter<'a> {
    stream: &'a mut Box<dyn AudioStream>,
}

impl Iterator for Iter<'_> {
    type Item = DecoderResult<GenericPacket>;
    fn next(&mut self) -> Option<Self::Item> {
        self.stream.next_packet().transpose()
    }
}

/// A direction, either [forward](Self::Forward) or [backward](Self::Backward)
///
/// Used by [`AudioStream::seek_by`]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Direction {
    Forward,
    Backward,
}

pub(crate) fn frame_to_duration(frame: usize, sample_rate: usize) -> Duration {
    let secs = frame / sample_rate;
    let remaining = frame % sample_rate;
    let nanos = remaining * 1_000_000_000 / sample_rate;

    Duration::new(
        secs as u64,
        nanos
            .try_into()
            .expect("nanos should only ever be less than 1_000_000_000"),
    )
}

pub(crate) fn duration_to_frame(duration: Duration, sample_rate: usize) -> usize {
    let secs = duration.as_secs();
    let nanos = duration.subsec_nanos();

    let secs_frames = secs * sample_rate as u64;
    let nanos_frames = nanos as usize * sample_rate / 1_000_000_000;

    usize::try_from(secs_frames).expect("duration should fit within a usize") + nanos_frames
}

/// A list of decoders that are tried in order
///
/// This implements [`Decoder`] itself by sequentially checking each decoder -
/// decoders closer to the start of the list are given priority.
///
/// If any decoder returns an [error](DecoderError) other than [`DecoderError::UnsupportedFormat`],
/// then that error will be immediately returned (it short-circuits)
#[derive(std::default::Default)]
pub struct List {
    decoders: Vec<Box<dyn Decoder>>,
}

impl List {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(&mut self, decoder: impl Decoder + 'static) {
        self.decoders.push(Box::new(decoder));
    }
}

impl Decoder for List {
    fn read_fallible(&self, source: &MediaSource) -> DecoderResult<Option<Box<dyn AudioStream>>> {
        for decoder in &self.decoders {
            if let Some(stream) = decoder.read_fallible(source)? {
                return Ok(Some(stream));
            }
        }
        Ok(None)
    }
}

/// Any error that can be occurred while decoding
#[derive(Error, Debug)]
// see [`crate::audio::AudioError`] for justification
#[allow(clippy::module_name_repetitions)]
pub enum DecoderError {
    #[error(
        "format of given {source} is not supported{}",
        reason.as_ref().map_or(
            String::new(), 
            |reason| format!(": {reason}")
        ),
    )]
    UnsupportedFormat {
        source: SourceName,
        reason: Option<String>,
    },
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("malformed data in {source}: {}", reason.as_deref().unwrap_or("unknown"))]
    MalformedData {
        source: SourceName,
        reason: Option<String>,
    },
    #[error("no tracks found for {0}")]
    NoTracks(SourceName),
    #[error("failed to seek {source}: {reason}")]
    SeekError {
        source: SourceName,
        reason: SeekError,
    },
    #[error("tried to decode an entire file into one packet when the file had multiple different StreamSpecs")]
    SpecMismatch,
    #[error("decoder found error: {}", .0.as_deref().unwrap_or("unknown"))]
    Other(Option<String>),
}

impl DecoderError {
    #[must_use]
    pub const fn log_level(&self) -> log::Level {
        match self {
            Self::UnsupportedFormat { .. } | Self::MalformedData { .. }  | Self::NoTracks(_) => log::Level::Warn,
            Self::IoError(_) | Self::SeekError { .. } | Self::SpecMismatch | Self::Other(_) => log::Level::Error,
        }
    }
}

/// An error that can occur when [seeking](AudioStream::seek_to) an [`AudioStream`]
#[derive(Error, Debug)]
pub enum SeekError {
    #[error("file is unseekable")]
    Unseekable,
    #[error("given timestamp is out of bounds")]
    OutOfBounds,
    #[error("file can only be seeked forward")]
    ForwardOnly,
}

/// A result of an operation on a [`Decoder`]
// see [`crate::audio::AudioError`] for justification
#[allow(clippy::module_name_repetitions)]
pub type DecoderResult<T> = Result<T, DecoderError>;

#[cfg(test)]
mod tests {
    use std::time::Duration;

    #[test]
    pub fn duration_to_frame() {
        let sample_rate = 44100;
        let frames = sample_rate * 2 + sample_rate / 2;
        let duration = Duration::from_secs_f64(2.5);
        assert_eq!(frames, super::duration_to_frame(duration, sample_rate));
    }

    #[test]
    pub fn frame_to_duration() {
        let sample_rate = 44100;
        let frames = sample_rate * 2 + sample_rate / 2;
        let duration = Duration::from_secs_f64(2.5);
        assert_eq!(duration, super::frame_to_duration(frames, sample_rate));
    }

    #[test]
    pub fn mixed() {
        let sample_rate = 44100;
        let original = sample_rate * 2 + sample_rate / 2;
        let duration = super::frame_to_duration(original, sample_rate);
        let result = super::duration_to_frame(duration, sample_rate);
        // there could be rounding errors, so give it some leeway
        assert!(result - original <= 1);
    }
}
