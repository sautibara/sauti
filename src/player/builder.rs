use crate::{
    audio::{Audio, DeviceOptions},
    decoder::Decoder,
    effect::{Effect, List},
};

use super::{Handle, Player};

macro_rules! impl_supplier {
    (prefix: $prefix:path, trait: $trait:ty) => {
        paste::paste! {
            pub trait [<$trait Supplier>] {
                type Out: $prefix::$trait;
                fn give(self) -> Self::Out;
            }

            impl<T: $prefix::$trait> [<$trait Supplier>]for T {
                type Out = Self;
                fn give(self) -> Self::Out {
                    self
                }
            }

            pub struct [<Default $trait>];

            impl [<$trait Supplier>] for [<Default $trait>] {
                type Out = $prefix::Default;
                fn give(self) -> Self::Out {
                    $prefix::default()
                }
            }
        }
    };
}

impl_supplier!(prefix: crate::decoder, trait: Decoder);
impl_supplier!(prefix: crate::effect, trait: Effect);
impl_supplier!(prefix: crate::audio, trait: Audio);

pub struct EffectListSupplier<E: EffectSupplier, N: EffectSupplier> {
    first: E,
    next: N,
}

impl<E: EffectSupplier, N: EffectSupplier> EffectSupplier for EffectListSupplier<E, N> {
    type Out = List<E::Out, N::Out>;
    fn give(self) -> Self::Out {
        self.first.give().then(self.next.give())
    }
}

pub struct Builder<D: DecoderSupplier, E: EffectSupplier, A: AudioSupplier> {
    decoder: D,
    effects: E,
    audio: A,
    options: DeviceOptions,
    volume: f64,
}

impl<D: DecoderSupplier, E: EffectSupplier, A: AudioSupplier> Builder<D, E, A> {
    pub fn run(self) -> Handle {
        let (player, handle) = self.build();
        player.run();
        handle
    }

    #[allow(clippy::type_complexity)] // it's only complex because of the ::Out
    pub fn build(self) -> (Player<D::Out, E::Out, A::Out>, Handle) {
        Player::new(
            self.decoder.give(),
            self.effects.give(),
            self.audio.give(),
            self.options,
            self.volume,
        )
    }

    #[must_use]
    pub fn decoder<N: Decoder>(self, decoder: N) -> Builder<N, E, A> {
        Builder {
            decoder,
            effects: self.effects,
            audio: self.audio,
            options: self.options,
            volume: self.volume,
        }
    }

    #[must_use]
    pub fn effects<N: Effect>(self, effects: N) -> Builder<D, N, A> {
        Builder {
            decoder: self.decoder,
            effects,
            audio: self.audio,
            options: self.options,
            volume: self.volume,
        }
    }

    #[must_use]
    pub fn add_effect<N: Effect>(self, effect: N) -> Builder<D, EffectListSupplier<E, N>, A> {
        Builder {
            decoder: self.decoder,
            effects: EffectListSupplier {
                first: self.effects,
                next: effect,
            },
            audio: self.audio,
            options: self.options,
            volume: self.volume,
        }
    }

    #[must_use]
    pub fn audio<N: Audio>(self, audio: N) -> Builder<D, E, N> {
        Builder {
            effects: self.effects,
            decoder: self.decoder,
            audio,
            options: self.options,
            volume: self.volume,
        }
    }

    #[must_use]
    pub fn options(self, options: DeviceOptions) -> Self {
        Self { options, ..self }
    }

    #[must_use]
    pub fn volume(self, volume: f64) -> Self {
        Self { volume, ..self }
    }
}

impl Default for Builder<DefaultDecoder, DefaultEffect, DefaultAudio> {
    fn default() -> Self {
        Self {
            decoder: DefaultDecoder,
            effects: DefaultEffect,
            audio: DefaultAudio,
            options: DeviceOptions::default(),
            volume: 1.0,
        }
    }
}
