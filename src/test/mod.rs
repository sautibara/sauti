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
//! let provider = Provider::repeat(generic.clone());
//!
//! // Create a [`Player`] using the [`Collector`] and [`Provider`]
//! let player = Player::builder()
//!     .decoder(provider)
//!     .output(collector).start_playing()
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
use crate::data::prelude::*;

#[cfg(feature = "output")]
mod collector;
#[cfg(any(
    feature = "output",
    feature = "decoder",
    feature = "effect",
    feature = "player"
))]
mod empty;
#[cfg(feature = "decoder")]
mod provider;

#[cfg(feature = "output")]
pub use collector::Collector;
#[cfg(feature = "output")]
pub use collector::Handle as CollectorHandle;
#[cfg(any(
    feature = "output",
    feature = "decoder",
    feature = "effect",
    feature = "player"
))]
pub use empty::Empty;
#[cfg(feature = "decoder")]
pub use provider::Provider;

#[cfg(any(
    feature = "output",
    feature = "decoder",
    feature = "effect",
    feature = "player"
))]
pub mod prelude {
    #[cfg(feature = "output")]
    pub use super::Collector;
    #[cfg(any(
        feature = "output",
        feature = "decoder",
        feature = "effect",
        feature = "player"
    ))]
    pub use super::Empty;
    #[cfg(feature = "decoder")]
    pub use super::Provider;
}

/// A test file consisting of a 22050hz square wave
///
/// The square wave switches every sample
#[must_use]
pub fn file() -> MediaSource {
    MediaSource::copy_buf(include_bytes!("test_file.flac"))
}
