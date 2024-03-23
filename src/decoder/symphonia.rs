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

use super::{prelude::*, SeekError, SourceName};

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
        error_source: SourceName,
        hint: &Hint,
    ) -> DecoderResult<Box<dyn AudioStream>> {
        let source = MediaSourceStream::new(source, MediaSourceStreamOptions::default());

        debug!("Testing if symphonia can decode {error_source}");

        // read the format of the file (but don't decode yet)
        let reader = (self.probe)
            .format(
                hint,
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
            .ok_or_else(|| DecoderError::NoTracks(error_source.clone()))?
            // if find returns [`Err`], that means that there are no working tracks
            ?;

        // we suceeded!
        debug!("Symphonia can decode {error_source}");

        let stream = Stream {
            file: reader.format,
            decoder,
            times,
            source: error_source,
            time_base,
            track_id,
        };

        let is_vorbis =
            stream.decoder.codec_params().codec == symphonia::core::codecs::CODEC_TYPE_VORBIS;
        let default = Box::new(stream);
        // the vorbis implementation tends to spit out different sized packets
        if is_vorbis {
            trace!("symphonia is reading a vorbis track, using a buffered AudioStream");
            Ok(Box::new(buffered::AudioStream::wrap(default)))
        } else {
            Ok(default)
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
    ) -> DecoderResult<DecodedTrack> {
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
}

impl Decoder for Symphonia {
    fn read(&self, source: &MediaSource) -> super::DecoderResult<Box<dyn super::AudioStream>> {
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

        self.read_source(symphonia_source, error_source, &hint)
    }
}

use symphonia::core::codecs::Decoder as SymphoniaDecoder;
struct Stream {
    source: SourceName,
    file: Box<dyn FormatReader>,
    decoder: Box<dyn SymphoniaDecoder>,
    times: Arc<Times>,
    time_base: TimeBase,
    track_id: u32,
}

impl AudioStream for Stream {
    fn next_packet(&mut self) -> DecoderResult<Option<GenericPacket>> {
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
                Err(err) => return Err(map_err(err, &self.source, None)),
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

    fn seek_to(&mut self, duration: Duration) -> DecoderResult<()> {
        // check if the seek is out of bounds
        let frames = super::duration_to_frame(duration, self.times.sample_rate);
        if frames > self.times.frame_count {
            return Err(DecoderError::SeekError {
                source: self.source.clone(),
                reason: SeekError::OutOfBounds,
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
            return Err(DecoderError::SeekError {
                source: self.source.clone(),
                reason: SeekError::OutOfBounds,
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

use symphonia::core::errors::Error as SymphoniaError;
fn map_err(
    error: SymphoniaError,
    source: &SourceName,
    unsupported_reason: Option<&'static str>,
) -> DecoderError {
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
        SymphoniaError::Unsupported(reason) => DecoderError::UnsupportedFormat {
            source: source.clone(),
            reason: Some(
                unsupported_reason.map_or_else(String::new, |reason| format!("{reason}: "))
                    + reason,
            ),
        },
        SymphoniaError::LimitError(error) => DecoderError::Other(Some(error.to_string())),
        SymphoniaError::ResetRequired => {
            DecoderError::Other(Some("decoder needs reset".to_string()))
        }
    }
}

fn unsupported(error_source: &SourceName, reason: &'static str) -> DecoderError {
    DecoderError::UnsupportedFormat {
        source: error_source.clone(),
        reason: Some(reason.to_owned()),
    }
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
