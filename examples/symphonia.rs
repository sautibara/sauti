use std::path::Path;

use sauti::{audio::prelude::*, decoder::prelude::*, effect::prelude::*};

use crossbeam_channel::Receiver;

// NOTE: this is a proof of concept - not meant to be an example
fn main() -> DecoderResult<()> {
    // decode the file given in the command line
    let path = std::env::args().nth(1).expect("usage: {command} {path}");

    // set up a stream between the decoder and the audio output
    let (sender, reciever) = crossbeam_channel::unbounded();
    // decode the file in another thread
    let decoder_result = std::thread::spawn(move || {
        let decoder = sauti::decoder::default();
        let mut stream = decoder.read(Path::new(&path))?;

        while (stream.next_packet()?)
            .and_then(|packet| sender.send(packet).ok())
            .is_some()
        {}

        println!("finished!");
        Ok(())
    });

    // set up a handle to activate or deactivate the volume
    let volume_handle = effect::Volume::create_handle(0.1);

    let _device = {
        // output audio (also in another thread)
        let volume_handle = volume_handle.clone();
        let audio = sauti::audio::default();
        audio
            .start(
                DeviceOptions::default().with_sample_rate(44100),
                AudioStreamSource {
                    reciever,
                    effects: effect::ResizeChannels.then(effect::Volume(volume_handle)),
                },
            )
            .expect("failed to start outputting sound")
    };

    std::io::stdin()
        .read_line(&mut String::new())
        .expect("failed to read stdin");

    volume_handle.set(1.0);

    std::io::stdin()
        .read_line(&mut String::new())
        .expect("failed to read stdin");

    volume_handle.set(0.5);

    std::io::stdin()
        .read_line(&mut String::new())
        .expect("failed to read stdin");

    if decoder_result.is_finished() {
        decoder_result.join().unwrap()
    } else {
        Ok(())
    }
}

#[derive(Clone)]
struct AudioStreamSource<E: Effect> {
    reciever: Receiver<GenericPacket>,
    effects: E,
}

impl<E: Effect> AudioStreamSource<E> {
    fn next_packet<S: ConvertibleSample>(&mut self, spec: &StreamSpec) -> SoundPacket<S> {
        let packet = self.reciever.recv().unwrap();
        let converted_packet = self.effects.apply_to_generic(packet, spec);
        converted_packet.convert::<S>()
    }
}

impl<E: Effect> SoundSource for AudioStreamSource<E> {
    fn build<S: ConvertibleSample>(
        &self,
        info: DeviceInfo,
    ) -> impl FnMut(&mut [S]) + Send + Sync + 'static {
        let mut source = self.clone();
        let spec = info.into();

        let mut current_packet = source.next_packet(&spec);
        let mut current_index = 0;

        move |channels| {
            channels.copy_from_slice(
                &current_packet.interleaved_samples()
                    [current_index..current_index + channels.len()],
            );
            current_index += channels.len();
            if current_index >= current_packet.frame_count() * current_packet.channels() {
                current_packet = source.next_packet(&spec);
                current_index = 0;
            }
        }
    }
}
