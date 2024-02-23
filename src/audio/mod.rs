#![allow(clippy::needless_doctest_main)] // the SoundSource impl is too large
//! Low-level audio handling
//!
//! To start playing audio, use an [`Audio`] player to create a [`Device`] using a [`SoundSource`] and
//! some [`DeviceOptions`]. The [`SoundSource`] is repeatedly called to get every frame of audio
//! for the device. The device can then be [paused](Device::pause), which pauses the thread, or
//! [resumed](Device::resume). Device options can also be changed on the fly using
//! [`DeviceExt::merge_options`].
//!
//! # Examples
//!
//! ```
//! use sauti::audio::prelude::*;
//! use sauti::test::prelude::*;
//!
//! fn main() {
//!     let (collector, handle) = Collector::take(4);
//!     let _device = collector
//!         .start(DeviceOptions::default().as_mono(), Beep { frequency: 11025.0 })
//!         .expect("failed to start outputting sound");
//!         
//!     let packet = handle.collect().convert::<i8>();
//!
//!     assert_eq!(packet, SoundPacket::from_channels(&[&[127, 0, -128, 0]], 44100));
//! }
//!
//! struct Beep {
//!     frequency: f64,
//! }
//!
//! impl SoundSource for Beep {
//!     // the sound source is generic over the sample type
//!     fn build<S: ConvertibleSample>(
//!         &self,
//!         info: DeviceInfo,
//!     ) -> impl Sound<S> {
//!         // config from the source can be passed in
//!         let frequency = self.frequency;
//!         // and internal variables can be initialized outside the closure
//!         let mut clock = 0;
//!
//!         // this closure is run for each sample to get the values
//!         // it's given a mutable slice `channels` that holds each channel of the current sample
//!         move |channels: &mut [S]| {
//!             clock = (clock + 1) % info.sample_rate;
//!             let val =
//!                 (clock as f64 * frequency * std::f64::consts::TAU / info.sample_rate as f64).sin();
//!             // S::from_sample must be used to convert the f64 value to the generic sample type
//!             channels.fill(S::from_sample(val));
//!         }
//!     }
//! }
//! ```

use std::ops::Deref;

use thiserror::Error;

mod cpal_impl;

/// Useful types for interacting with audio
pub mod prelude {
    pub use super::{
        Audio, AudioError, AudioResult, Device, DeviceExt, DeviceInfo, DeviceOptions, Sound,
        SoundSource,
    };
    pub use crate::data::*;
}

use crate::data::{ConvertibleSample, SampleFormat};

/// Find the default [`Audio`] handler
#[must_use]
pub const fn default() -> Default {
    cpal_impl::Cpal
}

pub type Default = cpal_impl::Cpal;

/// A low-level interface for outputting audio
///
/// Sound is started using the [start](Self::start) method
pub trait Audio: Clone + Send + 'static {
    /// Create a new [`Device`] and start it running using a [`SoundSource`]
    ///
    /// # Errors
    ///
    /// - If there are no output devices available
    /// - If the default output device isn't available
    /// - If the default output device doesn't support `options`
    /// - Other backend specific errors
    // TODO: allow devices other than the default device (probably in DeviceOptions)
    fn start<S: SoundSource>(
        &self,
        options: impl Into<DeviceOptions>,
        source: S,
    ) -> AudioResult<Box<dyn Device>>;
    /// Create a new [`Device`] and start it paused using a [`SoundSource`]
    ///
    /// # Errors
    ///
    /// - If there are no output devices available
    /// - If the default output device isn't available
    /// - If the default output device doesn't support `options`
    /// - If there is an error while [pausing](Device::pause)
    /// - Other backend specific errors
    fn start_paused<S: SoundSource>(
        &self,
        options: impl Into<DeviceOptions>,
        source: S,
    ) -> AudioResult<Box<dyn Device>> {
        let mut device = self.start(options, source)?;
        device.pause()?;
        Ok(device)
    }
}

/// A currently running stream on a sound device
///
/// When [dropped](std::mem::drop), the stream will be stopped.
///
/// Check [`DeviceExt`] for more methods
pub trait Device {
    /// # Errors
    ///
    /// - If the device has been invalidated due to a call of [`Self::inner_modify_options`]
    /// - If the device is not available anymore
    /// - Other backend-specific errors
    fn restart(&mut self) -> AudioResult<()>;
    /// # Errors
    ///
    /// - If the device is not available anymore
    /// - Other backend-specific errors
    fn resume(&mut self) -> AudioResult<()>;
    /// # Errors
    ///
    /// - If the device is not available anymore
    /// - Other backend-specific errors
    fn pause(&mut self) -> AudioResult<()>;

    fn info(&self) -> &DeviceInfo;

    /// Calling this could invalidate the device, see [`DeviceExt::merge_options`] instead
    ///
    /// Tries to modify the options in this device. If the device's options can't be changed without
    /// creating a new device, then a new device is created and returned. `merge_options` handles
    /// changing out the device if necessary (and dropping the old one), which is why it's suggested.
    ///
    /// # Errors
    ///
    /// - If the device has already been invalidated due to a previous call
    /// - Any other errors in [`Self::restart`]
    #[deprecated = "Calling this could invalidate the device if not careful, see DeviceExt::merge_options instead"]
    fn inner_modify_options(
        &mut self,
        options: DeviceOptions,
    ) -> AudioResult<Option<Box<dyn Device>>>;
}

/// Methods specific to a [`Device`] trait object ([`Box<dyn Device>`])
pub trait DeviceExt: Deref<Target = dyn Device> {
    /// Add onto this device's current options with `options` and then restart.
    ///
    /// # Errors
    ///
    /// - If the new options don't work, then [`AudioError::DeviceOptionsNotSupported`] will be
    /// raised
    /// - Other errors can occur while [restarting](Device::restart)
    fn merge_options(&mut self, options: DeviceOptions) -> AudioResult<()>;
    /// Add onto this device's current options with `options` and then restart.
    ///
    /// If the new options don't work, then the old options will be used instead
    ///
    /// # Errors
    ///
    /// - If the old options don't work anymore
    /// - Other errors can occur while [restarting](Device::restart)
    fn try_merge_options(&mut self, options: DeviceOptions) -> AudioResult<()> {
        self.merge_options(options.with_backup(*self.info()))
    }
}

impl DeviceExt for Box<dyn Device> {
    fn merge_options(&mut self, options: DeviceOptions) -> AudioResult<()> {
        #[allow(deprecated)] // the deprecation is just used to disuade using it elsewhere
        if let Some(new_device) = self.inner_modify_options(options)? {
            *self = new_device;
        }
        Ok(())
    }
}

/// A reusable source for a [`Sound`] played on a [`Device`]
///
/// See [`Audio::start`] for how to use this
pub trait SoundSource: 'static {
    /// Creates the [`Sound`] which will be used to write each frame
    ///
    /// # Examples
    ///
    /// ```
    /// # use sauti::audio::prelude::*;
    /// # struct Beep { frequency: f64 }
    /// # impl SoundSource for Beep {
    /// fn build<S: ConvertibleSample>(
    ///     &self,
    ///     info: DeviceInfo,
    /// ) -> impl Sound<S> {
    ///     // config from the source can be passed in (although no references are allowed)
    ///     let frequency = self.frequency;
    ///     // and internal variables can be initialized outside the closure
    ///     let mut clock = 0;
    ///
    ///     // this closure is run for each sample to get the values
    ///     // it's given a mutable slice `channels` that holds each channel of the current sample
    ///     move |channels: &mut [S]| {
    ///         clock = (clock + 1) % info.sample_rate;
    ///         let val =
    ///             (clock as f64 * frequency * std::f64::consts::TAU / info.sample_rate as f64).sin();
    ///         // S::from_sample must be used to convert the computed f64 value to the generic sample type
    ///         // see [[SampleFormat]] for all of the accepted sample formats
    ///         channels.fill(S::from_sample(val * 0.1));
    ///     }
    /// }
    /// # }
    /// ```
    fn build<S: ConvertibleSample>(&self, context: DeviceInfo) -> impl Sound<S>; // thank you 1.75
}

/// A currently playing sound on a [`Device`], usually created using a [`SoundSource`]
///
/// For each frame of audio, [`Self::next_frame`] will be called to populate it
///
/// [`FnMut(&mut [S])`](FnMut) notably implements this trait
///
/// # Examples
///
/// ```
/// use sauti::audio::prelude::*;
///
/// struct Sine {
///     frequency: f64,
///     clock: usize,
///     sample_rate: usize,
/// }
///
/// impl<S: ConvertibleSample> Sound<S> for Sine {
///     fn next_frame(&mut self, channels: &mut [S]) {
///         let val = self.clock as f64 * self.frequency * std::f64::consts::TAU / self.sample_rate as f64;
///         let sin = val.sin();
///         // S::from_sample must be used to convert the computed f64 value to the generic sample type
///         // see [[SampleFormat]] for all of the accepted sample formats
///         channels.fill(S::from_sample(sin));
///         // increment the clock for the next value (state persists)
///         self.clock = (self.clock + 1) % self.sample_rate;
///     }
/// }
///
/// let mut channels = vec![0.0; 1];
/// let mut wave = Sine {
///     frequency: 12000.0,
///     clock: 0,
///     sample_rate: 48000,
/// };
///
/// wave.next_frame(&mut channels[..]);
/// assert_eq!(channels, &[0.0]);
/// wave.next_frame(&mut channels[..]);
/// assert_eq!(channels, &[1.0]);
/// ```
pub trait Sound<S: ConvertibleSample>: Send + 'static {
    /// Populate the next frame of audio
    ///
    /// `channels` holds each channel, in order
    fn next_frame(&mut self, channels: &mut [S]);
}

impl<S: ConvertibleSample, T: FnMut(&mut [S]) + Send + 'static> Sound<S> for T {
    fn next_frame(&mut self, channels: &mut [S]) {
        self(channels);
    }
}

impl<S: ConvertibleSample> Sound<S> for Silence {
    fn next_frame(&mut self, channels: &mut [S]) {
        channels.fill(S::EQUILIBRIUM);
    }
}

/// Source that only outputs silence
pub struct Silence;

impl SoundSource for Silence {
    fn build<S: ConvertibleSample>(&self, _: DeviceInfo) -> impl Sound<S> {
        |channels: &mut [S]| channels.fill(S::EQUILIBRIUM)
    }
}

/// Information about the current sound device
#[derive(Debug, Clone, Copy)]
pub struct DeviceInfo {
    pub sample_rate: usize,
    pub sample_format: SampleFormat,
    pub channels: usize,
}

impl DeviceInfo {
    #[must_use]
    pub const fn with_sample_rate(self, sample_rate: usize) -> Self {
        Self {
            sample_rate,
            ..self
        }
    }

    #[must_use]
    pub const fn with_sample_format(self, sample_format: SampleFormat) -> Self {
        Self {
            sample_format,
            ..self
        }
    }

    #[must_use]
    pub const fn with_channel_count(self, channels: usize) -> Self {
        Self { channels, ..self }
    }

    #[must_use]
    pub fn apply(self, options: &DeviceOptions) -> Self {
        Self {
            sample_rate: options.sample_rate.unwrap_or(self.sample_rate),
            sample_format: options.sample_format.unwrap_or(self.sample_format),
            channels: options.channels.unwrap_or(self.channels),
        }
    }
}

impl std::default::Default for DeviceInfo {
    fn default() -> Self {
        Self {
            sample_format: SampleFormat::F32,
            sample_rate: 44100,
            channels: 2,
        }
    }
}

/// Desired options for a sound device
///
/// If an option is not given, then the default config will be used for it. This includes the
/// device itself: it will always use the default output device.
///
/// # Backups
///
/// - If this option doesn't work, then backups will be tried one by one until one works.
/// - If none work, then [`AudioError::DeviceOptionsNotSupported`] will be raised.
/// - To use the default options if no others work, then call [`Self::with_default_as_backup`]
#[derive(Default, Debug, Clone)]
#[must_use]
pub struct DeviceOptions {
    pub sample_rate: Option<usize>,
    pub sample_format: Option<SampleFormat>,
    pub channels: Option<usize>,
    // yes this is a linked list
    pub backup: Option<Box<Self>>,
}

/// Implements a with_<field> method for a builder
macro_rules! with {
    // Default version ($field is set to None)
    ( func_name: $func_name:ident, field: $field:ident, default: true ) => {
        pub fn $func_name(self) -> Self {
            Self {
                $field: None,
                ..self
            }
        }
    };
    // Normal version ($field is set to Some($field))
    ( func_name: $func_name:ident, field: $field:ident, typ: $typ:ty ) => {
        pub fn $func_name(self, $field: $typ) -> Self {
            Self {
                $field: Some($field),
                ..self
            }
        }
    };
}

impl DeviceOptions {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.sample_rate.is_none() && self.sample_format.is_none() && self.channels.is_none()
    }

    /// Merge `other`'s options onto `self`
    pub fn merge(self, other: Self) -> Self {
        Self {
            sample_rate: other.sample_rate.or(self.sample_rate),
            sample_format: other.sample_format.or(self.sample_format),
            channels: other.channels.or(self.channels),
            // append the other options' backups to this
            backup: match (self.backup, other.backup) {
                (Some(own), Some(other)) => Some(Box::new(own.with_backup(*other))),
                (Some(own), None) => Some(own),
                (None, Some(other)) => Some(other),
                (None, None) => None,
            },
        }
    }

    /// Merge `other`'s options onto `self`, ignoring backups
    pub fn simple_merge(&self, other: &Self) -> Self {
        Self {
            sample_rate: other.sample_rate.or(self.sample_rate),
            sample_format: other.sample_format.or(self.sample_format),
            channels: other.channels.or(self.channels),
            backup: None,
        }
    }

    with!( func_name: with_sample_rate, field: sample_rate, typ: usize );
    with!( func_name: with_sample_format, field: sample_format, typ: SampleFormat );
    with!( func_name: with_channel_count, field: channels, typ: usize );
    with!( func_name: with_default_sample_rate, field: sample_rate, default: true );
    with!( func_name: with_default_sample_format, field: sample_format, default: true );
    with!( func_name: with_default_channel_count, field: channels, default: true );

    pub fn as_stereo(self) -> Self {
        self.with_channel_count(2)
    }

    pub fn as_mono(self) -> Self {
        self.with_channel_count(1)
    }

    pub fn with_default_as_backup(self) -> Self {
        self.with_backup(Self::default())
    }

    /// Append backup to the backup list
    pub fn with_backup(self, backup: impl Into<Self>) -> Self {
        let backup = backup.into();
        let new_backup = match self.backup {
            Some(current) => current.with_backup(backup),
            None => backup,
        };

        Self {
            backup: Some(Box::new(new_backup)),
            ..self
        }
    }

    with!( func_name: without_backup, field: backup, default: true );

    /// Obtain an iterator over each potential option, including this one
    #[must_use]
    pub const fn iter(&self) -> DeviceOptionIter<'_> {
        DeviceOptionIter {
            current: Some(self),
        }
    }

    /// Get the number of potential options, including backups
    #[must_use]
    pub fn variants(&self) -> usize {
        1 + self.backup.as_ref().map_or(0, |backup| backup.variants())
    }
}

impl<'a> IntoIterator for &'a DeviceOptions {
    type Item = &'a DeviceOptions;
    type IntoIter = DeviceOptionIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        DeviceOptionIter {
            current: Some(self),
        }
    }
}

impl From<DeviceInfo> for DeviceOptions {
    fn from(info: DeviceInfo) -> Self {
        Self {
            sample_rate: Some(info.sample_rate),
            sample_format: Some(info.sample_format),
            channels: Some(info.channels),
            backup: None,
        }
    }
}

/// An iterator over the priority list of [`DeviceOptions`], including each's backups
pub struct DeviceOptionIter<'a> {
    current: Option<&'a DeviceOptions>,
}

impl<'a> Iterator for DeviceOptionIter<'a> {
    type Item = &'a DeviceOptions;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current;
        // stabilize Option::inspect soon plsss
        if let Some(current) = current {
            let after = current.backup.as_deref();
            self.current = after;
        }
        current
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let size = (self.current.as_ref()).map_or(0, |current| current.variants());
        (size, Some(size))
    }
}

/// Some errors that can be encountered while interacting with audio
#[derive(Error, Debug)]
// there's a good few errors from different modules, so adding names here makes sense
#[allow(clippy::module_name_repetitions)]
pub enum AudioError {
    #[error("no devices found")]
    NoDevicesFound,
    #[error("device '{0}' no longer exists")]
    DeviceNotAvailable(String),
    #[error("device '{0}' doesn't support output")]
    DeviceNoOutput(String),
    #[error("device '{device}' does not support options: {options:?}")]
    DeviceOptionsNotSupported {
        options: DeviceOptions,
        device: String,
    },
    #[error("backend error: {0}")]
    BackendError(String),
    #[error("backend error found while using device '{device}': '{error}'")]
    DeviceBackendError { device: String, error: String },
    #[error("unrecognized sample format: {0}")]
    UnrecognizedSampleFormat(SampleFormat),
}

// see [`crate::audio::AudioError`] for justification
#[allow(clippy::module_name_repetitions)]
pub type AudioResult<T> = Result<T, AudioError>;
