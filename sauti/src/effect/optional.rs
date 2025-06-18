use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use super::prelude::*;

/// Applies the inner [`Effect`] optionally, based on the atomic [`Handle`]
#[derive(Clone)]
pub struct Optional<E: Effect> {
    pub current: E,
    pub handle: Handle,
}

impl<E: Effect> Optional<E> {
    pub fn new(effect: E, activated: bool) -> (Self, Handle) {
        let handle = Handle::new(activated);
        (
            Self {
                current: effect,
                handle: handle.clone(),
            },
            handle,
        )
    }

    pub const fn with_handle(effect: E, handle: Handle) -> Self {
        Self {
            current: effect,
            handle,
        }
    }
}

/// A handle for [`Optional`], able to activate or deactivate the inner [`Effect`]
#[derive(Clone)]
pub struct Handle(Arc<AtomicBool>);

impl Handle {
    #[must_use]
    pub fn new(activated: bool) -> Self {
        Self(Arc::new(AtomicBool::new(activated)))
    }

    #[must_use]
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
    #[inline]
    fn apply_to<S: ConvertibleSample>(
        &mut self,
        input: SoundPacket<S>,
        output_spec: &StreamSpec,
    ) -> SoundPacket<S> {
        if self.handle.is_activated() {
            self.current.apply_to(input, output_spec)
        } else {
            input
        }
    }
}
