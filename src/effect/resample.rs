#![allow(clippy::cast_precision_loss)] // the sample rates shouldn't be that big

use rubato::{
    Resampler as _, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};

use super::prelude::*;

/// # Panics
///
/// - If the input amount of channels is different than the output
#[derive(Default)]
pub struct Resample {
    resampler: Option<Inner>,
}

impl Clone for Resample {
    fn clone(&self) -> Self {
        Self { resampler: None }
    }
}

impl Resample {
    fn resampler(
        &mut self,
        input_spec: &StreamSpec,
        output_spec: &StreamSpec,
        input_frames: usize,
    ) -> &mut Inner {
        if !self
            .resampler
            .as_ref()
            .is_some_and(|resampler| resampler.matches(input_spec, output_spec, input_frames))
        {
            let new = Inner::new(*input_spec, *output_spec, input_frames);
            self.resampler = Some(new);
        }

        // SAFETY: a None variant would be replaced with Some above
        // this can't be done the normal way (using an if let block) because of a limitation in the
        // borrow checker (see https://rust-lang.github.io/rfcs/2094-nll.html#problem-case-3-conditional-control-flow-across-functions)
        unsafe { self.resampler.as_mut().unwrap_unchecked() }
    }
}

impl Effect for Resample {
    fn apply_to<S: ConvertibleSample>(
        &mut self,
        input: SoundPacket<S>,
        output_spec: &StreamSpec,
    ) -> SoundPacket<S> {
        let input_spec = input.spec();
        // if the sample rates are the same, then there's no resampling to be done
        if input_spec.sample_rate == output_spec.sample_rate {
            return input;
        }

        let input_frames = input.frames();
        let resampler = self.resampler(input_spec, output_spec, input_frames);
        let processed = resampler.process(input.convert());
        processed.convert()
    }
}

struct Inner {
    resampler: SincFixedIn<f64>,
    input_buffer: Vec<Vec<f64>>,
    output_buffer: Vec<Vec<f64>>,
    input_spec: StreamSpec,
    output_spec: StreamSpec,
    input_frames: usize,
}

impl Inner {
    const fn default_params() -> SincInterpolationParameters {
        SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        }
    }

    fn new(input_spec: StreamSpec, output_spec: StreamSpec, input_frames: usize) -> Self {
        assert!(
            input_spec.channels == output_spec.channels,
            "input channel count should be the same as the output before resampling"
        );

        let resampler = SincFixedIn::new(
            output_spec.sample_rate as f64 / input_spec.sample_rate as f64,
            5.0, // this notably affects how big output_buffer will be
            Self::default_params(),
            input_frames,
            input_spec.channels,
        )
        .expect("resample ratio should be within acceptable bounds");

        let input_buffer = resampler.input_buffer_allocate(true);
        let output_buffer = resampler.output_buffer_allocate(true);

        Self {
            resampler,
            input_buffer,
            output_buffer,
            input_spec,
            output_spec,
            input_frames,
        }
    }

    fn matches(
        &self,
        input_spec: &StreamSpec,
        output_spec: &StreamSpec,
        input_frames: usize,
    ) -> bool {
        self.input_spec == *input_spec
            && self.output_spec == *output_spec
            && self.input_frames == input_frames
    }

    fn process(&mut self, mut packet: SoundPacket<f64>) -> SoundPacket<f64> {
        packet.copy_to_channels_unchecked(&mut self.input_buffer);
        let (_, frames) = self
            .resampler
            .process_into_buffer(&self.input_buffer, &mut self.output_buffer, None)
            .expect("resampler shouldn't fail if parameters are correct");
        packet.copy_from_channels(&self.output_buffer, frames);
        packet
    }
}
