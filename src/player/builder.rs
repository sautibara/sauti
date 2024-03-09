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
//!     // Set the decoder and output to be empty
//!     // Each ignores what it consumes
//!     .decoder(sauti::test::Empty)
//!     .output(sauti::test::Empty)
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
    decoder::Decoder,
    effect::{Effect, List},
    output::{DeviceOptions, Output},
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
impl_supplier!(prefix: crate::output, trait: Output, a: an);

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
pub struct Builder<O: OutputSupplier, D: DecoderSupplier, E: EffectSupplier, C: OnFileEnd> {
    output: O,
    decoder: D,
    effects: E,
    on_file_end: C,
    options: DeviceOptions,
    volume: f64,
}

impl<D: DecoderSupplier, E: EffectSupplier, O: OutputSupplier, C: OnFileEnd> Builder<O, D, E, C> {
    /// [Build](Self::build) the player and [run](Player::run) it in a separate thread, returning
    /// its handle.
    pub fn run(self) -> Handle {
        let (player, handle) = self.build();
        player.run();
        handle
    }

    /// Finish creating the player with the given options
    #[allow(clippy::type_complexity)] // it's only complex because of the ::Out
    pub fn build(self) -> (Player<O::Out, D::Out, E::Out, C>, Handle) {
        Player::new(
            self.output.give(),
            self.decoder.give(),
            self.effects.give(),
            self.on_file_end,
            self.options,
            self.volume,
        )
    }

    /// Replace the [`Decoder`] used to decode audio files to audio packets
    #[must_use]
    pub fn decoder<N: Decoder>(self, decoder: N) -> Builder<O, N, E, C> {
        Builder {
            decoder,
            effects: self.effects,
            output: self.output,
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
    pub fn effects<N: Effect>(self, effects: N) -> Builder<O, D, N, C> {
        Builder {
            decoder: self.decoder,
            effects,
            output: self.output,
            options: self.options,
            volume: self.volume,
            on_file_end: self.on_file_end,
        }
    }

    /// Append an [`Effect`] to the current effect stored in the builder
    #[must_use]
    pub fn add_effect<N: Effect>(self, effect: N) -> Builder<O, D, EffectListSupplier<E, N>, C> {
        Builder {
            decoder: self.decoder,
            effects: EffectListSupplier {
                first: self.effects,
                next: effect,
            },
            output: self.output,
            options: self.options,
            volume: self.volume,
            on_file_end: self.on_file_end,
        }
    }

    /// Replace the [`Output`] used to output audio to the system
    #[must_use]
    pub fn output<N: Output>(self, output: N) -> Builder<N, D, E, C> {
        Builder {
            effects: self.effects,
            decoder: self.decoder,
            output,
            options: self.options,
            volume: self.volume,
            on_file_end: self.on_file_end,
        }
    }

    /// Set the [`Player`] to run `on_end` after each song ends.
    #[must_use]
    pub fn on_file_end<N: OnFileEnd>(self, on_file_end: N) -> Builder<O, D, E, N> {
        Builder {
            effects: self.effects,
            decoder: self.decoder,
            output: self.output,
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
    pub fn on_file_end_run<F>(self, func: F) -> Builder<O, D, E, F>
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

impl Default for Builder<DefaultOutput, DefaultDecoder, DefaultEffect, on_file_end::Default> {
    fn default() -> Self {
        Self {
            output: DefaultOutput,
            decoder: DefaultDecoder,
            effects: DefaultEffect,
            options: DeviceOptions::default(),
            volume: 1.0,
            on_file_end: on_file_end::default(),
        }
    }
}
