//! Various utilities for writing tests
//!
//! # Examples
//!
//! ```
//! use sauti::player::prelude::*;
//! use sauti::test::prelude::*;
//!
//! // Create a sound packet with two frames and two channels
//! let packet = SoundPacket::from_channels(&[[1, 2], [3, 4]], 44100);
//! let generic = GenericPacket::from(packet);
//! // Use [`Collector`] to take the first two frames
//! let (collector, handle) = Collector::take(2);
//! // Use [`Provider`] to provide the packet to the collector
//! let provider = Provider::provide(generic.clone());
//!
//! // Create a [`Player`] using the [`Collector`] and [`Provider`]
//! let player = Player::default_builder()
//!     .decoder(provider)
//!     .audio(collector)
//!     // The SampleFormat has to be I32 because that's what's used above
//!     .options(DeviceOptions::default().with_sample_format(SampleFormat::I32))
//!     .run();
//! // Start decoding (and providing the data)
//! // [`Provider`] ignores the [`MediaSource`], so just send an empty path
//! player.play("").expect("failed to start sending data");
//!
//! let given_packet = handle.collect();
//! assert_eq!(given_packet, generic);
//! ```
mod collector;
mod provider;

pub use collector::Collector;
pub use collector::Handle as CollectorHandle;
pub use provider::Provider;

pub mod prelude {
    pub use super::Collector;
    pub use super::Provider;
}
