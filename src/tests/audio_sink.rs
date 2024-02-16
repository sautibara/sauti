use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender};

use crate::audio::prelude::*;

#[derive(Clone)]
pub struct Sink {
    sender: Sender<GenericPacket>,
    take: usize,
}

impl Sink {
    const DEFAULT_INFO: DeviceInfo = DeviceInfo {
        channels: 2,
        sample_format: SampleFormat::F32,
        sample_rate: 44100,
    };

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

pub struct Handle {
    reciever: Receiver<GenericPacket>,
}

impl Handle {
    /// # Panics
    ///
    /// - If the audio sender hangs up before recieving enough frames
    #[must_use]
    pub fn collect(&self) -> GenericPacket {
        self.reciever
            .recv()
            .expect("audio hung up without sending a packet")
    }
}

impl Audio for Sink {
    fn start<B: SoundSource>(
        &self,
        options: impl Into<DeviceOptions>,
        source: B,
    ) -> AudioResult<Box<dyn Device>> {
        let info = Self::DEFAULT_INFO.apply(&options.into());
        let device = match info.sample_format {
            SampleFormat::I8 => SinkDevice::<i8, B>::start_new_boxed(self, info, source),
            SampleFormat::I16 => SinkDevice::<i16, B>::start_new_boxed(self, info, source),
            SampleFormat::I32 => SinkDevice::<i32, B>::start_new_boxed(self, info, source),
            SampleFormat::I64 => SinkDevice::<i64, B>::start_new_boxed(self, info, source),
            SampleFormat::U8 => SinkDevice::<u8, B>::start_new_boxed(self, info, source),
            SampleFormat::U16 => SinkDevice::<u16, B>::start_new_boxed(self, info, source),
            SampleFormat::U32 => SinkDevice::<u32, B>::start_new_boxed(self, info, source),
            SampleFormat::U64 => SinkDevice::<u64, B>::start_new_boxed(self, info, source),
            SampleFormat::F32 => SinkDevice::<f32, B>::start_new_boxed(self, info, source),
            SampleFormat::F64 => SinkDevice::<f64, B>::start_new_boxed(self, info, source),
            _ => todo!(),
        };
        Ok(device)
    }
}

struct SinkDevice<S: ConvertibleSample, B: SoundSource> {
    sender: Sender<GenericPacket>,
    info: DeviceInfo,
    source: B,
    current: Option<JoinHandle<()>>,
    sample_type: std::marker::PhantomData<S>,
    take: usize,
}

impl<S: ConvertibleSample, B: SoundSource> SinkDevice<S, B> {
    fn new(sink: &Sink, info: DeviceInfo, source: B) -> Self {
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

    fn start_new_boxed(sink: &Sink, info: DeviceInfo, source: B) -> Box<dyn Device> {
        let mut new = Self::new(sink, info, source);
        new.start();
        Box::new(new)
    }

    fn start_blocking(
        mut source: impl FnMut(&mut [S]) + Send + 'static,
        sender: &Sender<GenericPacket>,
        info: DeviceInfo,
        take: usize,
    ) {
        // form the samples into a packet
        let mut samples = vec![S::EQUILIBRIUM; info.channels * take];
        for channels in samples.chunks_exact_mut(info.channels) {
            source(channels);
        }
        let packet = SoundPacket::from_interleaved(samples, info.into());
        // send it to the handle
        let _ = sender.send(GenericPacket::from(packet));
    }
}

impl<S: ConvertibleSample, B: SoundSource> Device for SinkDevice<S, B> {
    fn info(&self) -> &DeviceInfo {
        &self.info
    }

    fn pause(&mut self) -> AudioResult<()> {
        unimplemented!("pausing or playing would end up doing nothing")
    }

    fn resume(&mut self) -> AudioResult<()> {
        unimplemented!("pausing or playing would end up doing nothing")
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
    ) -> AudioResult<Option<Box<dyn Device>>> {
        self.info = self.info.apply(&options);
        Ok(None)
    }
}
