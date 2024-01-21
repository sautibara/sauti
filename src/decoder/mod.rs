use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::data::GenericPacket;

mod symphonia;

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
    fn read_fallible(&self, file: &Path) -> DecoderResult<Option<Box<dyn AudioStream>>>;

    /// Try to decode and read this file, returning `Err(UnsupportedFormat)` if the format isn't supported
    ///
    /// # Errors
    ///
    /// - If the format isn't supported
    /// - If there is an error with IO
    /// - If there is a backend-specific error
    fn read(&self, file: &Path) -> DecoderResult<Box<dyn AudioStream>> {
        self.read_fallible(file)
            .transpose()
            .unwrap_or(Err(DecoderError::UnsupportedFormat(file.to_owned())))
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
}

#[derive(Error, Debug)]
// see [`crate::audio::AudioError`] for justification
#[allow(clippy::module_name_repetitions)]
pub enum DecoderError {
    #[error("format of file '{0}' is not supported")]
    UnsupportedFormat(PathBuf),
    #[error("io error: {0}")]
    IoError(std::io::Error),
    #[error("malformed data in file '{path}': {}", reason.as_deref().unwrap_or("unknown"))]
    MalformedData {
        path: PathBuf,
        reason: Option<String>,
    },
    #[error("no tracks found for '{0}'")]
    NoTracks(PathBuf),
    #[error("decoder found error: {}", .0.as_deref().unwrap_or("unknown"))]
    Other(Option<String>),
}

impl From<std::io::Error> for DecoderError {
    fn from(v: std::io::Error) -> Self {
        Self::IoError(v)
    }
}

// see [`crate::audio::AudioError`] for justification
#[allow(clippy::module_name_repetitions)]
pub type DecoderResult<T> = Result<T, DecoderError>;
