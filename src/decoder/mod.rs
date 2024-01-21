//! Decoding of audio files
//!
//! # Examples
//!
//! ```
//! use sauti::decoder::prelude::*;
//! ```

use std::{
    fmt::Debug,
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::data::GenericPacket;

mod symphonia;

pub mod prelude {
    pub use super::{AudioStream, Decoder, DecoderError, DecoderResult};
    pub use crate::data::prelude::*;
}

pub use symphonia::Symphonia;

#[must_use]
pub fn default() -> impl Decoder {
    Symphonia::default()
}

pub trait Decoder {
    /// Try to decode and read this file, returning `Ok(None)` if the format isn't supported
    ///
    /// # Errors
    ///
    /// - If there is some error with IO
    /// - If there is a backend-specific error
    fn read_fallible(&self, path: &Path) -> DecoderResult<Option<Box<dyn AudioStream>>>;

    /// Try to decode and read this file, returning `Ok(None)` if the format isn't supported
    ///
    /// # Errors
    ///
    /// - If there is some error with IO
    /// - If there is a backend-specific error
    fn read_buf_fallible(&self, buf: &[u8]) -> DecoderResult<Option<Box<dyn AudioStream>>>;

    /// Try to decode and read this file, returning `Err(UnsupportedFormat)` if the format isn't supported
    ///
    /// # Errors
    ///
    /// - If the format isn't supported
    /// - If there is an error with IO
    /// - If there is a backend-specific error
    fn read(&self, path: &Path) -> DecoderResult<Box<dyn AudioStream>> {
        self.read_fallible(path)
            .transpose()
            .unwrap_or(Err(DecoderError::UnsupportedFormat(Source::File(
                path.to_owned(),
            ))))
    }

    /// Try to decode and read this file, returning `Err(UnsupportedFormat)` if the format isn't supported
    ///
    /// # Errors
    ///
    /// - If the format isn't supported
    /// - If there is an error with IO
    /// - If there is a backend-specific error
    fn read_buf(&self, buf: &[u8]) -> DecoderResult<Box<dyn AudioStream>> {
        self.read_buf_fallible(buf)
            .transpose()
            .unwrap_or(Err(DecoderError::UnsupportedFormat(Source::Buffer)))
    }
}

pub trait AudioStream {
    /// # Errors
    ///
    /// - If there is an error found while decoding
    /// - If there is an error with IO
    /// - If there is a backend-specific error
    fn next_packet(&mut self) -> DecoderResult<Option<GenericPacket>>;
}

#[derive(Default)]
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
    fn read_fallible(&self, file: &Path) -> DecoderResult<Option<Box<dyn AudioStream>>> {
        for decoder in &self.decoders {
            if let Some(stream) = decoder.read_fallible(file)? {
                return Ok(Some(stream));
            }
        }
        Ok(None)
    }

    fn read_buf_fallible(&self, buf: &[u8]) -> DecoderResult<Option<Box<dyn AudioStream>>> {
        for decoder in &self.decoders {
            if let Some(stream) = decoder.read_buf_fallible(buf)? {
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
    UnsupportedFormat(Source),
    #[error("io error: {0}")]
    IoError(std::io::Error),
    #[error("malformed data in {source:?}: {}", reason.as_deref().unwrap_or("unknown"))]
    MalformedData {
        source: Source,
        reason: Option<String>,
    },
    #[error("no tracks found for {0}")]
    NoTracks(Source),
    #[error("decoder found error: {}", .0.as_deref().unwrap_or("unknown"))]
    Other(Option<String>),
}

#[derive(Error, Debug, Clone)]
pub enum Source {
    #[error("file '{0}'")]
    File(PathBuf),
    #[error("buffer")]
    Buffer,
}

impl From<std::io::Error> for DecoderError {
    fn from(v: std::io::Error) -> Self {
        Self::IoError(v)
    }
}

// see [`crate::audio::AudioError`] for justification
#[allow(clippy::module_name_repetitions)]
pub type DecoderResult<T> = Result<T, DecoderError>;
