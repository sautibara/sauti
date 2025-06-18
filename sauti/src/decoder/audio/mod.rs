//! Decoding of the audio in audio files
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

pub mod buffered;

use std::sync::Arc;
use std::time::Duration;

use super::symphonia::Symphonia;
use crate::data::GenericPacket;
use prelude::*;
use thiserror::Error;

/// Useful types for interacting with audio decoders.
pub mod prelude {
    pub use super::super::{AudioDecoder, ExtensionSet};
    pub use super::{
        buffered, AudioStream, AudioStreamExt, DecoderError, DecoderResult, StreamTimes,
    };
    pub use crate::data::prelude::*;
}

/// Get the default [`Decoder`]
#[must_use]
pub fn default() -> self::Default {
    Symphonia::default()
}

/// The output type of [`default`]
pub type Default = Symphonia;

/// A type that can [`read`](Self::read) and decode audio from a [`MediaSource`], returning an [`AudioStream`]
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

    /// Get a [set](ExtensionSet) of all file extensions that this decoder can possibly decode.
    ///
    /// This does not mean that the decoder must be able to decode every file with this extension -
    /// only that it may be able to.
    fn supported_extensions(&self) -> ExtensionSet;
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
    /// Get a snapshot of the current times of the stream
    ///
    /// Although it derives [`StreamTimes`], it is not actually synchronized
    fn snapshot(&self) -> StreamTimesSnapshot {
        StreamTimesSnapshot {
            position: self.position(),
            duration: self.duration(),
        }
    }
}

/// An **unsynchronized** snapshot of a stream's current times
///
/// Notice that although this implements [`StreamTimes`], it is not actually synchronized to the
/// stream
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StreamTimesSnapshot {
    position: Duration,
    duration: Duration,
}

impl StreamTimes for StreamTimesSnapshot {
    fn position(&self) -> Duration {
        self.position
    }
    fn duration(&self) -> Duration {
        self.duration
    }
    fn snapshot(&self) -> StreamTimesSnapshot {
        *self
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

    fn supported_extensions(&self) -> ExtensionSet {
        self.decoders
            .iter()
            .map(|decoder| decoder.supported_extensions())
            .reduce(|left, right| &left | &right)
            .unwrap_or_default()
    }
}

/// Any error that can be occurred while decoding audio
#[derive(Error, Debug)]
// see [`crate::output::AudioError`] for justification
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
            Self::UnsupportedFormat { .. } | Self::MalformedData { .. } | Self::NoTracks(_) => {
                log::Level::Warn
            }
            Self::IoError(_) | Self::SeekError { .. } | Self::SpecMismatch | Self::Other(_) => {
                log::Level::Error
            }
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
