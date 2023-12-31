use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use thiserror::Error;

mod cpal_impl;

pub fn default_audio() -> impl Audio {
    cpal_impl::Cpal
}

pub trait Audio {
    type Device<F: FnMut(&mut [f32], &SampleContext) + Send + Sync + 'static, B: FnMut() -> F>: Device<F, B>;

    /// Create a new [[Device]] and start it running using a handler
    ///
    /// The handler is obtained using `handler_builder`, which will be rerun any time that the
    /// device options are changed. This allows the handler to be mutable without breaking when
    /// it's made again.
    ///
    /// The handler itself takes in a mutable slice of every channel in order.
    fn start<F: FnMut(&mut [f32], &SampleContext) + Send + Sync + 'static, B: FnMut() -> F>(
        &self,
        options: DeviceOptions,
        handler_builder: B,
    ) -> AudioResult<Self::Device<F, B>>;
}

pub trait Device<F: FnMut(&mut [f32], &SampleContext) + Send + Sync + 'static, B: FnMut() -> F> {
    fn restart(&mut self) -> AudioResult<()>;

    fn play(&mut self) -> AudioResult<()> {
        Ok(())
    }

    fn pause(&mut self) -> AudioResult<()> {
        Ok(())
    }
}

/// Some context given when asking for samples
pub struct SampleContext {
    /// The current sample rate of the stream
    pub sample_rate: u32,
}

#[derive(Debug)]
pub enum SampleFormat {
    I8,
    I16,
    I24,
    I32,
    I48,
    I64,
    U8,
    U16,
    U24,
    U32,
    U48,
    U64,
    F32,
    F64,
}

#[derive(Default, Debug)]
// TODO: builder
pub struct DeviceOptions {
    pub sample_rate: Option<u32>,
    pub sample_format: Option<SampleFormat>,
    pub channels: Option<u32>,
}

impl DeviceOptions {
    pub fn is_empty(&self) -> bool {
        self.sample_rate.is_none() && self.sample_format.is_none() && self.channels.is_none()
    }
}

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
}

type AudioResult<T> = Result<T, AudioError>;
