//! Audio effects that can be applied on a [`SoundPacket`]
//!
//! To apply an effect, push a [`SoundPacket`] through [`Effect::apply_to`] or a [`GenericPacket`]
//! through [`Generic::apply_to_generic`], which is implemented for all effects.
//!
//! Effects are given each sound packet and the [`StreamSpec`] of the output stream and return a
//! new, modified effect.
//!
//! # Examples
//!
//! ```
//! use sauti::effect::prelude::*;
//!
//! let mono = SoundPacket::from_channels(&[&[1, 2]], 44100);
//! let mut resizer = effect::ResizeChannels;
//! // each effect takes in the [`StreamSpec`] of the output stream
//! let stereo = resizer.apply_to(mono, &StreamSpec { channels: 2, sample_rate: 44100 });
//!
//! assert_eq!(stereo, SoundPacket::from_channels(&[&[1, 2], &[1, 2]], 44100));
//! ```

mod optional;
mod resample;
mod resize_channels;
mod volume;

/// Various implemented effects
pub mod effects {
    pub use super::optional::Handle as OptionalHandle;
    pub use super::optional::Optional;
    pub use super::resample::Resample;
    pub use super::resize_channels::ResizeChannels;
    pub use super::volume::Constant as ConstantVolume;
    pub use super::volume::Handle as VolumeHandle;
    pub use super::volume::Volume;
}

/// Useful types for interacting with effects
///
/// Most effects are reexported under the module name [`effect`], ex: [`effect::Optional`]
pub mod prelude {
    pub use super::{Effect, Generic as _};
    pub use crate::data::prelude::*;
    pub use crate::effect::effects as effect;
}

use prelude::*;

/// Get the default effects that should be used on a stream
///
/// These make sure that the packet [`StreamSpec`] matches the `output_spec`
#[must_use]
pub fn default() -> self::Default {
    effect::ResizeChannels.then(effect::Resample::default())
}

/// The concrete type of the default effects returned by [`default`]
///
/// Note: the actual type may change in the future, although it is guaranteed to implement [`Effect`]
pub type Default = List<effect::ResizeChannels, effect::Resample>;

/// An audio effect that modifies an input [`SoundPacket`]
pub trait Effect: Clone + Send + 'static {
    /// Apply the effect onto the `input` packet
    ///
    /// `_output_spec` holds the [`StreamSpec`] of the audio output
    fn apply_to<S: ConvertibleSample>(
        &mut self,
        input: SoundPacket<S>,
        _output_spec: &StreamSpec,
    ) -> SoundPacket<S>;

    /// Reset the effect, if needed
    ///
    /// This is guaranteed to be automatically called after `_output_spec` is changed.
    ///
    /// The default implementation does nothing
    fn reset(&mut self) {}

    /// Create an effect that applies `self` first and then `next` after
    fn then<N: Effect>(self, next: N) -> List<Self, N>
    where
        Self: Sized,
    {
        List {
            current: self,
            next,
        }
    }

    /// Tie `self` to an [`OptionalHandle`](effect::OptionalHandle)
    fn activate_with(self, handle: effect::OptionalHandle) -> effect::Optional<Self>
    where
        Self: Sized,
    {
        effect::Optional::with_handle(self, handle)
    }
}

/// A list of effects applied in order
///
/// Use [`Effect::then`] to create
#[derive(Clone)]
pub struct List<E: Effect, N: Effect> {
    current: E,
    next: N,
}

impl<E: Effect, N: Effect> Effect for List<E, N> {
    #[inline]
    fn apply_to<S: ConvertibleSample>(
        &mut self,
        input: SoundPacket<S>,
        output_spec: &StreamSpec,
    ) -> SoundPacket<S> {
        let packet = self.current.apply_to(input, output_spec);
        self.next.apply_to(packet, output_spec)
    }

    fn reset(&mut self) {
        self.current.reset();
        self.next.reset();
    }
}

impl<E: Effect, N: Effect> List<E, N> {
    /// Mutate the current effect
    pub const fn current(&mut self) -> &mut E {
        &mut self.current
    }

    /// Mutate the next effect
    pub const fn after(&mut self) -> &mut N {
        &mut self.next
    }
}

/// An effect that does nothing
#[derive(Clone)]
pub struct None;

impl Effect for None {
    #[inline]
    fn apply_to<S: ConvertibleSample>(
        &mut self,
        input: SoundPacket<S>,
        _: &StreamSpec,
    ) -> SoundPacket<S> {
        input
    }
}

/// A generic [`Effect`], able to be made into a trait object
pub trait Generic {
    /// A generic version of [`Effect::apply_to`]
    fn apply_to_generic(&mut self, input: GenericPacket, output_spec: &StreamSpec)
        -> GenericPacket;
}

impl<E: Effect> Generic for E {
    fn apply_to_generic(
        &mut self,
        input: GenericPacket,
        output_spec: &StreamSpec,
    ) -> GenericPacket {
        match input {
            GenericPacket::I8(input) => GenericPacket::I8(self.apply_to(input, output_spec)),
            GenericPacket::I16(input) => GenericPacket::I16(self.apply_to(input, output_spec)),
            GenericPacket::I32(input) => GenericPacket::I32(self.apply_to(input, output_spec)),
            GenericPacket::I64(input) => GenericPacket::I64(self.apply_to(input, output_spec)),
            GenericPacket::U8(input) => GenericPacket::U8(self.apply_to(input, output_spec)),
            GenericPacket::U16(input) => GenericPacket::U16(self.apply_to(input, output_spec)),
            GenericPacket::U32(input) => GenericPacket::U32(self.apply_to(input, output_spec)),
            GenericPacket::U64(input) => GenericPacket::U64(self.apply_to(input, output_spec)),
            GenericPacket::F32(input) => GenericPacket::F32(self.apply_to(input, output_spec)),
            GenericPacket::F64(input) => GenericPacket::F64(self.apply_to(input, output_spec)),
        }
    }
}
