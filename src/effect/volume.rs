use std::sync::{atomic::Ordering, Arc};

use crate::data::{ConvertibleSample, SoundPacket, StreamSpec};

use super::Effect;

use atomic_float::AtomicF32;
use dasp_sample::Sample;

#[derive(Clone)]
pub struct Volume(pub VolumeHandle);

impl Volume {
    pub fn create_handle(initial: f32) -> VolumeHandle {
        VolumeHandle::new(initial)
    }
}

impl Effect for Volume {
    fn apply_to<S: ConvertibleSample>(
        &mut self,
        input: SoundPacket<S>,
        _: &StreamSpec,
    ) -> SoundPacket<S> {
        let mul = S::from_sample(self.0.get()).to_float_sample();
        input.map_samples(|sample| (sample.to_float_sample() * mul).to_sample())
    }
}

#[derive(Clone)]
pub struct VolumeHandle(Arc<AtomicF32>);

impl VolumeHandle {
    pub fn new(initial: f32) -> Self {
        Self(Arc::new(AtomicF32::new(initial)))
    }

    pub fn get(&self) -> f32 {
        self.0.load(Ordering::Relaxed)
    }

    pub fn set(&self, new: f32) {
        self.0.store(new, Ordering::Relaxed);
    }
}
