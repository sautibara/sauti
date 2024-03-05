use crate::{
    audio::{Audio, DeviceOptions},
    decoder::Decoder,
    effect::{Effect, List},
};

use super::{on_end::OnFileEnd, prelude::*, Handle};

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

pub struct Builder<D: DecoderSupplier, E: EffectSupplier, A: AudioSupplier, O: OnFileEnd> {
    decoder: D,
    effects: E,
    audio: A,
    options: DeviceOptions,
    volume: f64,
    on_end: O,
}

impl<D: DecoderSupplier, E: EffectSupplier, A: AudioSupplier, O: OnFileEnd> Builder<D, E, A, O> {
    pub fn run(self) -> Handle {
        let (player, handle) = self.build();
        player.run();
        handle
    }

    #[allow(clippy::type_complexity)] // it's only complex because of the ::Out
    pub fn build(self) -> (Player<D::Out, E::Out, A::Out, O>, Handle) {
        Player::new(
            self.decoder.give(),
            self.effects.give(),
            self.audio.give(),
            self.on_end,
            self.options,
            self.volume,
        )
    }

    #[must_use]
    pub fn decoder<N: Decoder>(self, decoder: N) -> Builder<N, E, A, O> {
        Builder {
            decoder,
            effects: self.effects,
            audio: self.audio,
            options: self.options,
            volume: self.volume,
            on_end: self.on_end,
        }
    }

    #[must_use]
    pub fn effects<N: Effect>(self, effects: N) -> Builder<D, N, A, O> {
        Builder {
            decoder: self.decoder,
            effects,
            audio: self.audio,
            options: self.options,
            volume: self.volume,
            on_end: self.on_end,
        }
    }

    #[must_use]
    pub fn add_effect<N: Effect>(self, effect: N) -> Builder<D, EffectListSupplier<E, N>, A, O> {
        Builder {
            decoder: self.decoder,
            effects: EffectListSupplier {
                first: self.effects,
                next: effect,
            },
            audio: self.audio,
            options: self.options,
            volume: self.volume,
            on_end: self.on_end,
        }
    }

    #[must_use]
    pub fn audio<N: Audio>(self, audio: N) -> Builder<D, E, N, O> {
        Builder {
            effects: self.effects,
            decoder: self.decoder,
            audio,
            options: self.options,
            volume: self.volume,
            on_end: self.on_end,
        }
    }

    #[must_use]
    pub fn on_end<N: OnFileEnd>(self, on_end: N) -> Builder<D, E, A, N> {
        Builder {
            effects: self.effects,
            decoder: self.decoder,
            audio: self.audio,
            options: self.options,
            volume: self.volume,
            on_end,
        }
    }

    /// Set the [`Player`] to run `func` after each song ends.
    ///
    /// This is a more specific version of [`Self::on_end`] to aid the compiler with determining
    /// types
    #[must_use]
    pub fn on_end_run<F>(self, func: F) -> Builder<D, E, A, F>
    where
        F: Fn(&mut BoxedPlayer) -> PlayerResult<()> + Send + 'static,
    {
        self.on_end(func)
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

impl Default for Builder<DefaultDecoder, DefaultEffect, DefaultAudio, on_end::Default> {
    fn default() -> Self {
        Self {
            decoder: DefaultDecoder,
            effects: DefaultEffect,
            audio: DefaultAudio,
            options: DeviceOptions::default(),
            volume: 1.0,
            on_end: on_end::default(),
        }
    }
}
