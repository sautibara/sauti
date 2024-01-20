use std::path::{Path, PathBuf};

use dasp_sample::FromSample;
use thiserror::Error;

use crate::audio::ConvertibleSample;

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

#[derive(Clone, Copy)]
pub struct StreamSpec {
    pub channels: usize,
    pub sample_rate: usize,
}

pub enum GenericPacket {
    I8(SoundPacket<i8>),
    I16(SoundPacket<i16>),
    I32(SoundPacket<i32>),
    I64(SoundPacket<i64>),
    U8(SoundPacket<u8>),
    U16(SoundPacket<u16>),
    U32(SoundPacket<u32>),
    U64(SoundPacket<u64>),
    F32(SoundPacket<f32>),
    F64(SoundPacket<f64>),
}

impl GenericPacket {
    pub fn convert<N: ConvertibleSample>(self) -> SoundPacket<N> {
        self.into()
    }
}

impl<N: ConvertibleSample> From<GenericPacket> for SoundPacket<N> {
    fn from(value: GenericPacket) -> Self {
        match value {
            GenericPacket::I8(packets) => packets.convert(),
            GenericPacket::I16(packets) => packets.convert(),
            GenericPacket::I32(packets) => packets.convert(),
            GenericPacket::I64(packets) => packets.convert(),
            GenericPacket::U8(packets) => packets.convert(),
            GenericPacket::U16(packets) => packets.convert(),
            GenericPacket::U32(packets) => packets.convert(),
            GenericPacket::U64(packets) => packets.convert(),
            GenericPacket::F32(packets) => packets.convert(),
            GenericPacket::F64(packets) => packets.convert(),
        }
    }
}

pub struct SoundPacket<S: ConvertibleSample> {
    interleaved_samples: Vec<S>,
    spec: StreamSpec,
}

impl<S: ConvertibleSample> SoundPacket<S> {
    pub fn from_interleaved(samples: Vec<S>, spec: StreamSpec) -> Self {
        assert!(samples.len() % spec.channels == 0, "the interleaved samples should have the same amount of samples for each channel (samples.len % channels == 0)");
        Self {
            interleaved_samples: samples,
            spec,
        }
    }

    pub fn from_channels(samples: &[&[S]], sample_rate: usize) -> Self {
        let channels = samples.len();
        let frames = samples.first().map(|slice| slice.len()).unwrap_or(0);
        let mut interleaved_samples = vec![S::EQUILIBRIUM; channels * frames];

        for (channel, samples) in samples.iter().enumerate() {
            for (frame, sample) in samples.iter().enumerate() {
                interleaved_samples[frame * channels + channel] = *sample;
            }
        }

        Self {
            interleaved_samples,
            spec: StreamSpec {
                channels,
                sample_rate,
            },
        }
    }

    pub fn channels(&self) -> usize {
        self.spec.channels
    }

    pub fn sample_rate(&self) -> usize {
        self.spec.sample_rate
    }

    pub fn frames(&self) -> usize {
        self.interleaved_samples().len() / self.channels()
    }

    pub fn samples(&self) -> impl Iterator<Item = &[S]> {
        self.interleaved_samples.chunks_exact(self.channels())
    }

    pub fn interleaved_samples(&self) -> &[S] {
        &self.interleaved_samples[..]
    }

    pub fn chan(&self, channel: usize) -> impl Iterator<Item = &S> {
        assert!(
            channel < self.channels(),
            "accessed channel index shouldn't be more than the amount of channels in the packet"
        );
        self.samples().map(move |channels| &channels[channel])
    }

    pub fn convert<N: ConvertibleSample + FromSample<S>>(self) -> SoundPacket<N> {
        let interleaved_samples = self
            .interleaved_samples
            .into_iter()
            .map(|source| N::from_sample(source))
            .collect();
        SoundPacket {
            interleaved_samples,
            spec: self.spec,
        }
    }
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
