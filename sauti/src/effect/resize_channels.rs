use std::ops::Add;

use dasp_sample::Sample;

use super::prelude::*;

/// Resize the given channels to fit the output [`StreamSpec`]
///
/// If the packet doesn't need to be reisized, then it does nothing. If it does, then it fills
/// every output channel with the average of the input channels.
#[derive(Clone)]
pub struct ResizeChannels;

impl ResizeChannels {
    fn map_channels<S: ConvertibleSample>(
        frame: &mut [S],
        from_channels: usize,
        to_channels: usize,
    ) {
        // if the channels are already fine, then do nothing
        if from_channels == to_channels || to_channels == 0 || frame.is_empty() {
            return;
        }
        // find the average value of all of the channels
        let average = if from_channels == 1 {
            frame[0]
        } else {
            let sum = (frame.iter())
                // only signed samples can multiply
                .map(|sample| sample.to_signed_sample())
                // Sample doesn't implement Sum, so use reduce instead
                .reduce(Add::add)
                // already checked `frame.is_empty()` above
                .expect("reduce on a non-empty iterator should always return something")
                // bring it back to the original sample type
                .to_sample::<S>();
            // it's very unlikely that there will be more channels than a u32 could handle
            #[allow(clippy::cast_possible_truncation)]
            let amount = S::from_sample(from_channels as u32);
            (sum.to_float_sample() / amount.to_float_sample()).to_sample::<S>()
        };
        // fill the chnnels with the average
        frame.fill(average);
    }
}

impl Effect for ResizeChannels {
    fn apply_to<S: ConvertibleSample>(
        &mut self,
        input: SoundPacket<S>,
        output_spec: &StreamSpec,
    ) -> SoundPacket<S> {
        // only resize channels if it's necessary
        if output_spec.channels == input.channels() {
            input
        } else {
            input.resize_and_map_channels(output_spec.channels, Self::map_channels)
        }
    }
}
