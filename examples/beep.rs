use sauti::audio::{Audio, DeviceOptions, SampleContext};

fn main() {
    let audio = sauti::audio::default_audio();
    let _device = audio.start(DeviceOptions::default(), sound);

    // wait for something in the console and then exit
    let mut string = String::new();
    std::io::stdin()
        .read_line(&mut string)
        .expect("expected something");
}

fn sound() -> impl FnMut(&mut [f32], &SampleContext) {
    let mut clock = 0;
    move |channels, context| {
        clock = (clock + 1) % context.sample_rate;
        let val = (clock as f32 * 440.0 * std::f32::consts::TAU / context.sample_rate as f32).sin();
        channels.fill(val * 0.1);
    }
}
