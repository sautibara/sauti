use cpal::{SampleRate, SupportedStreamConfig, SupportedStreamConfigRange};

use super::{
    Audio, AudioError, AudioResult, ConvertibleSample, Device, DeviceInfo, DeviceOptions,
    DeviceTrait, HostTrait, SampleFormat, SizedSample, SoundSource, StreamTrait,
};

// options stuff //

fn is_none_or<T>(opt: Option<T>, predicate: impl FnOnce(T) -> bool) -> bool {
    opt.is_none() || opt.is_some_and(predicate)
}

fn is_none_or_eq<T: PartialEq<T>>(opt: Option<T>, val: T) -> bool {
    opt.is_none() || opt.is_some_and(|opt| opt == val)
}

fn sample_rate_within_range(rate: usize, range: &SupportedStreamConfigRange) -> bool {
    let rate = rate as u32;
    rate >= range.min_sample_rate().0 && rate <= range.max_sample_rate().0
}

fn options_supports(options: &DeviceOptions, config: &SupportedStreamConfigRange) -> bool {
    is_none_or_eq(options.sample_format, config.sample_format())
        && is_none_or_eq(options.channels, config.channels() as usize)
        && is_none_or(options.sample_rate, |rate| {
            sample_rate_within_range(rate, config)
        })
}

fn apply_options(
    options: &DeviceOptions,
    config: &cpal::SupportedStreamConfig,
) -> cpal::SupportedStreamConfig {
    SupportedStreamConfig::new(
        options
            .channels
            .map(|x| x as u16)
            .unwrap_or_else(|| config.channels()),
        options
            .sample_rate
            .map(|rate| SampleRate(rate as u32))
            .unwrap_or_else(|| config.sample_rate()),
        config.buffer_size().clone(),
        options
            .sample_format
            .unwrap_or_else(|| config.sample_format()),
    )
}

fn with_best_sample_rate(
    range: SupportedStreamConfigRange,
    options: &DeviceOptions,
) -> Option<SupportedStreamConfig> {
    let rate = if let Some(rate) = options.sample_rate {
        // if the sample rate doesn't match, then return None
        sample_rate_within_range(rate, &range).then_some(rate as u32)?
    } else if sample_rate_within_range(44100, &range) {
        44100
    } else {
        range.max_sample_rate().0
    };

    Some(range.with_sample_rate(SampleRate(rate)))
}

struct SupportedConfig {
    ranges: Vec<SupportedStreamConfigRange>,
}

impl SupportedConfig {
    fn from_device(device: &cpal::Device) -> AudioResult<Self> {
        Ok(Self {
            ranges: device
                .supported_output_configs()
                .map_err(|err| supported_configs_error(err, device))?
                .collect(),
        })
    }

    fn supports_config(&self, config: &SupportedStreamConfig) -> bool {
        self.ranges.iter().any(|option| {
            option.channels() == config.channels()
                && option.sample_format() == config.sample_format()
                && sample_rate_within_range(config.sample_rate().0 as usize, option)
        })
    }

    fn supports_options(&self, options: &DeviceOptions) -> bool {
        (self.ranges.iter()).any(|config| options_supports(options, config))
    }

    fn best_with_options(self, options: &DeviceOptions) -> Option<SupportedStreamConfig> {
        self.ranges
            .into_iter()
            .filter(|config| options_supports(options, config))
            .max_by(|a, b| a.cmp_default_heuristics(b))
            .and_then(|range| with_best_sample_rate(range, options))
    }

    fn first_supported(
        &self,
        options: impl IntoIterator<Item = DeviceOptions>,
    ) -> Option<DeviceOptions> {
        options
            .into_iter()
            .filter(|options| self.supports_options(options))
            .nth(0)
    }
}

// actual cpal stuff //

pub struct Cpal;

impl Cpal {
    fn find_device(host: &cpal::Host) -> AudioResult<cpal::Device> {
        host.default_output_device()
            .ok_or(AudioError::NoDevicesFound)
    }

    fn default_config(device: &cpal::Device) -> AudioResult<cpal::SupportedStreamConfig> {
        device
            .default_output_config()
            .map_err(|err| default_stream_config_error(err, device))
    }

    fn find_config(
        device: &cpal::Device,
        options: &DeviceOptions,
    ) -> AudioResult<cpal::SupportedStreamConfig> {
        // get the default config for the options to reference
        let default = Self::default_config(device)?;

        // find the first working config or error
        let val = (options.iter())
            .map(|options| Self::find_config_single(device, options, &default))
            // take only results that are Err or Some
            .filter_map(|config| config.transpose())
            .nth(0);

        // if there are no options, then return an error
        let Some(val) = val else {
            return Err(AudioError::DeviceOptionsNotSupported {
                options: options.clone(),
                device: device.name()?,
            });
        };

        // this can also return an error if found
        val
    }

    fn find_config_single(
        device: &cpal::Device,
        options: &DeviceOptions,
        default: &cpal::SupportedStreamConfig,
    ) -> AudioResult<Option<cpal::SupportedStreamConfig>> {
        // if there are no options, then just use the default
        if options.is_empty() {
            return Ok(Some(default.clone()));
        }

        // find the supported config to check against
        let supported_config = SupportedConfig::from_device(device)?;

        // check the given options + the device's default options
        let default_with_options = apply_options(options, default);
        if supported_config.supports_config(&default_with_options) {
            return Ok(Some(default_with_options));
        }

        // check only the given options + any others
        let best = supported_config.best_with_options(options);

        Ok(best)
    }

    fn create_stream<S: ConvertibleSample, B: SoundSource>(
        device: &cpal::Device,
        config: &cpal::SupportedStreamConfig,
        source: &B,
    ) -> AudioResult<cpal::Stream> {
        let device_info = DeviceInfo {
            sample_rate: config.sample_rate().0 as usize,
            sample_format: config.sample_format(),
            channels: config.channels() as usize,
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
        options: impl Into<DeviceOptions>,
        source: B,
    ) -> AudioResult<Box<dyn Device>> {
        let options = options.into();
        let host = cpal::default_host();
        let device = Self::find_device(&host)?;
        let config = Self::find_config(&device, &options)?;

        match config.sample_format() {
            SampleFormat::I8 => CpalDevice::<i8, B>::new_boxed(device, options, config, source),
            SampleFormat::I16 => CpalDevice::<i16, B>::new_boxed(device, options, config, source),
            SampleFormat::I32 => CpalDevice::<i32, B>::new_boxed(device, options, config, source),
            SampleFormat::I64 => CpalDevice::<i64, B>::new_boxed(device, options, config, source),
            SampleFormat::U8 => CpalDevice::<u8, B>::new_boxed(device, options, config, source),
            SampleFormat::U16 => CpalDevice::<u16, B>::new_boxed(device, options, config, source),
            SampleFormat::U32 => CpalDevice::<u32, B>::new_boxed(device, options, config, source),
            SampleFormat::U64 => CpalDevice::<u64, B>::new_boxed(device, options, config, source),
            SampleFormat::F32 => CpalDevice::<f32, B>::new_boxed(device, options, config, source),
            SampleFormat::F64 => CpalDevice::<f64, B>::new_boxed(device, options, config, source),
            format => Err(AudioError::UnrecognizedSampleFormat(format)),
        }
    }
}

pub struct CpalDevice<S: ConvertibleSample, B: SoundSource> {
    /// The original options that were used to make this device
    dna: Option<DeviceDna<B>>,
    stream: cpal::Stream,
    device: cpal::Device,
    /// Information about the current stream
    device_info: DeviceInfo,
    /// The buffer size allowed for the current stream
    buffer_size: cpal::SupportedBufferSize,
    // a marker to the sample this device is using
    // cpal expects this sample type when creating the stream
    sample_marker: std::marker::PhantomData<S>,
}

/// Original options that are used to make a device
pub struct DeviceDna<B: SoundSource> {
    /// The original sound source of the device
    source: B,
    /// The original device options used to create this device
    device_options: DeviceOptions,
}

impl<S: ConvertibleSample, B: SoundSource> CpalDevice<S, B> {
    fn new(
        device: cpal::Device,
        device_options: DeviceOptions,
        config: cpal::SupportedStreamConfig,
        source: B,
    ) -> AudioResult<Self> {
        let stream = Cpal::create_stream::<S, B>(&device, &config, &source)?;

        // the stream sometimes starts off paused, so resume it
        stream
            .play()
            .map_err(|err| play_stream_error(err, &device))?;

        Ok(Self {
            dna: Some(DeviceDna {
                source,
                device_options,
            }),
            stream,
            device,
            buffer_size: config.buffer_size().clone(),
            device_info: config.into(),
            sample_marker: std::marker::PhantomData,
        })
    }

    fn new_boxed(
        device: cpal::Device,
        device_options: DeviceOptions,
        config: cpal::SupportedStreamConfig,
        source: B,
    ) -> AudioResult<Box<dyn Device>> {
        // map doesn't work for some reason
        Ok(Box::new(Self::new(device, device_options, config, source)?))
    }

    fn stream_config(&self) -> SupportedStreamConfig {
        SupportedStreamConfig::new(
            self.device_info.channels as u16,
            cpal::SampleRate(self.device_info.sample_rate as u32),
            self.buffer_size.clone(),
            self.device_info.sample_format,
        )
    }
}

const DNA_ERROR: &str =
    "device skeletons that are modified and used to create a new device should not be able to be used to create another device";

impl<S: ConvertibleSample, B: SoundSource> Device for CpalDevice<S, B> {
    fn restart(&mut self) -> AudioResult<()> {
        let dna = self.dna.as_ref().expect(DNA_ERROR);
        let stream = Cpal::create_stream::<S, B>(&self.device, &self.stream_config(), &dna.source)?;
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

    fn info(&self) -> &DeviceInfo {
        &self.device_info
    }

    fn inner_modify_options(
        &mut self,
        options: DeviceOptions,
    ) -> AudioResult<Option<Box<dyn Device>>> {
        let mut dna = self.dna.take().expect(DNA_ERROR);
        let supported_config = SupportedConfig::from_device(&self.device)?;

        // find first supported device options
        let new_options = supported_config.first_supported(
            options
                .iter()
                .map(|option| dna.device_options.simple_merge(option)),
        );

        // if there are none, then return an error
        let Some(new_options) = new_options else {
            return Err(AudioError::DeviceOptionsNotSupported {
                options: dna.device_options.merge(options),
                device: self.device.name()?,
            });
        };

        // if the options change the sample format, then
        // a new device has to be made because of generics
        if new_options.sample_format.is_some() {
            return Cpal.start(new_options, dna.source).map(Some);
        }

        // otherwise, edit the current device

        // update device info
        if let Some(sample_rate) = new_options.sample_rate {
            self.device_info.sample_rate = sample_rate;
        }
        if let Some(channels) = new_options.channels {
            self.device_info.channels = channels;
        }

        // put back the dna
        dna.device_options = new_options;
        self.dna = Some(dna);

        // restart the device
        self.restart()?;
        // a new device did not need to be made
        Ok(None)
    }
}

impl From<SupportedStreamConfig> for DeviceInfo {
    fn from(value: SupportedStreamConfig) -> Self {
        Self {
            sample_rate: value.sample_rate().0 as usize,
            sample_format: value.sample_format(),
            channels: value.channels() as usize,
        }
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

fn supported_configs_error(
    err: cpal::SupportedStreamConfigsError,
    device: &cpal::Device,
) -> AudioError {
    let name = match device.name() {
        Ok(name) => name,
        Err(err) => return err.into(),
    };

    match err {
        cpal::SupportedStreamConfigsError::DeviceNotAvailable => AudioError::DeviceNotAvailable(name),
        cpal::SupportedStreamConfigsError::InvalidArgument => AudioError::BackendError("cpal passed an invalid argument somewhere (see cpal::BuildStreamError::InvalidArgument)".to_string()),
        cpal::SupportedStreamConfigsError::BackendSpecific { err } => AudioError::DeviceBackendError { device: name, error: err.description },
    }
}
