use std::sync::Arc;

use super::prelude::*;

use crossbeam::atomic::AtomicCell;
use dasp_sample::Sample;

/// Applies a changeable volume using an atomic [`Handle`]
///
/// [`Player`](crate::player::Player) already has its own implementation of volume, but this could
/// also be used if necessary.
#[derive(Clone)]
pub struct Volume(pub Handle);

impl Volume {
    #[must_use]
    pub fn create_handle(initial: f64) -> Handle {
        Handle::new(initial)
    }

    #[must_use]
    pub fn constant(initial: f64) -> Self {
        let handle = Self::create_handle(initial);
        Self(handle)
    }
}

impl Effect for Volume {
    fn apply_to<S: ConvertibleSample>(
        &mut self,
        input: SoundPacket<S>,
        spec: &StreamSpec,
    ) -> SoundPacket<S> {
        Constant(self.0.get()).apply_to(input, spec)
    }
}

/// A [`Volume`] handle, able to get or set the current volume
#[derive(Clone)]
pub struct Handle(Arc<AtomicCell<f64>>);

impl Handle {
    pub fn new(initial: f64) -> Self {
        Self(Arc::new(AtomicCell::new(initial)))
    }

    pub fn get(&self) -> f64 {
        self.0.load()
    }

    pub fn set(&self, new: f64) {
        self.0.store(new);
    }
}

/// Applies a constant volume to incoming [`SoundPacket`]s
#[derive(Clone)]
pub struct Constant(pub f64);

impl Effect for Constant {
    fn apply_to<S: ConvertibleSample>(
        &mut self,
        input: SoundPacket<S>,
        _: &StreamSpec,
    ) -> SoundPacket<S> {
        #[allow(clippy::float_cmp)] // 1.0 is a concrete value
        if self.0 == 1.0 {
            input
        } else {
            let mul = S::from_sample(self.0).to_float_sample();
            input.map_samples(|sample| (sample.to_float_sample() * mul).to_sample())
        }
    }
}
