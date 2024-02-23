use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender};

use crate::audio::prelude::*;

/// An implementation of [`Audio`] that collects a given about of frames and sends them to a
/// [`CollectorHandle`](Handle)
#[derive(Clone)]
pub struct Collector {
    sender: Sender<GenericPacket>,
    take: usize,
}

impl Collector {
    #[must_use]
    pub fn take(amount: usize) -> (Self, Handle) {
        let (sender, reciever) = crossbeam_channel::bounded(0);
        (
            Self {
                sender,
                take: amount,
            },
            Handle { reciever },
        )
    }
}

/// A handle for a [`Collector`] that can be used to recieve the collected packet through
/// [`Self::collect`]
pub struct Handle {
    reciever: Receiver<GenericPacket>,
}

impl Handle {
    /// Recieve the collected packet, blocking until it's given
    ///
    /// # Panics
    ///
    /// - If the audio sender hangs up before recieving enough frames
    #[must_use]
    pub fn collect(self) -> GenericPacket {
        self.reciever
            .recv()
            .expect("audio hung up without sending a packet")
    }
}

impl Audio for Collector {
    fn start<S: SoundSource>(
        &self,
        options: impl Into<DeviceOptions>,
        source: S,
    ) -> AudioResult<Box<dyn crate::audio::Device>> {
        let info = DeviceInfo::default().apply(&options.into());
        let device = match info.sample_format {
            SampleFormat::I8 => Device::<i8, S>::start_new_boxed(self, info, source),
            SampleFormat::I16 => Device::<i16, S>::start_new_boxed(self, info, source),
            SampleFormat::I32 => Device::<i32, S>::start_new_boxed(self, info, source),
            SampleFormat::I64 => Device::<i64, S>::start_new_boxed(self, info, source),
            SampleFormat::U8 => Device::<u8, S>::start_new_boxed(self, info, source),
            SampleFormat::U16 => Device::<u16, S>::start_new_boxed(self, info, source),
            SampleFormat::U32 => Device::<u32, S>::start_new_boxed(self, info, source),
            SampleFormat::U64 => Device::<u64, S>::start_new_boxed(self, info, source),
            SampleFormat::F32 => Device::<f32, S>::start_new_boxed(self, info, source),
            SampleFormat::F64 => Device::<f64, S>::start_new_boxed(self, info, source),
            _ => todo!(),
        };
        Ok(device)
    }

    fn start_paused<S: SoundSource>(
        &self,
        options: impl Into<DeviceOptions>,
        source: S,
    ) -> AudioResult<Box<dyn crate::audio::Device>> {
        let info = DeviceInfo::default().apply(&options.into());
        let device = match info.sample_format {
            SampleFormat::I8 => Device::<i8, S>::start_paused_boxed(self, info, source),
            SampleFormat::I16 => Device::<i16, S>::start_paused_boxed(self, info, source),
            SampleFormat::I32 => Device::<i32, S>::start_paused_boxed(self, info, source),
            SampleFormat::I64 => Device::<i64, S>::start_paused_boxed(self, info, source),
            SampleFormat::U8 => Device::<u8, S>::start_paused_boxed(self, info, source),
            SampleFormat::U16 => Device::<u16, S>::start_paused_boxed(self, info, source),
            SampleFormat::U32 => Device::<u32, S>::start_paused_boxed(self, info, source),
            SampleFormat::U64 => Device::<u64, S>::start_paused_boxed(self, info, source),
            SampleFormat::F32 => Device::<f32, S>::start_paused_boxed(self, info, source),
            SampleFormat::F64 => Device::<f64, S>::start_paused_boxed(self, info, source),
            _ => todo!(),
        };
        Ok(device)
    }
}

struct Device<S: ConvertibleSample, B: SoundSource> {
    sender: Sender<GenericPacket>,
    info: DeviceInfo,
    source: B,
    current: Option<JoinHandle<()>>,
    sample_type: std::marker::PhantomData<S>,
    take: usize,
}

impl<S: ConvertibleSample, B: SoundSource> Device<S, B> {
    fn new(sink: &Collector, info: DeviceInfo, source: B) -> Self {
        Self {
            sender: sink.sender.clone(),
            info,
            source,
            current: None,
            sample_type: std::marker::PhantomData,
            take: sink.take,
        }
    }

    fn start(&mut self) {
        let source = self.source.build::<S>(self.info);
        let sender = self.sender.clone();
        let Self { info, take, .. } = *self;
        let handle = std::thread::spawn(move || Self::start_blocking(source, &sender, info, take));
        self.current = Some(handle);
    }

    fn start_new_boxed(
        sink: &Collector,
        info: DeviceInfo,
        source: B,
    ) -> Box<dyn crate::audio::Device> {
        let mut new = Self::new(sink, info, source);
        new.start();
        Box::new(new)
    }

    fn start_paused_boxed(
        sink: &Collector,
        info: DeviceInfo,
        source: B,
    ) -> Box<dyn crate::audio::Device> {
        Box::new(Self::new(sink, info, source))
    }

    fn start_blocking(
        mut source: impl Sound<S>,
        sender: &Sender<GenericPacket>,
        info: DeviceInfo,
        take: usize,
    ) {
        // form the samples into a packet
        let mut samples = vec![S::EQUILIBRIUM; info.channels * take];
        for channels in samples.chunks_exact_mut(info.channels) {
            source.next_frame(channels);
        }
        let packet = SoundPacket::from_interleaved(samples, info.into());
        // send it to the handle
        let _ = sender.send(GenericPacket::from(&packet));
    }
}

impl<S: ConvertibleSample, B: SoundSource> crate::audio::Device for Device<S, B> {
    fn info(&self) -> &DeviceInfo {
        &self.info
    }

    fn pause(&mut self) -> AudioResult<()> {
        // pausing doesn't do anything so far
        Ok(())
    }

    fn resume(&mut self) -> AudioResult<()> {
        if self.current.is_none() {
            self.start();
        }
        Ok(())
    }

    fn restart(&mut self) -> AudioResult<()> {
        // stop the other thread
        if let Some(current) = self.current.take() {
            if let Err(err) = current.join() {
                std::panic::resume_unwind(err);
            }
        }
        // start a new one
        self.start();
        Ok(())
    }

    fn inner_modify_options(
        &mut self,
        options: DeviceOptions,
    ) -> AudioResult<Option<Box<dyn crate::audio::Device>>> {
        self.info = self.info.apply(&options);
        Ok(None)
    }
}
