use sauti::player::prelude::*;

pub fn main() {
    let path = std::env::args().nth(1).expect("usage: {command} {path}");
    let volume_handle = effect::Volume::create_handle(0.5);
    let handle = PlayerBuilder::default()
        .add_effect(effect::Volume(volume_handle))
        .run();
    handle.play(path).unwrap();

    std::io::stdin()
        .read_line(&mut String::new())
        .expect("failed to read stdin");
}
