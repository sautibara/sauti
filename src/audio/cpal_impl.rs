use super::*;

pub struct Cpal;

impl Cpal {
    fn find_device(host: cpal::Host) -> AudioResult<cpal::Device> {
        host.default_output_device()
            .ok_or(AudioError::NoDevicesFound)
    }

    fn find_config(
        device: &cpal::Device,
        options: DeviceOptions,
    ) -> AudioResult<cpal::StreamConfig> {
        if options.is_empty() {
            device
                .default_output_config()
                .map_err(|err| default_stream_config_error(err, device))
                .map(|config| config.config())
        } else {
            todo!("cpal stream config")
        }
    }

    fn create_stream(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        mut handler: impl FnMut(&mut [f32], &SampleContext) + Send + Sync + 'static,
    ) -> AudioResult<cpal::Stream> {
        let data_config = config.clone();
        let sample_context = SampleContext {
            sample_rate: config.sample_rate.0,
        };
        device
            .build_output_stream(
                config,
                // FIXME: currently this doesn't respect what the stream actually expects
                move |data, _| {
                    Self::data_callback(data, &mut handler, &data_config, &sample_context)
                },
                |err| eprintln!("{err:?}"),
                None,
            )
            .map_err(|err| build_stream_error(err, device))
    }

    fn data_callback(
        data: &mut [f32],
        handler: &mut (impl FnMut(&mut [f32], &SampleContext) + Send + Sync + 'static),
        config: &cpal::StreamConfig,
        sample_context: &SampleContext,
    ) {
        for sample in data.chunks_mut(config.channels as usize) {
            handler(sample, sample_context);
        }
    }
}

impl Audio for Cpal {
    type Device<F: FnMut(&mut [f32], &SampleContext) + Send + Sync + 'static, B: FnMut() -> F> =
        CpalDevice<F, B>;

    fn start<F: FnMut(&mut [f32], &SampleContext) + Send + Sync + 'static, B: FnMut() -> F>(
        &self,
        options: DeviceOptions,
        mut handler_builder: B,
    ) -> AudioResult<Self::Device<F, B>> {
        let host = cpal::default_host();
        let device = Cpal::find_device(host)?;
        let config = Cpal::find_config(&device, options)?;

        let stream = Cpal::create_stream(&device, &config, handler_builder())?;

        stream
            .play()
            .map_err(|err| play_stream_error(err, &device))?;

        Ok(CpalDevice {
            handler_builder,
            stream,
            device,
            config,
        })
    }
}

pub struct CpalDevice<F: FnMut(&mut [f32], &SampleContext) + Send + Sync + 'static, B: FnMut() -> F>
{
    handler_builder: B,
    stream: cpal::Stream,
    device: cpal::Device,
    config: cpal::StreamConfig,
}

impl<F: FnMut(&mut [f32], &SampleContext) + Send + Sync + 'static, B: FnMut() -> F> Device<F, B>
    for CpalDevice<F, B>
{
    fn restart(&mut self) -> AudioResult<()> {
        let stream = Cpal::create_stream(&self.device, &self.config, (self.handler_builder)())?;
        self.stream = stream; // old stream drops and disconnects

        // start the stream again
        self.play()
    }

    fn play(&mut self) -> AudioResult<()> {
        self.stream
            .play()
            .map_err(|err| play_stream_error(err, &self.device))
    }

    fn pause(&mut self) -> AudioResult<()> {
        self.stream
            .pause()
            .map_err(|err| pause_stream_error(err, &self.device))
    }
}

// error mappings //

fn default_stream_config_error(
    err: cpal::DefaultStreamConfigError,
    device: &cpal::Device,
) -> AudioError {
    let name = match device.name() {
        Ok(name) => name,
        Err(err) => return err.into(),
    };

    match err {
        cpal::DefaultStreamConfigError::DeviceNotAvailable => AudioError::DeviceNotAvailable(name),
        // This happens when a device doesn't support the stream type (input or output)
        // requested. Since only outputs are requested, it can be mapped to output
        cpal::DefaultStreamConfigError::StreamTypeNotSupported => AudioError::DeviceNoOutput(name),
        cpal::DefaultStreamConfigError::BackendSpecific { err } => AudioError::DeviceBackendError {
            device: name,
            error: err.description,
        },
    }
}

fn build_stream_error(err: cpal::BuildStreamError, device: &cpal::Device) -> AudioError {
    let name = match device.name() {
        Ok(name) => name,
        Err(err) => return err.into(),
    };

    match err {
        cpal::BuildStreamError::DeviceNotAvailable => AudioError::DeviceNotAvailable(name),
        cpal::BuildStreamError::StreamConfigNotSupported => unreachable!("must be caught earlier"),
        cpal::BuildStreamError::InvalidArgument => AudioError::BackendError("cpal passed an invalid argument somewhere (see cpal::BuildStreamError::InvalidArgument)".to_string()),
        cpal::BuildStreamError::StreamIdOverflow => AudioError::BackendError("cpal - too many stream ids, overflow (see cpal::BuildStreamError::StreamIdOverflow)".to_string()),
        cpal::BuildStreamError::BackendSpecific { err } => AudioError::DeviceBackendError { device: name, error: err.description },
    }
}

impl From<cpal::DeviceNameError> for AudioError {
    fn from(value: cpal::DeviceNameError) -> Self {
        let cpal::DeviceNameError::BackendSpecific { err } = value;
        AudioError::BackendError(err.description)
    }
}

fn play_stream_error(err: cpal::PlayStreamError, device: &cpal::Device) -> AudioError {
    let name = match device.name() {
        Ok(name) => name,
        Err(err) => return err.into(),
    };

    match err {
        cpal::PlayStreamError::DeviceNotAvailable => AudioError::DeviceNotAvailable(name),
        cpal::PlayStreamError::BackendSpecific { err } => AudioError::DeviceBackendError {
            device: name,
            error: err.description,
        },
    }
}

fn pause_stream_error(err: cpal::PauseStreamError, device: &cpal::Device) -> AudioError {
    let name = match device.name() {
        Ok(name) => name,
        Err(err) => return err.into(),
    };

    match err {
        cpal::PauseStreamError::DeviceNotAvailable => AudioError::DeviceNotAvailable(name),
        cpal::PauseStreamError::BackendSpecific { err } => AudioError::DeviceBackendError {
            device: name,
            error: err.description,
        },
    }
}
