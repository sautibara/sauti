//! Various data structures that are used throughout the crate
use std::fmt::{Debug, Display};
use std::iter::once;
use std::path::{Path, PathBuf};
use std::{cmp::Ordering, convert::identity, ops::Range};

#[cfg(feature = "output")]
use crate::output::DeviceInfo;

/// An enum representing the acceptable sound sample types
pub use cpal::SampleFormat;
/// A sound sample that's represented by [`SampleFormat`]
pub use cpal::SizedSample;

pub use dasp_sample::{FromSample, Sample, ToSample};
use thiserror::Error;

/// Some useful things to have when working with the different data types in this crate
///
/// Every other module's prelude already exports this, so it's rare that it will have to be
/// imported by itself
pub mod prelude {
    pub use super::{
        ConvertibleSample, GenericPacket, MediaSource, Sample, SampleFormat, SoundPacket,
        SourceName, StreamSpec,
    };
}

/// Supertrait of [`Sample`] and conversions from all others
pub trait ConvertibleSample:
    SizedSample
    + FromSample<i8>
    + FromSample<i16>
    + FromSample<i32>
    + FromSample<i64>
    + FromSample<u8>
    + FromSample<u16>
    + FromSample<u32>
    + FromSample<u64>
    + FromSample<f32>
    + FromSample<f64>
    + ToSample<i8>
    + ToSample<i16>
    + ToSample<i32>
    + ToSample<i64>
    + ToSample<u8>
    + ToSample<u16>
    + ToSample<u32>
    + ToSample<u64>
    + ToSample<f32>
    + ToSample<f64>
    + Send
    + Sync
    + Debug
    + 'static
{
}

impl<T> ConvertibleSample for T where
    T: SizedSample
        + FromSample<i8>
        + FromSample<i16>
        + FromSample<i32>
        + FromSample<i64>
        + FromSample<u8>
        + FromSample<u16>
        + FromSample<u32>
        + FromSample<u64>
        + FromSample<f32>
        + FromSample<f64>
        + ToSample<i8>
        + ToSample<i16>
        + ToSample<i32>
        + ToSample<i64>
        + ToSample<u8>
        + ToSample<u16>
        + ToSample<u32>
        + ToSample<u64>
        + ToSample<f32>
        + ToSample<f64>
        + Debug
        + Send
        + Sync
        + 'static
{
}

/// A source for a sound file
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MediaSource {
    Path(PathBuf),
    Buffer(Box<[u8]>),
}

impl MediaSource {
    /// Create [`Self`] by copying `buf` into a [`Box`]
    #[must_use]
    pub fn copy_buf(buf: &[u8]) -> Self {
        Self::Buffer(buf.iter().copied().collect())
    }
}

impl<T: AsRef<Path>> From<T> for MediaSource {
    fn from(value: T) -> Self {
        Self::Path(value.as_ref().to_owned())
    }
}

impl Display for MediaSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Path(path) => write!(f, "path `{}`", path.display()),
            Self::Buffer(_) => write!(f, "buffer"),
        }
    }
}

/// A description for a [`MediaSource`]
#[derive(Error, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SourceName {
    #[error("file '{0}'")]
    File(PathBuf),
    #[error("buffer")]
    Buffer,
    #[error("unknown source")]
    Unknown,
}

impl From<&MediaSource> for SourceName {
    fn from(value: &MediaSource) -> Self {
        match value {
            MediaSource::Buffer(_) => Self::Buffer,
            MediaSource::Path(path) => Self::File(path.to_owned()),
        }
    }
}

/// Stores information about an audio stream
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StreamSpec {
    pub channels: usize,
    pub sample_rate: usize,
}

impl Default for StreamSpec {
    fn default() -> Self {
        DeviceInfo::default().into()
    }
}

#[cfg(feature = "output")]
impl From<DeviceInfo> for StreamSpec {
    fn from(value: DeviceInfo) -> Self {
        Self {
            channels: value.channels,
            sample_rate: value.sample_rate,
        }
    }
}

/// A generic form of [`SoundPacket`]
///
/// Use [`Self::convert`] to turn this into a more useable type
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

macro_rules! get {
    ($val:ident => |$packets:ident| $expr:expr) => {
        match $val {
            GenericPacket::I8($packets) => $expr,
            GenericPacket::I16($packets) => $expr,
            GenericPacket::I32($packets) => $expr,
            GenericPacket::I64($packets) => $expr,
            GenericPacket::U8($packets) => $expr,
            GenericPacket::U16($packets) => $expr,
            GenericPacket::U32($packets) => $expr,
            GenericPacket::U64($packets) => $expr,
            GenericPacket::F32($packets) => $expr,
            GenericPacket::F64($packets) => $expr,
        }
    };
}

impl GenericPacket {
    /// Convert this packet into a typed [`SoundPacket`]
    pub fn convert<N: ConvertibleSample>(&self) -> SoundPacket<N> {
        self.into()
    }

    /// Get the [`StreamSpec`] for this packet
    #[must_use]
    pub const fn spec(&self) -> &StreamSpec {
        get!(self => |packet| packet.spec())
    }

    /// Get the number of frames that this packet contains
    #[must_use]
    pub fn frames(&self) -> usize {
        get!(self => |packet| packet.frames())
    }

    /// Get the underlying [`SampleFormat`] of the packet
    #[must_use]
    pub const fn sample_format(&self) -> SampleFormat {
        match self {
            Self::I8(_) => SampleFormat::I8,
            Self::I16(_) => SampleFormat::I16,
            Self::I32(_) => SampleFormat::I32,
            Self::I64(_) => SampleFormat::I64,
            Self::U8(_) => SampleFormat::U8,
            Self::U16(_) => SampleFormat::U16,
            Self::U32(_) => SampleFormat::U32,
            Self::U64(_) => SampleFormat::U64,
            Self::F32(_) => SampleFormat::F32,
            Self::F64(_) => SampleFormat::F64,
        }
    }

    /// Convert the underlying sample format to be `format`
    #[must_use]
    pub fn convert_to(&self, format: SampleFormat) -> Self {
        match format {
            SampleFormat::I8 => Self::I8(self.convert()),
            SampleFormat::I16 => Self::I16(self.convert()),
            SampleFormat::I32 => Self::I32(self.convert()),
            SampleFormat::I64 => Self::I64(self.convert()),
            SampleFormat::U8 => Self::U8(self.convert()),
            SampleFormat::U16 => Self::U16(self.convert()),
            SampleFormat::U32 => Self::U32(self.convert()),
            SampleFormat::U64 => Self::U64(self.convert()),
            SampleFormat::F32 => Self::F32(self.convert()),
            SampleFormat::F64 => Self::F64(self.convert()),
            _ => todo!("GenericPacket cannot hold this type of format yet"),
        }
    }

    /// Joins the samples of two packets together
    ///
    /// # Panics
    ///
    /// - If the [`StreamSpec`]s of the two packets mismatch
    #[must_use]
    pub fn join(self, other: &Self) -> Self {
        get!(self => |packet| packet.join(&other.convert()).into())
    }

    /// Split a packet in two at `index`, which is counted as a frame
    ///
    /// # Panics
    ///
    /// - If `index` is out of bounds of this packet
    #[must_use]
    pub fn split(&self, index: usize) -> (Self, Self) {
        get!(self => |packet| {
            let (left, right) = packet.split(index);
            (left.into(), right.into())
        })
    }

    /// Returns `true` if `self` is "equivalent" to `other`
    ///
    /// The underlying sample formats don't have to be the same, but once converted to the same format,
    /// the samples must be equal to each other.
    #[must_use]
    pub fn equivalent(&self, other: &Self) -> bool {
        self.convert_to(other.sample_format()) == *other
    }
}

impl<N: ConvertibleSample> From<&GenericPacket> for SoundPacket<N> {
    fn from(value: &GenericPacket) -> Self {
        get!(value => |packet| packet.convert())
    }
}

impl<N: ConvertibleSample> From<GenericPacket> for SoundPacket<N> {
    fn from(value: GenericPacket) -> Self {
        (&value).into()
    }
}

impl<S: ConvertibleSample> From<&SoundPacket<S>> for GenericPacket {
    fn from(value: &SoundPacket<S>) -> Self {
        match S::FORMAT {
            SampleFormat::I8 => Self::I8(value.convert()),
            SampleFormat::I16 => Self::I16(value.convert()),
            SampleFormat::I32 => Self::I32(value.convert()),
            SampleFormat::I64 => Self::I64(value.convert()),
            SampleFormat::U8 => Self::U8(value.convert()),
            SampleFormat::U16 => Self::U16(value.convert()),
            SampleFormat::U32 => Self::U32(value.convert()),
            SampleFormat::U64 => Self::U64(value.convert()),
            SampleFormat::F32 => Self::F32(value.convert()),
            SampleFormat::F64 => Self::F64(value.convert()),
            _ => unimplemented!("generic packet of that type not implemented yet"),
        }
    }
}

impl<S: ConvertibleSample> From<SoundPacket<S>> for GenericPacket {
    fn from(value: SoundPacket<S>) -> Self {
        (&value).into()
    }
}

/// An immutable packet of sound data, potentially with multiple channels
///
/// Terminology:
/// - A `sample` is any single value representing sound - it represents the compression or
/// expansion of the air
/// - A `channel` is a list of samples coming from a single direction - there's usually one for
/// each ear (stereo)
/// - A `frame` represents a single instance in time, combining each channel's current sample
/// together
///
/// The packet also includes a [`StreamSpec`] that holds information about the stream (most notably
/// its sample rate)
#[must_use]
#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SoundPacket<S: ConvertibleSample> {
    interleaved_samples: Vec<S>,
    spec: StreamSpec,
}

impl<S: ConvertibleSample> SoundPacket<S> {
    /// Creates a sound packet from an interleaved vector of samples.
    ///
    /// Each frame of audio consists of a slice of channels in order.
    ///
    /// # Panics
    ///
    /// - If `spec.channels` doesn't evenly fit into `samples`
    ///
    /// # Examples
    ///
    /// ```
    /// use sauti::data::prelude::*;
    ///
    /// let spec = StreamSpec { channels: 2, sample_rate: 44100 };
    /// let samples = vec![1, 2, 3, 4];
    /// let packet = SoundPacket::from_interleaved(samples, spec);
    ///
    /// assert_eq!(
    ///     packet.frame_iter()
    ///           .collect::<Vec<_>>(),
    ///     [[1, 2], [3, 4]]
    /// );
    /// ```
    pub fn from_interleaved(samples: Vec<S>, spec: StreamSpec) -> Self {
        assert!(samples.len() % spec.channels == 0, "the interleaved samples should have the same amount of samples for each channel (samples.len % channels == 0)");
        Self {
            interleaved_samples: samples,
            spec,
        }
    }

    /// Creates a sound packet from a list of channels for the packet
    ///
    /// # Examples
    ///
    /// ```
    /// use sauti::data::prelude::*;
    ///
    /// // Channels are separated at the top level
    /// let samples = [[1, 2, 3], [4, 5, 6]];
    /// let packet = SoundPacket::from_channels(&samples, 44100);
    ///
    /// assert_eq!(
    ///     packet.frame_iter().collect::<Vec<_>>(),
    ///     [[1, 4], [2, 5], [3, 6]]
    /// );
    /// ```
    pub fn from_channels<V: AsRef<[S]>>(samples: &[V], sample_rate: usize) -> Self {
        let channels = samples.len();
        let frames = samples.first().map_or(0, |slice| slice.as_ref().len());
        let mut interleaved_samples = vec![S::EQUILIBRIUM; channels * frames];

        for (channel, samples) in samples.iter().enumerate() {
            for (frame, sample) in samples.as_ref().iter().enumerate() {
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

    pub(crate) fn copy_from_channels<V: AsRef<[S]>>(&mut self, samples: &[V], frames: usize) {
        let channels = samples.len();
        if self.interleaved_samples.len() < channels * frames {
            self.interleaved_samples
                .resize(channels * frames, S::EQUILIBRIUM);
        }

        for (channel, samples) in samples.iter().enumerate() {
            for (frame, sample) in samples.as_ref().iter().take(frames).enumerate() {
                self.interleaved_samples[frame * channels + channel] = *sample;
            }
        }

        if self.interleaved_samples.len() > channels * frames {
            self.interleaved_samples
                .resize(channels * frames, S::EQUILIBRIUM);
        }
    }

    /// Converts this sound packet to a list of audio channels
    ///
    /// This is the reciprocal of [`Self::from_channels`]
    ///
    /// Also see [`Self::copy_to_channels`] for an option that doesn't always allocate data
    ///
    /// # Examples
    ///
    /// ```
    /// use sauti::data::prelude::*;
    ///
    /// let channels = vec![vec![1, 2, 3], vec![4, 5, 6]];
    /// let packet = SoundPacket::from_channels(&channels, 44100);
    ///
    /// assert_eq!(channels, packet.to_channels());
    /// ```
    pub fn to_channels(&self) -> Vec<Vec<S>> {
        let mut channels = vec![vec![S::EQUILIBRIUM; self.frames()]; self.channels()];
        self.copy_to_channels(&mut channels);
        channels
    }

    /// Copies samples from this sound packet to a list of audio channels
    ///
    /// # Examples
    ///
    /// ```
    /// use sauti::data::prelude::*;
    ///
    /// let channels = vec![vec![1, 2, 3], vec![4, 5, 6]];
    /// let mut channels_out = vec![vec![0; 3]; 2];
    ///
    /// let packet = SoundPacket::from_channels(&channels, 44100);
    /// packet.copy_to_channels(&mut channels_out);
    ///
    /// assert_eq!(channels, channels_out);
    /// ```
    pub fn copy_to_channels(&self, channels: &mut Vec<Vec<S>>) {
        let frame_count = self.frames();
        let channel_count = self.channels();
        // make sure there's enough channels in the output
        if channels.len() < channel_count {
            channels.resize(channel_count, vec![S::EQUILIBRIUM; frame_count]);
        }
        // and enough frames in each channel
        for channel in channels
            .iter_mut()
            .filter(|channel| channel.len() < frame_count)
        {
            channel.resize(frame_count, S::EQUILIBRIUM);
        }
        // then copy over all of the samples
        self.copy_to_channels_unchecked(channels);
    }

    pub(crate) fn copy_to_channels_unchecked(&self, channels: &mut [Vec<S>]) {
        let channel_count = self.channels();
        for frame in 0..self.frames() {
            let frame_index = frame * channel_count;
            let frame_slice = &self.interleaved_samples[frame_index..frame_index + channel_count];
            for channel in 0..channel_count {
                channels[channel][frame] = frame_slice[channel];
            }
        }
    }

    #[must_use]
    pub const fn spec(&self) -> &StreamSpec {
        &self.spec
    }

    #[must_use]
    pub const fn channels(&self) -> usize {
        self.spec.channels
    }

    #[must_use]
    pub const fn sample_rate(&self) -> usize {
        self.spec.sample_rate
    }

    #[must_use]
    pub fn frames(&self) -> usize {
        (self.interleaved_samples.len())
            .checked_div(self.channels())
            .unwrap_or(0)
    }

    /// Obtain an iterator over each frame of the packet
    ///
    /// # Examples
    ///
    /// ```
    /// use sauti::data::prelude::*;
    ///
    /// let samples = vec![1, 2, 3, 4];
    /// let spec = StreamSpec { channels: 2, sample_rate: 44100 };
    /// let packet = SoundPacket::from_interleaved(samples, spec);
    ///
    /// assert_eq!(
    ///     packet.frame_iter()
    ///           .collect::<Vec<_>>(),
    ///     [[1, 2], [3, 4]]
    /// );
    /// ```
    #[must_use]
    pub fn frame_iter(&self) -> impl DoubleEndedIterator<Item = &[S]> {
        self.interleaved_samples.chunks_exact(self.channels())
    }

    /// Obtain a slice of all samples in an interleaved order
    ///
    /// This is the reciprocal of [`Self::from_interleaved`]
    ///
    /// # Examples
    ///
    /// ```
    /// use sauti::data::prelude::*;
    ///
    /// let samples = vec![1, 2, 3, 4];
    /// let spec = StreamSpec { channels: 2, sample_rate: 44100 };
    /// let packet = SoundPacket::from_interleaved(samples.clone(), spec);
    ///
    /// assert_eq!(samples, packet.interleaved_samples());
    /// ```
    #[must_use]
    pub fn interleaved_samples(&self) -> &[S] {
        &self.interleaved_samples[..]
    }

    /// Access a single channel of the packet
    ///
    /// # Panics
    ///
    /// - If `index` is out of bounds for the amount of channels
    ///
    /// # Examples
    ///
    /// ```
    /// use sauti::data::prelude::*;
    ///
    /// let channels = [[1, 2, 3], [4, 5, 6]];
    /// let packet = SoundPacket::from_channels(&channels, 44100);
    ///
    /// assert_eq!(
    ///     packet.chan(1).collect::<Vec<_>>(),
    ///     channels[1]
    /// );
    /// ```
    pub fn chan(&self, index: usize) -> impl Iterator<Item = S> + '_ {
        assert!(
            index < self.channels(),
            "accessed channel index shouldn't be more than the amount of channels in the packet"
        );
        // equivalent to self.frame_iter().map(move |channels| channels[index])
        self.interleaved_samples
            .iter()
            .copied()
            .skip(index)
            .step_by(index + 1)
    }

    /// Resize the channels of this packet by mapping each frame
    ///
    /// # Examples
    ///
    /// ```
    /// use sauti::data::prelude::*;
    ///
    /// let channels = [[1, 2, 3], [3, 4, 5]];
    /// let packet = SoundPacket::from_channels(&channels, 44100);
    ///
    /// // converts to mono by averaging all channels
    /// let mapped = packet.resize_and_map_channels(1,
    ///     |frame, from_channels, _to_channels| {
    ///         let average = frame.iter().sum::<u32>() / from_channels as u32;
    ///         frame.fill(average);
    ///     }
    /// );
    ///
    /// assert_eq!(
    ///     mapped.to_channels(),
    ///     [[2, 3, 4]]
    /// );
    /// ```
    pub fn resize_and_map_channels<F>(mut self, to_channels: usize, mut map: F) -> Self
    where
        F: FnMut(&mut [S], usize, usize),
    {
        let from_channels = self.channels();
        match to_channels.cmp(&from_channels) {
            // if the channels don't have to be resized, just map them
            Ordering::Equal => {
                self.map_frames(&mut map, from_channels, to_channels);
            }
            // there are less channels than before, so they're getting compressed
            Ordering::Less => {
                // map the frames now because they'll get compressed later
                self.map_frames(&mut map, from_channels, to_channels);
                // move each sample sequentually because they'll get less room
                self.move_samples_for_resize(from_channels, to_channels, identity);
                // resize the sample vector to the new, smaller size
                self.resize_to_fit_channels(to_channels);
            }
            // there are more channels than before, so they're getting expanded
            Ordering::Greater => {
                // resize the sample vector to fit the extra space
                self.resize_to_fit_channels(to_channels);
                // move each sample in reverse to not overwrite upcoming frames
                self.move_samples_for_resize(from_channels, to_channels, Iterator::rev);
                // map the frames now because they have the extra room
                self.map_frames(&mut map, from_channels, to_channels);
            }
        }
        // everything else is the same
        self
    }

    fn resize_to_fit_channels(&mut self, to_channels: usize) {
        self.interleaved_samples
            .resize(to_channels * self.frames(), S::EQUILIBRIUM);
        // the amount of channels are changed now
        self.spec.channels = to_channels;
    }

    fn map_frames<F>(&mut self, map: &mut F, from_channels: usize, to_channels: usize)
    where
        F: FnMut(&mut [S], usize, usize),
    {
        let frames = self.frames();
        let channels = self.channels();
        for frame in 0..frames {
            let frame_index = frame * channels;
            map(
                &mut self.interleaved_samples[frame_index..frame_index + channels],
                from_channels,
                to_channels,
            );
        }
    }

    fn move_samples_for_resize<O: Iterator<Item = usize>>(
        &mut self,
        from_channels: usize,
        to_channels: usize,
        // the iterators might have to be reversed if the space available is expanding
        iter_map: impl Fn(Range<usize>) -> O,
    ) {
        let frames = self.frames();
        // the amount of channels that will be copied
        let copy_channels = from_channels.min(to_channels);
        // go through each frame and copy over the channels
        for frame in iter_map(0..frames) {
            let from_frame = frame * from_channels;
            let to_frame = frame * to_channels;
            // copy over each channels
            for channel in iter_map(0..copy_channels) {
                self.interleaved_samples
                    .swap(from_frame + channel, to_frame + channel);
            }
        }
    }

    /// Joins the samples of two packets together
    ///
    /// # Panics
    ///
    /// - If the [`StreamSpec`]s of the two packets mismatch
    pub fn join(mut self, other: &Self) -> Self {
        assert!(
            self.spec() == other.spec(),
            "StreamSpecs of two joined packets should be the same"
        );
        self.interleaved_samples
            .extend_from_slice(other.interleaved_samples());
        self
    }

    /// Split a packet in two at `index`, which is counted as a frame
    ///
    /// # Panics
    ///
    /// - If `index` is out of bounds of this packet (`index` > `self.frames()`)
    pub fn split(&self, index: usize) -> (Self, Self) {
        assert!(
            index <= self.frames(),
            "index of split should be in-bounds of packet"
        );

        let (left, right) = self.interleaved_samples.split_at(index * self.channels());

        (
            Self::from_interleaved(Vec::from(left), *self.spec()),
            Self::from_interleaved(Vec::from(right), *self.spec()),
        )
    }

    /// Convert the packet to a new type
    ///
    /// # Examples
    ///
    /// ```
    /// use sauti::data::prelude::*;
    ///
    /// let channels = [[i32::EQUILIBRIUM]];
    ///
    /// let packet = SoundPacket::from_channels(&channels, 44100);
    /// let converted = packet.convert::<f32>();
    ///
    /// assert_eq!(
    ///     converted.interleaved_samples()[0],
    ///     f32::EQUILIBRIUM
    /// );
    /// ```
    pub fn convert<N: ConvertibleSample>(&self) -> SoundPacket<N>
    where
        S: ToSample<N>,
    {
        let interleaved_samples = self
            .interleaved_samples
            .iter()
            .map(|sample| sample.to_sample())
            .collect();
        SoundPacket {
            interleaved_samples,
            spec: self.spec,
        }
    }

    /// Map each sample with the provided function
    ///
    /// ```
    /// use sauti::data::prelude::*;
    ///
    /// let samples = vec![1.0, 2.0, 3.0, 4.0];
    /// let spec = StreamSpec { channels: 2, sample_rate: 44100 };
    /// let packet = SoundPacket::from_interleaved(samples, spec);
    ///
    /// let mapped = packet.map_samples(|sample| sample * 2.0);
    ///
    /// assert_eq!(
    ///     mapped.interleaved_samples(),
    ///     &[2.0, 4.0, 6.0, 8.0]
    /// );
    /// ```
    pub fn map_samples(mut self, mut map: impl FnMut(&S) -> S) -> Self {
        for sample in &mut self.interleaved_samples {
            *sample = map(sample);
        }
        self
    }
}

impl<S: ConvertibleSample> FromIterator<SoundPacket<S>>
    for Option<Result<SoundPacket<S>, SpecMismatch>>
{
    fn from_iter<T: IntoIterator<Item = SoundPacket<S>>>(iter: T) -> Self {
        let mut packets = iter.into_iter();
        let first = packets.next()?;
        let packet = packets.try_fold(first, |mut previous, current| {
            if previous.spec() == current.spec() {
                previous
                    .interleaved_samples
                    .extend_from_slice(&current.interleaved_samples[..]);
                Ok(previous)
            } else {
                Err(SpecMismatch)
            }
        });
        Some(packet)
    }
}

impl FromIterator<GenericPacket> for Option<Result<GenericPacket, SpecMismatch>> {
    fn from_iter<T: IntoIterator<Item = GenericPacket>>(iter: T) -> Self {
        let mut packets = iter.into_iter();
        let first = packets.next()?;
        get!(first => |first| {
            let list = once(first).chain(packets.map(|packet| packet.convert()));
            let packet: Option<Result<SoundPacket<_>, _>> = list.collect();
            match packet {
                Some(Ok(packet)) => Some(Ok(GenericPacket::from(packet))),
                Some(Err(e)) => Some(Err(e)),
                None => None,
            }
        })
    }
}

/// An error for when sound packets with different [`StreamSpec`]s try to be collected into a
/// single packet
#[derive(Debug, Error)]
#[error("Tried to collect sound packets with different StreamSpecs into a single packet")]
pub struct SpecMismatch;
