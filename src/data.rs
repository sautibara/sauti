use std::{cmp::Ordering, convert::identity, ops::Range};

use crate::audio::DeviceInfo;

pub use dasp_sample::{FromSample, Sample, ToSample};

/// Supertrait of [`SizedSample`] and conversions from all others
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

#[derive(Clone, Copy)]
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

    pub fn frame_count(&self) -> usize {
        self.interleaved_samples().len() / self.channels()
    }

    pub fn frames(&self) -> impl DoubleEndedIterator<Item = &[S]> {
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
        self.frames().map(move |channels| &channels[channel])
    }

    pub fn to_channels(&self) -> Vec<Vec<S>> {
        let mut channels = vec![vec![S::EQUILIBRIUM; self.frame_count()]; self.channels()];
        self.copy_to_channels(&mut channels);
        channels
    }

    pub fn copy_to_channels(&self, channels: &mut Vec<Vec<S>>) {
        let frame_count = self.frame_count();
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
            channel.resize(frame_count, S::EQUILIBRIUM)
        }
        // then copy over all of the samples
        for frame in 0..self.frame_count() {
            let frame_index = frame * channel_count;
            let frame_slice = &self.interleaved_samples[frame_index..frame_index + channel_count];
            for channel in 0..channel_count {
                channels[channel][frame] = frame_slice[channel];
            }
        }
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
                self.move_samples_for_resize(from_channels, to_channels, |iter| iter.rev());
                // map the frames now because they have the extra room
                self.map_frames(&mut map, from_channels, to_channels);
            }
        }
        // everything else is the same
        self
    }

    fn resize_to_fit_channels(&mut self, to_channels: usize) {
        self.interleaved_samples
            .resize(to_channels * self.frame_count(), S::EQUILIBRIUM);
        // the amount of channels are changed now
        self.spec.channels = to_channels;
    }

    fn map_frames<F>(&mut self, map: &mut F, from_channels: usize, to_channels: usize)
    where
        F: FnMut(&mut [S], usize, usize),
    {
        let frames = self.frame_count();
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
        let frames = self.frame_count();
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

    pub fn map_samples(mut self, mut map: impl FnMut(&S) -> S) -> Self {
        for sample in self.interleaved_samples.iter_mut() {
            *sample = map(sample);
        }
        self
    }
}
