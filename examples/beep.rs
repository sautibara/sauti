use sauti::audio::{Audio, ConvertibleSample, DeviceInfo, DeviceOptions, SoundSource};

// this program outputs a 440.0 hz sin wave on the main device
fn main() {
    let audio = sauti::audio::default();
    let _device = audio
        .start(DeviceOptions::default(), Beep { frequency: 440.0 })
        .expect("failed to start outputting sound");

    // wait for something in the console, ignore it, and then exit
    std::io::stdin()
        .read_line(&mut String::new())
        .expect("failed to read stdin");
}

struct Beep {
    frequency: f64,
}

impl SoundSource for Beep {
    // the sound source is generic over the sample type
    fn build<S: ConvertibleSample>(
        &self,
        info: DeviceInfo,
    ) -> impl FnMut(&mut [S]) + Send + Sync + 'static {
        // config from the source can be passed in
        let frequency = self.frequency;
        // and internal variables can be initialized outside the closure
        let mut clock = 0;

        // this closure is run for each sample to get the values
        // it's given a mutable slice `channels` that holds each channel of the current sample
        move |channels| {
            clock = (clock + 1) % info.sample_rate;
            let val =
                (clock as f64 * frequency * std::f64::consts::TAU / info.sample_rate as f64).sin();
            // S::from_sample must be used to convert the f64 value to the generic sample type
            channels.fill(S::from_sample(val * 0.1));
        }
    }
}
