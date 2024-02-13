//! Decoding of audio files
//!
//! # Examples
//!
//! ```
//! use sauti::decoder::prelude::*;
//! ```

use std::{default::Default as _, fmt::Debug, path::PathBuf, time::Duration};

use thiserror::Error;

use crate::data::GenericPacket;

mod symphonia;

pub mod prelude {
    pub use super::{AudioStream, Decoder, DecoderError, DecoderResult};
    pub use crate::data::prelude::*;
}

pub use symphonia::Symphonia;

use self::prelude::*;

#[must_use]
pub fn default() -> self::Default {
    Symphonia::default()
}

pub type Default = Symphonia;

// NOTE: for implementors: read and read_fallible + buf_read and buf_read_fallible are defined in
// terms of each other. It's expected for the implementor to either implement read and buf_read or
// read_fallible and buf_read_fallible, letting the default implementation get the other side
pub trait Decoder: Send + 'static {
    /// Try to decode and read this file, returning `Ok(None)` if the format isn't supported
    ///
    /// # Errors
    ///
    /// - If there is some error with IO
    /// - If there is a backend-specific error
    fn read_fallible(&self, source: &MediaSource) -> DecoderResult<Option<Box<dyn AudioStream>>> {
        let res = self.read(source);
        if matches!(res, Err(DecoderError::UnsupportedFormat(_))) {
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
            .unwrap_or(Err(DecoderError::UnsupportedFormat(source.into())))
    }
}

// TODO: guarantee that the number of frames will be about equal
pub trait AudioStream {
    /// Get the next packet of data from the stream
    ///
    /// The packets are guaranteed to be:
    /// - Fairly small (so that [effects](crate::effect) aren't too intensive)
    /// - A consistent size (for effects that [need this](crate::effect::ResizeChannels))
    ///
    /// # Errors
    ///
    /// - If there is an error found while decoding
    /// - If there is an error with IO
    /// - If there is a backend-specific error
    fn next_packet(&mut self) -> DecoderResult<Option<GenericPacket>>;

    /// # Errors
    ///
    /// - If the stream is unseekable
    /// - If the stream can only be seeked forward
    /// - If the duration is out of the bounds of the stream
    /// - If there's a backend-specific error
    fn seek_to(&mut self, duration: Duration) -> DecoderResult<()>;

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
            Direction::Backward => position - duration,
        };
        self.seek_to(new)
    }

    fn position(&self) -> Duration;
    fn duration(&self) -> Duration;
}

#[derive(Clone, Copy)]
pub enum Direction {
    Forward,
    Backward,
}

fn frame_to_duration(frame: usize, sample_rate: usize) -> Duration {
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

#[derive(Error, Debug)]
// see [`crate::audio::AudioError`] for justification
#[allow(clippy::module_name_repetitions)]
pub enum DecoderError {
    #[error("format of given {0:?} is not supported")]
    UnsupportedFormat(ErrorSource),
    #[error("io error: {0}")]
    IoError(std::io::Error),
    #[error("malformed data in {source:?}: {}", reason.as_deref().unwrap_or("unknown"))]
    MalformedData {
        source: ErrorSource,
        reason: Option<String>,
    },
    #[error("no tracks found for {0}")]
    NoTracks(ErrorSource),
    #[error("failed to seek {source:?}: {reason:?}")]
    SeekError {
        source: ErrorSource,
        reason: SeekError,
    },
    #[error("decoder found error: {}", .0.as_deref().unwrap_or("unknown"))]
    Other(Option<String>),
}

#[derive(Error, Debug)]
pub enum SeekError {
    #[error("file is unseekable")]
    Unseekable,
    #[error("given timestamp is out of bounds")]
    OutOfBounds,
    #[error("file can only be seeked forward")]
    ForwardOnly,
}

#[derive(Error, Debug, Clone)]
pub enum ErrorSource {
    #[error("file '{0}'")]
    File(PathBuf),
    #[error("buffer")]
    Buffer,
}

impl From<&MediaSource> for ErrorSource {
    fn from(value: &MediaSource) -> Self {
        match value {
            MediaSource::Buffer(_) => Self::Buffer,
            MediaSource::Path(path) => Self::File(path.to_owned()),
        }
    }
}

impl From<std::io::Error> for DecoderError {
    fn from(v: std::io::Error) -> Self {
        Self::IoError(v)
    }
}

// see [`crate::audio::AudioError`] for justification
#[allow(clippy::module_name_repetitions)]
pub type DecoderResult<T> = Result<T, DecoderError>;
