#![allow(clippy::needless_doctest_main)] // meant to show an example
//! Low-level audio handling
//!
//! # Examples
//!
//! ```no_run
//! use sauti::audio::{Audio, ConvertibleSample, DeviceInfo, DeviceOptions, SoundSource};
//!
//! // this program outputs a 440.0 hz sin wave on the main device
//! fn main() {
//!     // start outputting sound on the default device
//!     let audio = sauti::audio::default();
//!     let _device = audio
//!         .start(DeviceOptions::default(), Beep { frequency: 440.0 })
//!         .expect("failed to start outputting sound");
//!
//!     // wait for something in the console, ignore it, and then exit
//!     std::io::stdin()
//!         .read_line(&mut String::new())
//!         .expect("failed to read stdin");
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
//!     ) -> impl FnMut(&mut [S]) + Send + Sync + 'static {
//!         // config from the source can be passed in
//!         let frequency = self.frequency;
//!         // and internal variables can be initialized outside the closure
//!         let mut clock = 0;
//!
//!         // this closure is run for each sample to get the values
//!         // it's given a mutable slice `channels` that holds each channel of the current sample
//!         move |channels| {
//!             clock = (clock + 1) % info.sample_rate;
//!             let val =
//!                 (clock as f64 * frequency * std::f64::consts::TAU / info.sample_rate as f64).sin();
//!             // S::from_sample must be used to convert the f64 value to the generic sample type
//!             channels.fill(S::from_sample(val * 0.1));
//!         }
//!     }
//! }
//! ```

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use dasp_sample::FromSample;
use thiserror::Error;

mod cpal_impl;

/// An enum representing the acceptable sound sample types
pub use cpal::SampleFormat;
/// A basic sound sample
pub use cpal::SizedSample;

/// Find the default [`Audio`] handler
#[must_use]
pub fn default() -> impl Audio {
    cpal_impl::Cpal
}

/// A low-level interface for outputting audio
///
/// Sound is started using the [start](Self::start) method
pub trait Audio {
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
}

/// A currently running stream on a sound device
///
/// When [dropped](std::mem::drop), the stream will be stopped
pub trait Device {
    fn restart(&mut self) -> AudioResult<()>;
    fn play(&mut self) -> AudioResult<()>;
    fn pause(&mut self) -> AudioResult<()>;

    fn info(&self) -> &DeviceInfo;

    /// Add onto the current options with `options` and restart the current device.
    ///
    /// # Errors
    ///
    /// - If the new options don't work, then [`AudioError::StreamConfigNotSupported`] will be
    /// raised
    /// - If some other error occured while [restarting](Self::restart)
    fn modify_options(self: Box<Self>, options: DeviceOptions) -> AudioResult<Box<dyn Device>>;
    /// Add onto the current options with `options` and restart the current device
    ///
    /// If the new options don't work, then the old options will be used instead
    ///
    /// # Errors
    ///
    /// - If the old options don't work anymore
    /// - If some other error occured while [restarting](Self::restart)
    fn try_modify_options(self: Box<Self>, options: DeviceOptions) -> AudioResult<Box<dyn Device>> {
        let info = self.info().clone();
        self.modify_options(options.append_backup(info))
    }
}

/// A source for sound played on a device
///
/// See [`Audio::start`] for how to use this
pub trait SoundSource: 'static {
    /// Creates a producer of samples for the sound
    ///
    /// The producer is run for each sample and given a mutable slice of channels for the current sample.
    ///
    /// # Examples
    ///
    /// ```
    /// # use sauti::audio::*;
    /// # struct Beep { frequency: f64 }
    /// # impl SoundSource for Beep {
    /// fn build<S: ConvertibleSample>(
    ///     &self,
    ///     info: DeviceInfo,
    /// ) -> impl FnMut(&mut [S]) + Send + Sync + 'static {
    ///     // config from the source can be passed in (although no references are allowed)
    ///     let frequency = self.frequency;
    ///     // and internal variables can be initialized outside the closure
    ///     let mut clock = 0;
    ///
    ///     // this closure is run for each sample to get the values
    ///     // it's given a mutable slice `channels` that holds each channel of the current sample
    ///     move |channels| {
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
    fn build<S: ConvertibleSample>(
        &self,
        context: DeviceInfo,
    ) -> impl FnMut(&mut [S]) + Send + Sync + 'static; // thank you 1.75
}

/// Supertrait of [`SizedSample`] and conversions from all others
pub trait ConvertibleSample:
    SizedSample
    + FromSample<i8>
    + FromSample<i16>
    + FromSample<i32>
    + FromSample<i64>
    + FromSample<u8>
    + FromSample<u16>
    + FromSample<u32>
    + FromSample<u64>
    + FromSample<f32>
    + FromSample<f64>
    + 'static
{
}

impl<
        T: SizedSample
            + FromSample<i8>
            + FromSample<i16>
            + FromSample<i32>
            + FromSample<i64>
            + FromSample<u8>
            + FromSample<u16>
            + FromSample<u32>
            + FromSample<u64>
            + FromSample<f32>
            + FromSample<f64>
            + 'static,
    > ConvertibleSample for T
{
}

/// Information about the current sound device
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub sample_rate: u32,
    pub sample_format: SampleFormat,
    pub channels: u16,
}

impl DeviceInfo {
    pub fn with_sample_rate(self, sample_rate: u32) -> Self {
        Self {
            sample_rate,
            ..self
        }
    }

    pub fn with_sample_format(self, sample_format: SampleFormat) -> Self {
        Self {
            sample_format,
            ..self
        }
    }

    pub fn with_channel_count(self, channels: u16) -> Self {
        Self { channels, ..self }
    }

    pub fn apply(self, options: DeviceOptions) -> Self {
        Self {
            sample_rate: options.sample_rate.unwrap_or(self.sample_rate),
            sample_format: options.sample_format.unwrap_or(self.sample_format),
            channels: options.channels.unwrap_or(self.channels),
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
/// - To use the default if none work, then call [`Self::with_default_as_backup`]
#[derive(Default, Debug, Clone)]
// TODO: builder pattern
pub struct DeviceOptions {
    pub sample_rate: Option<u32>,
    pub sample_format: Option<SampleFormat>,
    pub channels: Option<u16>,
    // yes this is a linked list
    pub backup: Option<Box<Self>>,
}

impl DeviceOptions {
    pub fn is_empty(&self) -> bool {
        self.sample_rate.is_none() && self.sample_format.is_none() && self.channels.is_none()
    }

    pub fn merge(self, other: Self) -> Self {
        Self {
            sample_rate: other.sample_rate.or(self.sample_rate),
            sample_format: other.sample_format.or(self.sample_format),
            channels: other.channels.or(self.channels),
            // append the other options' backups to this
            backup: match (self.backup, other.backup) {
                (Some(own), Some(other)) => Some(Box::new(own.append_backup(*other))),
                (Some(own), None) => Some(own),
                (None, Some(other)) => Some(other),
                (None, None) => None,
            },
        }
    }

    pub fn with_sample_rate(self, rate: u32) -> Self {
        Self {
            sample_rate: Some(rate),
            ..self
        }
    }

    pub fn with_sample_format(self, format: SampleFormat) -> Self {
        Self {
            sample_format: Some(format),
            ..self
        }
    }

    pub fn with_channel_count(self, channels: u16) -> Self {
        Self {
            channels: Some(channels),
            ..self
        }
    }

    pub fn with_backup(self, backup: impl Into<Self>) -> Self {
        Self {
            backup: Some(Box::new(backup.into())),
            ..self
        }
    }

    pub fn with_default_as_backup(self) -> Self {
        self.with_backup(Self::default())
    }

    pub fn append_backup(self, new: impl Into<Self>) -> Self {
        let new = new.into();
        let backup = match self.backup {
            Some(backup) => backup.append_backup(new),
            None => new,
        };

        Self {
            backup: Some(Box::new(backup)),
            ..self
        }
    }

    pub fn with_default_sample_rate(self) -> Self {
        Self {
            sample_rate: None,
            ..self
        }
    }

    pub fn with_default_sample_format(self) -> Self {
        Self {
            sample_format: None,
            ..self
        }
    }

    pub fn with_default_channel_count(self) -> Self {
        Self {
            channels: None,
            ..self
        }
    }

    pub fn remove_backup(self) -> Self {
        Self {
            backup: None,
            ..self
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

/// Some errors that can be encountered while interacting with audio
#[derive(Error, Debug)]
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

type AudioResult<T> = Result<T, AudioError>;
