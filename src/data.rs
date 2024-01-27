use std::{cmp::Ordering, convert::identity, ops::Range};

use crate::audio::DeviceInfo;

pub use dasp_sample::{FromSample, Sample, ToSample};

pub mod prelude {
    pub use super::{ConvertibleSample, GenericPacket, SoundPacket, StreamSpec};
}

/// Supertrait of [`Sample`] and conversions from all others
pub trait ConvertibleSample:
    cpal::SizedSample
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
    + 'static
{
}

impl<T> ConvertibleSample for T where
    T: cpal::SizedSample
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
        + 'static
{
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct StreamSpec {
    pub channels: usize,
    pub sample_rate: usize,
}

impl From<DeviceInfo> for StreamSpec {
    fn from(value: DeviceInfo) -> Self {
        Self {
            channels: value.channels,
            sample_rate: value.sample_rate,
        }
    }
}

#[must_use]
#[derive(Clone)]
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

#[must_use]
#[derive(Clone)]
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
    /// let samples: Vec<i32> = vec![1, 2, 3, 4];
    /// let packet = SoundPacket::from_interleaved(samples, spec);
    ///
    /// assert_eq!(
    ///     &*packet.frame_iter().collect::<Box<[_]>>(),
    ///     &[&[1, 2], &[3, 4]]
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
    ///     &*packet.frame_iter().collect::<Box<[_]>>(),
    ///     &[&[1, 4], &[2, 5], &[3, 6]]
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
    }

    /// Converts this sound packet to a list of audio channels
    ///
    /// This is the reciprocal of [`Self::from_channels`]
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
    ///     channels,
    ///     packet.to_channels(),
    /// );
    /// ```
    pub fn to_channels(&self) -> Vec<Vec<S>> {
        let mut channels = vec![vec![S::EQUILIBRIUM; self.frames()]; self.channels()];
        self.copy_to_channels(&mut channels);
        channels
    }

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
        self.interleaved_samples.len() / self.channels()
    }

    #[must_use]
    pub fn frame_iter(&self) -> impl DoubleEndedIterator<Item = &[S]> {
        self.interleaved_samples.chunks_exact(self.channels())
    }

    #[must_use]
    pub fn interleaved_samples(&self) -> &[S] {
        &self.interleaved_samples[..]
    }

    /// # Panics
    ///
    /// - If `channel` is more than the amount of channels in the packet
    pub fn chan(&self, channel: usize) -> impl Iterator<Item = &S> {
        assert!(
            channel < self.channels(),
            "accessed channel index shouldn't be more than the amount of channels in the packet"
        );
        self.frame_iter().map(move |channels| &channels[channel])
    }

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

    pub fn map_samples(mut self, mut map: impl FnMut(&S) -> S) -> Self {
        for sample in &mut self.interleaved_samples {
            *sample = map(sample);
        }
        self
    }
}
