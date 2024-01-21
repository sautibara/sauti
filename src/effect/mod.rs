use crate::{
    audio::ConvertibleSample,
    file::{GenericPacket, SoundPacket, StreamSpec},
};

mod channels;
mod optional;
pub use channels::*;
pub use optional::*;

use dasp_sample::Sample;

pub trait Effect: Send + Sync + 'static {
    fn apply<S: ConvertibleSample>(
        &mut self,
        input: SoundPacket<S>,
        output_spec: &StreamSpec,
    ) -> SoundPacket<S>;

    fn then<N: Effect>(self, next: N) -> List<Self, N>
    where
        Self: Sized,
    {
        List {
            current: self,
            next,
        }
    }

    fn activate_with_handle(self, handle: OptionalHandle) -> Optional<Self>
    where
        Self: Sized,
    {
        Optional::with_handle(self, handle)
    }
}

pub struct List<E: Effect, N: Effect> {
    current: E,
    next: N,
}

impl<E: Effect, N: Effect> Effect for List<E, N> {
    #[inline(always)]
    fn apply<S: ConvertibleSample>(
        &mut self,
        input: SoundPacket<S>,
        output_spec: &StreamSpec,
    ) -> SoundPacket<S> {
        let packet = self.current.apply(input, output_spec);
        self.next.apply(packet, output_spec)
    }
}

pub trait EffectGeneric {
    fn apply_generic(&mut self, input: GenericPacket, output_spec: &StreamSpec) -> GenericPacket;
}

impl<E: Effect> EffectGeneric for E {
    fn apply_generic(&mut self, input: GenericPacket, output_spec: &StreamSpec) -> GenericPacket {
        match input {
            GenericPacket::I8(input) => GenericPacket::I8(self.apply(input, output_spec)),
            GenericPacket::I16(input) => GenericPacket::I16(self.apply(input, output_spec)),
            GenericPacket::I32(input) => GenericPacket::I32(self.apply(input, output_spec)),
            GenericPacket::I64(input) => GenericPacket::I64(self.apply(input, output_spec)),
            GenericPacket::U8(input) => GenericPacket::U8(self.apply(input, output_spec)),
            GenericPacket::U16(input) => GenericPacket::U16(self.apply(input, output_spec)),
            GenericPacket::U32(input) => GenericPacket::U32(self.apply(input, output_spec)),
            GenericPacket::U64(input) => GenericPacket::U64(self.apply(input, output_spec)),
            GenericPacket::F32(input) => GenericPacket::F32(self.apply(input, output_spec)),
            GenericPacket::F64(input) => GenericPacket::F64(self.apply(input, output_spec)),
        }
    }
}

pub struct Volume(pub f32);

impl Effect for Volume {
    fn apply<S: ConvertibleSample>(
        &mut self,
        input: SoundPacket<S>,
        _: &StreamSpec,
    ) -> SoundPacket<S> {
        let mul = S::from_sample(self.0).to_float_sample();
        input.map_samples(|sample| (sample.to_float_sample() * mul).to_sample())
    }
}
