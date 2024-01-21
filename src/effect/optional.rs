use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use crate::{
    audio::ConvertibleSample,
    file::{SoundPacket, StreamSpec},
};

use super::Effect;

pub struct Optional<E: Effect> {
    pub current: E,
    pub handle: OptionalHandle,
}

impl<E: Effect> Optional<E> {
    pub fn new(effect: E, activated: bool) -> (Self, OptionalHandle) {
        let handle = OptionalHandle::new(activated);
        (
            Self {
                current: effect,
                handle: handle.clone(),
            },
            handle,
        )
    }

    pub fn with_handle(effect: E, handle: OptionalHandle) -> Self {
        Self {
            current: effect,
            handle,
        }
    }
}

#[derive(Clone)]
pub struct OptionalHandle(Arc<AtomicBool>);

impl OptionalHandle {
    pub fn new(activated: bool) -> Self {
        Self(Arc::new(AtomicBool::new(activated)))
    }

    pub fn is_activated(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    pub fn set(&self, new: bool) {
        self.0.store(new, Ordering::Relaxed);
    }

    pub fn activate(&self) {
        self.set(true);
    }

    pub fn deactivate(&self) {
        self.set(false);
    }
}

impl<E: Effect> Effect for Optional<E> {
    #[inline(always)]
    fn apply<S: ConvertibleSample>(
        &mut self,
        input: SoundPacket<S>,
        output_spec: &StreamSpec,
    ) -> SoundPacket<S> {
        if self.handle.is_activated() {
            self.current.apply(input, output_spec)
        } else {
            input
        }
    }
}
