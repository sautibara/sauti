//! A way to create a [`Player`] step-by-step
//!
//! # Examples
//!
//! ```
//! use sauti::player::prelude::*;
//! use std::time::Duration;
//!
//! // An example of the many different options that can be changed
//! // See the example for [`crate::player::on_end`] for a similar example
//!
//! struct Exit;
//! // A channel that exits the program when a message is sent
//! let (sender, reciever) = crossbeam_channel::bounded(0);
//! // Create a [`Player`] and start it running in another thread
//! let handle = Player::builder()
//!     // Set the player to start at half volume
//!     .volume(0.5)
//!     // Set the decoder and audio to be empty
//!     // Each ignores what it consumes
//!     .decoder(sauti::test::Empty)
//!     .audio(sauti::test::Empty)
//!     // Set the player to send an [`Exit`] signal when the file ends
//!     // Since the decoder only returns None, the file will immediately end,
//!     // so the Exit value should be sent right after a file is played
//!     .on_file_end_run(move |_| { sender.send(Exit).unwrap(); Ok(()) })
//!     // Modify the audio device options to set a sample rate of 48000
//!     // This won't actually do anything, though
//!     .options(DeviceOptions::default().with_sample_rate(48000))
//!     // Start running the player in another thread
//!     .run();
//!
//! // Start decoding (and providing the data)
//! // [`Empty`] ignores the [`MediaSource`], so just send an empty path
//! handle.play("").expect("player disconnected");
//!
//! // When decoding a file, [`Empty`] returns None, so the file will end immediately.
//! let res = reciever.recv_timeout(Duration::from_secs(1));
//! // If the player takes too long, it'll return Err
//! assert!(res.is_ok(), "empty player should end immediately");
//! ```
use crate::{
    audio::{Audio, DeviceOptions},
    decoder::Decoder,
    effect::{Effect, List},
};

use super::{on_file_end::OnFileEnd, prelude::*, Handle};

macro_rules! impl_supplier {
    (prefix: $prefix:path, trait: $trait:ty, a: $a:ident) => {
        paste::paste! {
            #[doc = "A supplier for " $a " [`" $trait "`]"]
            #[doc = ""]
            #[doc = "[`" $trait "`] automatically implements this, so " $a " " $trait " itself could be used as a supplier"]
            #[doc = ""]
            #[doc = "This trait is used in the [`Builder`] instead of just taking in " $a " " $trait " so that [`Default" $trait "`] can be created lazily."]
            pub trait [<$trait Supplier>] {
                type Out: $prefix::$trait;
                fn give(self) -> Self::Out;
            }

            impl<T: $prefix::$trait> [<$trait Supplier>] for T {
                type Out = Self;
                fn give(self) -> Self::Out {
                    self
                }
            }

            #[doc = $a:camel " [`" $trait "Supplier`] that supplies the default value"]
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

impl_supplier!(prefix: crate::decoder, trait: Decoder, a: a);
impl_supplier!(prefix: crate::effect, trait: Effect, a: an);
impl_supplier!(prefix: crate::audio, trait: Audio, a: an);

/// An [`EffectSupplier`] that supplies a [`List`] of [`Effect`]s
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

/// A builder for a [`Player`]
///
/// The builder starts out with default options and can be changed to what is needed.
///
/// - Call [`Self::run`] to build the player, run it in a separate thread, and return the handle
/// - Call [`Self::build`] to build the player and give both it and its handle back, without
/// running it
///
/// This takes in [suppliers](DecoderSupplier) that lazily provide a way to obtain each component.
/// These are eventually consumed when the builder is [run](Self::run) or [built](Self::build).
pub struct Builder<D: DecoderSupplier, E: EffectSupplier, A: AudioSupplier, O: OnFileEnd> {
    decoder: D,
    effects: E,
    audio: A,
    options: DeviceOptions,
    volume: f64,
    on_file_end: O,
}

impl<D: DecoderSupplier, E: EffectSupplier, A: AudioSupplier, O: OnFileEnd> Builder<D, E, A, O> {
    /// [Build](Self::build) the player and [run](Player::run) it in a separate thread, returning
    /// its handle.
    pub fn run(self) -> Handle {
        let (player, handle) = self.build();
        player.run();
        handle
    }

    /// Finish creating the player with the given options
    #[allow(clippy::type_complexity)] // it's only complex because of the ::Out
    pub fn build(self) -> (Player<D::Out, E::Out, A::Out, O>, Handle) {
        Player::new(
            self.decoder.give(),
            self.effects.give(),
            self.audio.give(),
            self.on_file_end,
            self.options,
            self.volume,
        )
    }

    /// Replace the [`Decoder`] used to decode audio files to audio packets
    #[must_use]
    pub fn decoder<N: Decoder>(self, decoder: N) -> Builder<N, E, A, O> {
        Builder {
            decoder,
            effects: self.effects,
            audio: self.audio,
            options: self.options,
            volume: self.volume,
            on_file_end: self.on_file_end,
        }
    }

    /// Replace the [`Effect`] used on the decoded packets
    ///
    /// It's often reccomended to use [`Self::add_effect`], as the [default effects](crate::effect::default)
    /// are required to match the packets' [sample rate](effect::Resample) and [channel count](effect::ResizeChannels)
    /// from the decoder to the output stream.
    #[must_use]
    pub fn effects<N: Effect>(self, effects: N) -> Builder<D, N, A, O> {
        Builder {
            decoder: self.decoder,
            effects,
            audio: self.audio,
            options: self.options,
            volume: self.volume,
            on_file_end: self.on_file_end,
        }
    }

    /// Append an [`Effect`] to the current effect stored in the builder
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
            on_file_end: self.on_file_end,
        }
    }

    /// Replace the [`Audio`] used to output audio to the system
    #[must_use]
    pub fn audio<N: Audio>(self, audio: N) -> Builder<D, E, N, O> {
        Builder {
            effects: self.effects,
            decoder: self.decoder,
            audio,
            options: self.options,
            volume: self.volume,
            on_file_end: self.on_file_end,
        }
    }

    /// Set the [`Player`] to run `on_end` after each song ends.
    #[must_use]
    pub fn on_file_end<N: OnFileEnd>(self, on_file_end: N) -> Builder<D, E, A, N> {
        Builder {
            effects: self.effects,
            decoder: self.decoder,
            audio: self.audio,
            options: self.options,
            volume: self.volume,
            on_file_end,
        }
    }

    /// Set the [`Player`] to run `func` after each song ends.
    ///
    /// This is a more specific version of [`Self::on_file_end`] to aid the compiler with determining
    /// types
    #[must_use]
    pub fn on_file_end_run<F>(self, func: F) -> Builder<D, E, A, F>
    where
        F: Fn(&mut BoxedPlayer) -> PlayerResult<()> + Send + 'static,
    {
        self.on_file_end(func)
    }

    /// Replace the [`DeviceOptions`] used for the output stream
    #[must_use]
    pub fn options(self, options: DeviceOptions) -> Self {
        Self { options, ..self }
    }

    /// Set the initial volume for the player
    #[must_use]
    pub fn volume(self, volume: f64) -> Self {
        Self { volume, ..self }
    }
}

impl Default for Builder<DefaultDecoder, DefaultEffect, DefaultAudio, on_file_end::Default> {
    fn default() -> Self {
        Self {
            decoder: DefaultDecoder,
            effects: DefaultEffect,
            audio: DefaultAudio,
            options: DeviceOptions::default(),
            volume: 1.0,
            on_file_end: on_file_end::default(),
        }
    }
}
