use std::{fs::File, io::Cursor, ops::Deref, option::Option, sync::Arc, time::Duration};

use crossbeam::atomic::AtomicCell;
use itertools::Itertools;
use log::{debug, trace};
use symphonia::core::{
    audio::{AudioBuffer, AudioBufferRef, Signal},
    codecs::{CodecRegistry, DecoderOptions},
    errors::SeekErrorKind,
    formats::{FormatOptions, FormatReader, SeekMode, Track},
    io::{MediaSource as SymphoniaSource, MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    probe::{Hint, Probe},
    units::{Time, TimeBase, TimeStamp},
};

// FIXME: find out why AAC doesn't work

use super::{ExtensionSet, SourceName};
use crate::data::prelude::*;

#[cfg(feature = "audio-decoder")]
use super::audio::StreamTimes;

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

struct DecodedTrack {
    decoder: Box<dyn SymphoniaDecoder>,
    times: Arc<Times>,
    time_base: TimeBase,
    track_id: u32,
}

impl Symphonia {
    fn decode_track(
        &self,
        track: &Track,
        error_source: &SourceName,
    ) -> Result<DecodedTrack, GenericError> {
        // try to decode the codec
        let decoder = self
            .codec_registry
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|err| map_err(err, error_source, Some("codec not supported")))?;

        // extract all of the parameters from the codec
        let sample_rate_u32 = (track.codec_params.sample_rate)
            .ok_or_else(|| unsupported(error_source, "no sample rate found"))?;
        let sample_rate = usize::try_from(sample_rate_u32)
            .map_err(|_| unsupported(error_source, "sample rate is too large"))?;
        let frame_count = (track.codec_params.n_frames)
            .and_then(|frames| usize::try_from(frames).ok())
            .ok_or_else(|| unsupported(error_source, "no frame count found"))?;
        let time_base = (track.codec_params)
            .time_base
            .unwrap_or_else(|| TimeBase::new(1, sample_rate_u32));

        let times = Arc::new(Times::new(frame_count, sample_rate));
        Ok(DecodedTrack {
            decoder,
            times,
            time_base,
            track_id: track.id,
        })
    }

    fn read_source(&self, source: &MediaSource) -> Result<Stream, GenericError> {
        let error_source = source.into();
        let mut hint = Hint::new();

        let symphonia_source: Box<dyn SymphoniaSource> = match source {
            MediaSource::Path(path) => {
                // NOTE: as of now, symphonia ignores the hint, but I'd like to imagine that it doesn't
                if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
                    hint.with_extension(extension);
                }

                Box::new(File::open(path)?)
            }
            MediaSource::Buffer(buf) => Box::new(Cursor::new(buf.clone())),
        };

        let source = MediaSourceStream::new(symphonia_source, MediaSourceStreamOptions::default());

        debug!("Testing if symphonia can decode {error_source}");

        // read the format of the file (but don't decode yet)
        let reader = (self.probe)
            .format(
                &hint,
                source,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|err| map_err(err, &error_source, Some("format not supported")))?;

        debug!("Symphonia can read {error_source}");

        // find the first working track
        let default_track = reader.format.default_track();
        let DecodedTrack {
            decoder,
            times,
            time_base,
            track_id,
        } = (default_track.into_iter())
            .chain(reader.format.tracks())
            // try to decode each track
            .map(|track| self.decode_track(track, &error_source))
            // find a working track or return the first error
            .find_or_first(Result::is_ok)
            // if find returns [`None`], that means that there are no tracks
            .ok_or_else(|| GenericError::NoTracks{ source: error_source.clone() })?
            // if find returns [`Err`], that means that there are no working tracks
            ?;

        // we suceeded!
        debug!("Symphonia can decode {error_source}");

        Ok(Stream {
            file: reader.format,
            decoder,
            times,
            source: error_source,
            time_base,
            track_id,
        })
    }

    fn supported_extensions() -> ExtensionSet {
        ExtensionSet::from_slice(&[
            "mp3", "flac", "ogg", "oga", "opus", "aiff", "aif", "aifc", "mkv", "mka", "caf", "wav",
            "mp1", "mp2", "pcm", "alac",
            // mp4 and aac have issues currently, so these might have to be disabled
            "mp4", "m4a", "m4b", "m4r", "aac",
        ])
    }
}

#[cfg(feature = "audio-decoder")]
impl super::audio::Decoder for Symphonia {
    fn read(
        &self,
        source: &MediaSource,
    ) -> super::audio::DecoderResult<Box<dyn super::audio::AudioStream>> {
        let stream = self.read_source(source)?;

        let is_vorbis =
            stream.decoder.codec_params().codec == symphonia::core::codecs::CODEC_TYPE_VORBIS;
        let default = Box::new(stream);
        // the vorbis implementation tends to spit out different sized packets
        if is_vorbis {
            trace!("symphonia is reading a vorbis track, using a buffered AudioStream");
            Ok(Box::new(super::audio::buffered::AudioStream::wrap(default)))
        } else {
            Ok(default)
        }
    }

    fn supported_extensions(&self) -> ExtensionSet {
        Self::supported_extensions()
    }
}

#[cfg(feature = "metadata-decoder")]
impl super::metadata::Decoder for Symphonia {
    type Tag = Stream;

    fn read(&self, source: &MediaSource) -> super::metadata::MetadataResult<Stream> {
        Ok(self.read_source(source)?)
    }

    fn supported_extensions(&self) -> ExtensionSet {
        Self::supported_extensions()
    }
}

use symphonia::core::codecs::Decoder as SymphoniaDecoder;
pub struct Stream {
    source: SourceName,
    file: Box<dyn FormatReader>,
    decoder: Box<dyn SymphoniaDecoder>,
    times: Arc<Times>,
    time_base: TimeBase,
    track_id: u32,
}

#[cfg(feature = "audio-decoder")]
impl super::audio::AudioStream for Stream {
    fn next_packet(&mut self) -> super::audio::DecoderResult<Option<GenericPacket>> {
        // find the next packet from this track
        let symphonia_packet = loop {
            let packet = self.file.next_packet();
            let packet = match packet {
                Ok(packet) => packet,
                Err(SymphoniaError::IoError(io_err))
                    if io_err.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None)
                }
                Err(err) => return Err(map_err(err, &self.source, None).into()),
            };

            // we found a packet from this track!
            if packet.track_id() == self.track_id {
                break self
                    .decoder
                    .decode(&packet)
                    .map_err(|err| map_err(err, &self.source, Some("failed to decode packet")))?;
            }
        };

        self.times
            .current_frame
            .fetch_add(symphonia_packet.frames());
        let packet: GenericPacket = symphonia_packet.into();
        Ok(Some(packet))
    }

    fn seek_to(&mut self, duration: Duration) -> super::audio::DecoderResult<()> {
        // check if the seek is out of bounds
        let frames = super::duration_to_frame(duration, self.times.sample_rate);
        if frames > self.times.frame_count {
            return Err(super::audio::DecoderError::SeekError {
                source: self.source.clone(),
                reason: super::audio::SeekError::OutOfBounds,
            });
        }
        // actually seek the file
        let secs = duration.as_secs();
        let subsecs = duration.as_secs_f64().fract();
        let time = Time::new(secs, subsecs);
        let seek_res = self.file.seek(
            SeekMode::Coarse,
            symphonia::core::formats::SeekTo::Time {
                time,
                track_id: None,
            },
        );
        // if the seek ended up being out of bounds anyways, then error too
        if is_end_of_stream(&seek_res) {
            return Err(super::audio::DecoderError::SeekError {
                source: self.source.clone(),
                reason: super::audio::SeekError::OutOfBounds,
            });
        }
        // update the times
        let seeked_to = seek_res.map_err(|err| map_err(err, &self.source, None))?;
        self.decoder.reset();
        self.times
            .current_frame
            .store(self.time_stamp_to_frame(seeked_to.actual_ts));
        Ok(())
    }

    fn source(&self) -> &SourceName {
        &self.source
    }

    fn position(&self) -> Duration {
        self.times.position()
    }

    fn duration(&self) -> Duration {
        self.times.duration()
    }

    fn progress(&self) -> f64 {
        self.times.progress()
    }

    fn times(&self) -> Arc<dyn StreamTimes> {
        self.times.clone()
    }
}

impl Stream {
    #[allow(clippy::cast_possible_truncation)] // it's fine
    fn time_stamp_to_frame(&self, time_stamp: TimeStamp) -> usize {
        if self.time_base.numer == 1 && self.time_base.denom as usize == self.times.sample_rate {
            time_stamp as usize
        } else {
            time_stamp as usize * self.times.sample_rate * self.time_base.numer as usize
                / self.time_base.denom as usize
        }
    }
}

struct Times {
    current_frame: AtomicCell<usize>,
    frame_count: usize,
    sample_rate: usize,
}

impl Times {
    fn new(frame_count: usize, sample_rate: usize) -> Self {
        Self {
            current_frame: AtomicCell::default(),
            frame_count,
            sample_rate,
        }
    }
}

#[cfg(feature = "audio-decoder")]
impl StreamTimes for Times {
    fn duration(&self) -> Duration {
        super::frame_to_duration(self.frame_count, self.sample_rate)
    }

    fn position(&self) -> Duration {
        super::frame_to_duration(self.current_frame.load(), self.sample_rate)
    }

    #[allow(clippy::cast_precision_loss)] // the frames wouldn't get that high
    fn progress(&self) -> f64 {
        self.current_frame.load() as f64 / self.frame_count as f64
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

#[cfg(feature = "metadata-decoder")]
use super::metadata::prelude::*;

#[cfg(feature = "metadata-decoder")]
const fn is_duration_tag(id: &FrameId) -> bool {
    matches!(id, FrameId::Duration)
}

#[cfg(feature = "metadata-decoder")]
use super::metadata::Operation;

#[cfg(feature = "metadata-decoder")]
impl super::metadata::Tag for Stream {
    fn has(&self, id: super::metadata::FrameId) -> bool {
        is_duration_tag(&id)
    }

    fn get(&self, id: super::metadata::FrameId) -> FrameOptCow<'_> {
        if is_duration_tag(&id) {
            FrameOptCow::some(FrameCow {
                data: DataCow::Owned(Data::Duration(self.times.duration())),
                id,
            })
        } else {
            FrameOptCow::none(id)
        }
    }

    fn get_all(&self, id: super::metadata::FrameId) -> impl Iterator<Item = FrameCow<'_>> {
        is_duration_tag(&id)
            .then(|| FrameCow {
                data: DataCow::Owned(Data::Duration(self.times.duration())),
                id,
            })
            .into_iter()
    }

    fn frames(&self) -> impl Iterator<Item = FrameCow<'_>> {
        std::iter::once(FrameCow::from(Frame {
            id: FrameId::Duration,
            data: Data::Duration(self.times.duration()),
        }))
    }

    #[inline]
    fn supports(&self, query: Operation) -> bool {
        matches!(
            query,
            Operation::Get(FrameId::Duration)
                | Operation::GetAll(FrameId::Duration)
                | Operation::Data(DataType::Duration)
                | Operation::Frames
        )
    }
}

use symphonia::core::errors::Error as SymphoniaError;

enum GenericError {
    Symphonia {
        error: SymphoniaError,
        source: SourceName,
        unsupported_reason: Option<&'static str>,
    },
    Unsupported {
        source: SourceName,
        reason: &'static str,
    },
    NoTracks {
        source: SourceName,
    },
    IoError(std::io::Error),
}

impl From<std::io::Error> for GenericError {
    fn from(v: std::io::Error) -> Self {
        Self::IoError(v)
    }
}

#[cfg(feature = "metadata-decoder")]
impl From<GenericError> for super::metadata::MetadataError {
    fn from(value: GenericError) -> Self {
        match value {
            GenericError::Symphonia {
                error,
                source,
                unsupported_reason,
            } => match error {
                SymphoniaError::IoError(error) => Self::IoError(error),
                SymphoniaError::DecodeError(reason) => Self::MalformedData {
                    source,
                    reason: Some(reason.to_string()),
                },
                SymphoniaError::Unsupported(reason) => Self::UnsupportedFormat {
                    source,
                    reason: Some(
                        unsupported_reason.map_or_else(String::new, |reason| format!("{reason}: "))
                            + reason,
                    ),
                },
                SymphoniaError::ResetRequired => {
                    Self::Other(Some("decoder needs reset".to_string()))
                }
                error => Self::Other(Some(error.to_string())),
            },
            GenericError::Unsupported { source, reason } => Self::UnsupportedFormat {
                source,
                reason: Some(reason.to_owned()),
            },
            GenericError::NoTracks { source } => Self::MalformedData {
                source,
                reason: Some("source has no tracks".to_string()),
            },
            GenericError::IoError(error) => Self::IoError(error),
        }
    }
}

#[cfg(feature = "audio-decoder")]
impl From<GenericError> for super::audio::DecoderError {
    fn from(value: GenericError) -> Self {
        match value {
            GenericError::Symphonia {
                error,
                source,
                unsupported_reason,
            } => match error {
                SymphoniaError::IoError(error) => Self::IoError(error),
                SymphoniaError::DecodeError(reason) => Self::MalformedData {
                    source,
                    reason: Some(reason.to_string()),
                },
                SymphoniaError::SeekError(kind) => Self::SeekError {
                    source,
                    reason: kind.into(),
                },
                SymphoniaError::Unsupported(reason) => Self::UnsupportedFormat {
                    source,
                    reason: Some(
                        unsupported_reason.map_or_else(String::new, |reason| format!("{reason}: "))
                            + reason,
                    ),
                },
                SymphoniaError::LimitError(error) => Self::Other(Some(error.to_string())),
                SymphoniaError::ResetRequired => {
                    Self::Other(Some("decoder needs reset".to_string()))
                }
            },
            GenericError::Unsupported { source, reason } => Self::UnsupportedFormat {
                source,
                reason: Some(reason.to_owned()),
            },
            GenericError::NoTracks { source } => Self::NoTracks(source),
            GenericError::IoError(error) => Self::IoError(error),
        }
    }
}

fn map_err(
    error: SymphoniaError,
    source: &SourceName,
    unsupported_reason: Option<&'static str>,
) -> GenericError {
    GenericError::Symphonia {
        error,
        source: source.clone(),
        unsupported_reason,
    }
}

fn unsupported(error_source: &SourceName, reason: &'static str) -> GenericError {
    GenericError::Unsupported {
        source: error_source.clone(),
        reason,
    }
}

#[cfg(feature = "audio-decoder")]
impl From<SeekErrorKind> for super::audio::SeekError {
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
