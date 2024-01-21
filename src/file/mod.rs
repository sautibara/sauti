use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::data::GenericPacket;

mod symphonia;

pub use symphonia::Symphonia;

pub fn default_decoder() -> impl Decoder {
    Symphonia::default()
}

pub trait Decoder {
    /// Try to decode and read this file, returning `Ok(None)` if the format isn't supported
    fn read_fallible(&self, file: &Path) -> FileResult<Option<Box<dyn AudioStream>>>;

    /// Try to decode and read this file, returning `Err(UnsupportedFormat)` if the format isn't supported
    fn read(&self, file: &Path) -> FileResult<Box<dyn AudioStream>> {
        self.read_fallible(file)
            .transpose()
            .unwrap_or(Err(FileError::UnsupportedFormat(file.to_owned())))
    }
}

pub trait AudioStream {
    fn next_packet(&mut self) -> FileResult<Option<GenericPacket>>;
}

#[derive(Default)]
pub struct DecoderList {
    decoders: Vec<Box<dyn Decoder>>,
}

impl DecoderList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_decoder(&mut self, decoder: impl Decoder + 'static) {
        self.decoders.push(Box::new(decoder))
    }
}

impl Decoder for DecoderList {
    fn read_fallible(&self, file: &Path) -> FileResult<Option<Box<dyn AudioStream>>> {
        for decoder in &self.decoders {
            if let Some(stream) = decoder.read_fallible(file)? {
                return Ok(Some(stream));
            }
        }
        Ok(None)
    }
}

#[derive(Error, Debug)]
pub enum FileError {
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

impl From<std::io::Error> for FileError {
    fn from(v: std::io::Error) -> Self {
        Self::IoError(v)
    }
}

pub type FileResult<T> = Result<T, FileError>;
