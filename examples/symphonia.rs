use std::{ops::Add, path::Path};

use dasp_sample::Sample;
use sauti::{
    audio::{
        prelude::{ConvertibleSample, DeviceInfo},
        Audio, DeviceOptions, SoundSource,
    },
    file::{Decoder, GenericPacket, SoundPacket},
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
    let source = AudioStreamSource { reciever };

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

    Ok(())
}

#[derive(Clone)]
struct AudioStreamSource {
    reciever: Receiver<GenericPacket>,
}

impl AudioStreamSource {
    fn next_packet<S: ConvertibleSample>(&self, to_channels: usize) -> SoundPacket<S> {
        let packet = self.reciever.recv().unwrap().convert::<S>();
        packet.resize_and_map_channels(to_channels, Self::map_channels)
    }

    fn map_channels<S: ConvertibleSample>(
        frame: &mut [S],
        from_channels: usize,
        to_channels: usize,
    ) {
        // if the channels are already fine, then do nothing
        if from_channels == to_channels || to_channels == 0 || frame.is_empty() {
            return;
        }
        // find the average value of all of the channels
        let average = if from_channels == 1 {
            frame[0]
        } else {
            let sum = (frame.iter())
                // only signed samples can multiply
                .map(|sample| sample.to_signed_sample())
                // Sample doesn't implement Sum, so use reduce instead
                .reduce(Add::add)
                // already checked `frame.is_empty()` above
                .expect("reduce on a non-empty iterator should always return something")
                // bring it back to the original sample type
                .to_sample::<S>();
            let amount = S::from_sample(from_channels as u32);
            (sum.to_float_sample() / amount.to_float_sample()).to_sample::<S>()
        };
        // fill the channels with the average
        frame.fill(average)
    }
}

impl SoundSource for AudioStreamSource {
    fn build<S: ConvertibleSample>(
        &self,
        info: DeviceInfo,
    ) -> impl FnMut(&mut [S]) + Send + Sync + 'static {
        let reciever = self.clone();
        let mut current_packet = reciever.next_packet(info.channels as usize);
        let mut current_index = 0;
        move |channels| {
            channels.copy_from_slice(
                &current_packet.interleaved_samples()
                    [current_index..current_index + channels.len()],
            );
            current_index += channels.len();
            if current_index >= current_packet.frame_count() * current_packet.channels() {
                current_packet = reciever.next_packet(info.channels as usize);
                current_index = 0;
            }
        }
    }
}
