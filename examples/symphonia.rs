use std::path::Path;

use sauti::{
    audio::{
        prelude::{ConvertibleSample, DeviceInfo},
        Audio, DeviceOptions, SoundSource,
    },
    file::{Decoder, GenericPacket},
};

use crossbeam_channel::Receiver;

// NOTE: this is a proof of concept - not meant to be an example
fn main() -> sauti::file::FileResult<()> {
    let path = std::env::args().nth(1).expect("usage: {command} {path}");
    let decoder = sauti::file::default_decoder();
    let mut stream = decoder.read(Path::new(&path))?;

    let (sender, reciever) = crossbeam_channel::unbounded();
    let source = AudioStreamSource { reciever };

    if let Some(packet) = stream.next_packet()? {
        sender.send(packet).unwrap();
    }

    let audio = sauti::audio::default();
    let _device = audio
        .start(DeviceOptions::default().with_sample_rate(44100), source)
        .expect("failed to start outputting sound");

    while let Some(packet) = stream.next_packet()? {
        sender.send(packet).unwrap();
    }

    println!("finished!");

    std::io::stdin()
        .read_line(&mut String::new())
        .expect("failed to read stdin");
    Ok(())
}

struct AudioStreamSource {
    reciever: Receiver<GenericPacket>,
}

impl SoundSource for AudioStreamSource {
    fn build<S: ConvertibleSample>(
        &self,
        _: DeviceInfo,
    ) -> impl FnMut(&mut [S]) + Send + Sync + 'static {
        let reciever = self.reciever.clone();
        let mut current_packet = reciever.recv().unwrap().convert::<S>();
        let mut current_index = 0;
        move |channels| {
            channels.copy_from_slice(
                &current_packet.interleaved_samples()
                    [current_index..current_index + channels.len()],
            );
            current_index += channels.len();
            if current_index >= current_packet.frames() * current_packet.channels() {
                current_packet = reciever.recv().unwrap().convert::<S>();
                current_index = 0;
            }
        }
    }
}
