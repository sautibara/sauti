use std::path::Path;

use sauti::{
    audio::{
        prelude::{ConvertibleSample, DeviceInfo},
        Audio, DeviceOptions, SoundSource,
    },
    effect::{Effect, EffectGeneric, OptionalHandle, ResizeChannels, Volume},
    file::{Decoder, GenericPacket, SoundPacket, StreamSpec},
};

use crossbeam_channel::Receiver;

// NOTE: this is a proof of concept - not meant to be an example
fn main() -> sauti::file::FileResult<()> {
    // decode the file given in the command line
    let path = std::env::args().nth(1).expect("usage: {command} {path}");
    let decoder = sauti::file::default_decoder();
    let mut stream = decoder.read(Path::new(&path))?;

    // set up a stream between the decoder and the audio output
    let (sender, reciever) = crossbeam_channel::unbounded();
    let volume_handle = OptionalHandle::new(true);
    let source_volume_handle = volume_handle.clone();
    let source = AudioStreamSource::new(reciever, move || {
        ResizeChannels.then(Volume(0.1).activate_with_handle(source_volume_handle.clone()))
    });

    // the audio output needs to start with a packet
    if let Some(packet) = stream.next_packet()? {
        sender.send(packet).unwrap();
    }

    // start outputting audio
    let audio = sauti::audio::default();
    let _device = audio
        .start(DeviceOptions::default().with_sample_rate(44100), source)
        .expect("failed to start outputting sound");

    // TODO: resampler using rubato

    // keep sending the rest of the audio
    while let Some(packet) = stream.next_packet()? {
        sender.send(packet).unwrap();
    }

    println!("finished!");

    std::io::stdin()
        .read_line(&mut String::new())
        .expect("failed to read stdin");

    volume_handle.deactivate();

    std::io::stdin()
        .read_line(&mut String::new())
        .expect("failed to read stdin");

    volume_handle.activate();

    std::io::stdin()
        .read_line(&mut String::new())
        .expect("failed to read stdin");

    Ok(())
}

#[derive(Clone)]
struct AudioStreamSource<E, F>
where
    E: Effect,
    F: Fn() -> E + 'static,
{
    reciever: AudioStreamReciever,
    effect_builder: F,
}

impl<E, F> AudioStreamSource<E, F>
where
    E: Effect,
    F: Fn() -> E + 'static,
{
    pub fn new(reciever: Receiver<GenericPacket>, effect_builder: F) -> Self {
        Self {
            reciever: AudioStreamReciever { reciever },
            effect_builder,
        }
    }
}

#[derive(Clone)]
struct AudioStreamReciever {
    reciever: Receiver<GenericPacket>,
}

impl AudioStreamReciever {
    fn next_packet<S: ConvertibleSample>(
        &self,
        spec: &StreamSpec,
        effects: &mut impl Effect,
    ) -> SoundPacket<S> {
        let packet = self.reciever.recv().unwrap();
        let converted_packet = effects.apply_generic(packet, spec);
        converted_packet.convert::<S>()
    }
}

impl<E, F> SoundSource for AudioStreamSource<E, F>
where
    E: Effect,
    F: Fn() -> E + 'static,
{
    fn build<S: ConvertibleSample>(
        &self,
        info: DeviceInfo,
    ) -> impl FnMut(&mut [S]) + Send + Sync + 'static {
        let reciever = self.reciever.clone();
        let mut effects = (self.effect_builder)();
        let spec = info.into();
        let mut current_packet = reciever.next_packet(&spec, &mut effects);
        let mut current_index = 0;
        move |channels| {
            channels.copy_from_slice(
                &current_packet.interleaved_samples()
                    [current_index..current_index + channels.len()],
            );
            current_index += channels.len();
            if current_index >= current_packet.frame_count() * current_packet.channels() {
                current_packet = reciever.next_packet(&spec, &mut effects);
                current_index = 0;
            }
        }
    }
}
