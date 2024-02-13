use std::{fs::File, io::Cursor, ops::Deref, option::Option, time::Duration};

use symphonia::core::{
    audio::{AudioBuffer, AudioBufferRef, Signal},
    codecs::{CodecRegistry, DecoderOptions},
    errors::SeekErrorKind,
    formats::{FormatOptions, FormatReader, SeekMode},
    io::{MediaSource as SymphoniaSource, MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    probe::{Hint, Probe},
    units::Time,
};

use super::{prelude::*, ErrorSource, SeekError};

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

impl Symphonia {
    fn read_source(
        &self,
        source: Box<dyn SymphoniaSource>,
        error_source: ErrorSource,
        hint: &Hint,
    ) -> DecoderResult<Box<dyn AudioStream>> {
        let source = MediaSourceStream::new(source, MediaSourceStreamOptions::default());

        // read the format of the file (but don't decode yet)
        let reader = (self.probe)
            .format(
                hint,
                source,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|err| map_error_with_source(err, &error_source))?;

        // get the default track
        let track = (reader.format)
            .default_track()
            .ok_or_else(|| DecoderError::NoTracks(error_source.clone()))?;

        // try to decode the track
        let decoder = self
            .codec_registry
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|err| map_error_with_source(err, &error_source))?;

        let sample_rate = track
            .codec_params
            .sample_rate
            .ok_or_else(|| unsupported(&error_source))?;
        let frame_count = track
            .codec_params
            .n_frames
            .ok_or_else(|| unsupported(&error_source))?;

        let stream = Stream {
            file: reader.format,
            decoder,
            current_frame: 0,
            sample_rate: sample_rate as usize,
            frame_count: usize::try_from(frame_count).map_err(|_| unsupported(&error_source))?,
            error_source,
        };

        Ok(Box::new(stream))
    }
}

impl Decoder for Symphonia {
    fn read(&self, source: &MediaSource) -> super::DecoderResult<Box<dyn super::AudioStream>> {
        let error_source = source.into();
        let mut hint = Hint::new();

        let symphonia_source: Box<dyn SymphoniaSource> = match source {
            MediaSource::Path(path) => {
                // NOTE: as of now, symphonia ignores the hint, but I'd like to imagine that it does
                if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
                    hint.with_extension(extension);
                }

                Box::new(File::open(path)?)
            }
            MediaSource::Buffer(buf) => Box::new(Cursor::new(buf.clone())),
        };

        self.read_source(symphonia_source, error_source, &hint)
    }
}

use symphonia::core::codecs::Decoder as SymphoniaDecoder;
struct Stream {
    error_source: ErrorSource,
    file: Box<dyn FormatReader>,
    decoder: Box<dyn SymphoniaDecoder>,
    current_frame: usize,
    frame_count: usize,
    sample_rate: usize,
}

impl AudioStream for Stream {
    fn next_packet(&mut self) -> DecoderResult<Option<GenericPacket>> {
        let undecoded_packet = self.file.next_packet();
        if is_end_of_stream(&undecoded_packet) {
            return Ok(None);
        }
        let undecoded_packet =
            undecoded_packet.map_err(|err| map_error_with_source(err, &self.error_source))?;
        let symphonia_packet = (self.decoder)
            .decode(&undecoded_packet)
            .map_err(|err| map_error_with_source(err, &self.error_source))?;
        self.current_frame += symphonia_packet.frames();
        let packet = symphonia_packet.into();
        Ok(Some(packet))
    }

    fn seek_to(&mut self, duration: Duration) -> DecoderResult<()> {
        let secs = duration.as_secs();
        let subsecs = duration.as_secs_f64().fract();
        let time = Time::new(secs, subsecs);
        self.file
            .seek(
                SeekMode::Coarse,
                symphonia::core::formats::SeekTo::Time {
                    time,
                    track_id: None,
                },
            )
            .map_err(|err| map_error_with_source(err, &self.error_source))?;
        Ok(())
    }

    fn position(&self) -> Duration {
        super::frame_to_duration(self.current_frame, self.sample_rate)
    }

    fn duration(&self) -> Duration {
        super::frame_to_duration(self.frame_count, self.sample_rate)
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
            AudioBufferRef::U24(_) => todo!("implement U24 and S24 in samples"),
            AudioBufferRef::U32(buffer) => Self::U32(buffer.deref().into()),
            AudioBufferRef::S8(buffer) => Self::I8(buffer.deref().into()),
            AudioBufferRef::S16(buffer) => Self::I16(buffer.deref().into()),
            AudioBufferRef::S24(_) => todo!("implement U24 and S24 in samples"),
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
fn map_error_with_source(error: SymphoniaError, source: &ErrorSource) -> DecoderError {
    match error {
        SymphoniaError::IoError(error) => DecoderError::IoError(error),
        SymphoniaError::DecodeError(reason) => DecoderError::MalformedData {
            source: source.clone(),
            reason: Some(reason.to_string()),
        },
        SymphoniaError::SeekError(kind) => DecoderError::SeekError {
            source: source.clone(),
            reason: kind.into(),
        },
        SymphoniaError::Unsupported(_) => DecoderError::UnsupportedFormat(source.clone()),
        SymphoniaError::LimitError(error) => DecoderError::Other(Some(error.to_string())),
        SymphoniaError::ResetRequired => {
            DecoderError::Other(Some("decoder needs reset".to_string()))
        }
    }
}

fn unsupported(error_source: &ErrorSource) -> DecoderError {
    DecoderError::UnsupportedFormat(error_source.clone())
}

impl From<SeekErrorKind> for SeekError {
    fn from(value: SeekErrorKind) -> Self {
        match value {
            SeekErrorKind::Unseekable => Self::Unseekable,
            SeekErrorKind::OutOfRange => Self::OutOfBounds,
            SeekErrorKind::ForwardOnly => Self::ForwardOnly,
            SeekErrorKind::InvalidTrack => {
                unreachable!("decoder never sets the track when seeking")
            }
        }
    }
}
