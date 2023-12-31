use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use dasp_sample::FromSample;
use thiserror::Error;

mod cpal_impl;

/// An enum representing the acceptable sound sample types
pub use cpal::SampleFormat;
/// A basic sound sample
pub use cpal::SizedSample;

/// Find the default [`Audio`] handler
pub fn default_audio() -> impl Audio {
    cpal_impl::Cpal
}

/// A low-level interface for outputting audio
///
/// Sound is started using the [start](Self::start) method
pub trait Audio {
    /// Create a new [`Device`] and start it running using a [`SoundSource`]
    fn start<S: SoundSource>(
        &self,
        options: DeviceOptions,
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
}

/// A source for sound played on a device
///
/// See [`Audio::start`] for how to use this
pub trait SoundSource: 'static {
    /// Creates a producer of samples for the sound
    ///
    /// The producer is run for each sample and given a mutable slice of channels for the current sample.
    ///
    /// # Example
    ///
    /// ```
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
pub struct DeviceInfo {
    pub sample_rate: u32,
    pub sample_format: SampleFormat,
    pub channels: u16,
}

/// Desired options for a sound device
///
/// If an option is not given, then the default config will be used for it
#[derive(Default, Debug)]
// TODO: builder pattern
pub struct DeviceOptions {
    pub sample_rate: Option<u32>,
    pub sample_format: Option<SampleFormat>,
    pub channels: Option<u16>,
}

impl DeviceOptions {
    pub fn is_empty(&self) -> bool {
        self.sample_rate.is_none() && self.sample_format.is_none() && self.channels.is_none()
    }
}

/// Some errors that can be encountered while interacting with audio
#[derive(Error, Debug)]
pub enum AudioError {
    #[error("no devices found")]
    NoDevicesFound,
    #[error("device {0} no longer exists")]
    DeviceNotAvailable(String),
    #[error("device {0} doesn't support output")]
    DeviceNoOutput(String),
    #[error("device {device} does not support config: {config:?}")]
    StreamConfigNotSupported {
        config: DeviceOptions,
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
