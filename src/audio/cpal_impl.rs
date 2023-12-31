use super::{
    Audio, AudioError, AudioResult, ConvertibleSample, Device, DeviceInfo, DeviceOptions,
    DeviceTrait, HostTrait, SampleFormat, SizedSample, SoundSource, StreamTrait,
};

pub struct Cpal;

impl Cpal {
    fn find_device(host: &cpal::Host) -> AudioResult<cpal::Device> {
        host.default_output_device()
            .ok_or(AudioError::NoDevicesFound)
    }

    fn find_config(
        device: &cpal::Device,
        options: &DeviceOptions,
    ) -> AudioResult<cpal::SupportedStreamConfig> {
        if options.is_empty() {
            device
                .default_output_config()
                .map_err(|err| default_stream_config_error(err, device))
        } else {
            todo!("cpal stream config")
        }
    }

    fn create_stream<S: ConvertibleSample, B: SoundSource>(
        device: &cpal::Device,
        config: &cpal::SupportedStreamConfig,
        source: &B,
    ) -> AudioResult<cpal::Stream> {
        let device_info = DeviceInfo {
            sample_rate: config.sample_rate().0,
            sample_format: config.sample_format(),
            channels: config.channels(),
        };
        let concrete_config = config.config();
        let channels = concrete_config.channels;

        // build the sound handler
        let mut handler = source.build::<S>(device_info);

        // build the stream and pass in the handler
        device
            .build_output_stream(
                &concrete_config,
                move |data, _| Self::data_callback(data, &mut handler, channels),
                // TODO: find some other way to notify of errors
                |err| eprintln!("{err:?}"),
                None,
            )
            .map_err(|err| build_stream_error(err, device))
    }

    fn data_callback<S: SizedSample>(
        data: &mut [S],
        handler: &mut (impl FnMut(&mut [S]) + Send + Sync + 'static),
        channels: u16,
    ) {
        for sample in data.chunks_mut(channels as usize) {
            handler(sample);
        }
    }
}

impl Audio for Cpal {
    fn start<B: SoundSource>(
        &self,
        options: DeviceOptions,
        source: B,
    ) -> AudioResult<Box<dyn Device>> {
        let host = cpal::default_host();
        let device = Self::find_device(&host)?;
        let config = Self::find_config(&device, &options)?;

        match config.sample_format() {
            SampleFormat::I8 => CpalDevice::<i8, B>::new_boxed(device, config, source),
            SampleFormat::I16 => CpalDevice::<i16, B>::new_boxed(device, config, source),
            SampleFormat::I32 => CpalDevice::<i32, B>::new_boxed(device, config, source),
            SampleFormat::I64 => CpalDevice::<i64, B>::new_boxed(device, config, source),
            SampleFormat::U8 => CpalDevice::<u8, B>::new_boxed(device, config, source),
            SampleFormat::U16 => CpalDevice::<u16, B>::new_boxed(device, config, source),
            SampleFormat::U32 => CpalDevice::<u32, B>::new_boxed(device, config, source),
            SampleFormat::U64 => CpalDevice::<u64, B>::new_boxed(device, config, source),
            SampleFormat::F32 => CpalDevice::<f32, B>::new_boxed(device, config, source),
            SampleFormat::F64 => CpalDevice::<f64, B>::new_boxed(device, config, source),
            format => Err(AudioError::UnrecognizedSampleFormat(format)),
        }
    }
}

pub struct CpalDevice<S: ConvertibleSample, B: SoundSource> {
    source: B,
    stream: cpal::Stream,
    device: cpal::Device,
    config: cpal::SupportedStreamConfig,
    sample_marker: std::marker::PhantomData<S>,
}

impl<S: ConvertibleSample, B: SoundSource> CpalDevice<S, B> {
    fn new(
        device: cpal::Device,
        config: cpal::SupportedStreamConfig,
        source: B,
    ) -> AudioResult<Self> {
        let stream = Cpal::create_stream::<S, B>(&device, &config, &source)?;

        // the stream sometimes starts off paused, so resume it
        stream
            .play()
            .map_err(|err| play_stream_error(err, &device))?;

        Ok(Self {
            source,
            stream,
            device,
            config,
            sample_marker: std::marker::PhantomData,
        })
    }

    fn new_boxed(
        device: cpal::Device,
        config: cpal::SupportedStreamConfig,
        source: B,
    ) -> AudioResult<Box<dyn Device>> {
        // map doesn't work for some reason
        Ok(Box::new(Self::new(device, config, source)?))
    }
}

impl<S: ConvertibleSample, B: SoundSource> Device for CpalDevice<S, B> {
    fn restart(&mut self) -> AudioResult<()> {
        let stream = Cpal::create_stream::<S, B>(&self.device, &self.config, &self.source)?;
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

    fn info(&self) -> DeviceInfo {
        DeviceInfo {
            sample_rate: self.config.sample_rate().0,
            sample_format: self.config.sample_format(),
            channels: self.config.channels(),
        }
    }

    fn change_sample_rate(&mut self, new: u32) -> AudioResult<()> {
        self.config = cpal::SupportedStreamConfig::new(
            self.config.channels(),
            cpal::SampleRate(new),
            self.config.buffer_size().clone(),
            self.config.sample_format(),
        );

        self.restart()
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
        Self::BackendError(err.description)
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
