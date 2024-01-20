use std::{
    fs::File,
    ops::Deref,
    path::{Path, PathBuf},
};

use symphonia::core::{
    audio::{AudioBuffer, AudioBufferRef, Signal},
    codecs::{CodecRegistry, DecoderOptions},
    formats::{FormatOptions, FormatReader},
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    probe::{Hint, Probe},
};

use crate::audio::ConvertibleSample;

use super::{AudioStream, Decoder, FileError, FileResult, GenericPacket, SoundPacket};

pub struct Symphonia {
    probe: Probe,
    codec_registry: CodecRegistry,
}

impl Default for Symphonia {
    fn default() -> Self {
        let mut probe = Probe::default();
        let mut codec_registry = CodecRegistry::default();
        symphonia::default::register_enabled_formats(&mut probe);
        symphonia::default::register_enabled_codecs(&mut codec_registry);
        Self {
            probe,
            codec_registry,
        }
    }
}

impl Decoder for Symphonia {
    fn read_fallible(
        &self,
        path: &std::path::Path,
    ) -> super::FileResult<Option<Box<dyn super::AudioStream>>> {
        let source = Box::new(File::open(path)?);
        let source = MediaSourceStream::new(source, MediaSourceStreamOptions::default());

        let mut hint = Hint::new();
        // NOTE: as of now, symphonia ignores the hint, but I'd like to imagine that it does
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            hint.with_extension(extension);
        }

        // read the format of the file (but don't decode yet)
        let format_result = (self.probe)
            .format(
                &hint,
                source,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|err| map_error_with_path(err, path));

        // if the format is unsupported, then return None to signify it
        if matches!(format_result, Err(FileError::UnsupportedFormat(_))) {
            return Ok(None);
        }

        // propagate the other errors
        let reader = format_result?;

        // get the default track
        let track = (reader.format)
            .default_track()
            .ok_or_else(|| FileError::NoTracks(path.to_owned()))?;

        // try to decode the track
        let decode_result = self
            .codec_registry
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|err| map_error_with_path(err, path));

        // if the codec is unsupported, then return None to signify it
        if matches!(decode_result, Err(FileError::UnsupportedFormat(_))) {
            return Ok(None);
        }

        // propagate the other errors
        let decoder = decode_result?;

        let stream = Stream {
            path: path.to_owned(),
            file: reader.format,
            decoder,
        };

        Ok(Some(Box::new(stream)))
    }
}

use symphonia::core::codecs::Decoder as SymphoniaDecoder;
struct Stream {
    path: PathBuf,
    file: Box<dyn FormatReader>,
    decoder: Box<dyn SymphoniaDecoder>,
}

impl AudioStream for Stream {
    fn next_packet(&mut self) -> FileResult<Option<GenericPacket>> {
        let undecoded_packet = self.file.next_packet();
        if is_end_of_stream(&undecoded_packet) {
            return Ok(None);
        }
        let undecoded_packet =
            undecoded_packet.map_err(|err| map_error_with_path(err, &self.path))?;
        let symphonia_packet = (self.decoder)
            .decode(&undecoded_packet)
            .map_err(|err| map_error_with_path(err, &self.path))?;
        let packet = symphonia_packet.into();
        Ok(Some(packet))
    }
}

fn is_end_of_stream<T>(error: &Result<T, SymphoniaError>) -> bool {
    if let Err(SymphoniaError::IoError(io_err)) = error {
        io_err.kind() == std::io::ErrorKind::UnexpectedEof
    } else {
        false
    }
}

impl<'a> From<AudioBufferRef<'a>> for GenericPacket {
    fn from(value: AudioBufferRef<'a>) -> Self {
        match value {
            AudioBufferRef::U8(buffer) => Self::U8(buffer.deref().into()),
            AudioBufferRef::U16(buffer) => Self::U16(buffer.deref().into()),
            AudioBufferRef::U24(_) => todo!(),
            AudioBufferRef::U32(buffer) => Self::U32(buffer.deref().into()),
            AudioBufferRef::S8(buffer) => Self::I8(buffer.deref().into()),
            AudioBufferRef::S16(buffer) => Self::I16(buffer.deref().into()),
            AudioBufferRef::S24(_) => todo!(),
            AudioBufferRef::S32(buffer) => Self::I32(buffer.deref().into()),
            AudioBufferRef::F32(buffer) => Self::F32(buffer.deref().into()),
            AudioBufferRef::F64(buffer) => Self::F64(buffer.deref().into()),
        }
    }
}

use symphonia::core::sample::Sample as SymphoniaSample;
impl<S: ConvertibleSample + SymphoniaSample> From<&AudioBuffer<S>> for SoundPacket<S> {
    fn from(buffer: &AudioBuffer<S>) -> Self {
        let channels: Box<[_]> = (0..buffer.spec().channels.count())
            .map(|channel| buffer.chan(channel))
            .collect();

        Self::from_channels(&channels, buffer.spec().rate as usize)
    }
}

use symphonia::core::errors::Error as SymphoniaError;
fn map_error_with_path(error: SymphoniaError, path: &Path) -> FileError {
    match error {
        SymphoniaError::IoError(error) => FileError::IoError(error),
        SymphoniaError::DecodeError(reason) => FileError::MalformedData {
            path: path.to_owned(),
            reason: Some(reason.to_string()),
        },
        SymphoniaError::SeekError(_) => unimplemented!("decoder never seeks"),
        SymphoniaError::Unsupported(_) => FileError::UnsupportedFormat(path.to_owned()),
        SymphoniaError::LimitError(error) => FileError::Other(Some(error.to_string())),
        SymphoniaError::ResetRequired => FileError::Other(Some("decoder needs reset".to_string())),
    }
}
