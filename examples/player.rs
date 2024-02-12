use sauti::player::prelude::*;

pub fn main() {
    let volume_handle = effect::Volume::create_handle(0.5);
    let handle = Player::default_builder()
        .add_effect(effect::Volume(volume_handle))
        .run();

    let path = std::env::args().nth(1).expect("usage: {command} {path}");
    handle.play(path).unwrap();

    std::io::stdin()
        .read_line(&mut String::new())
        .expect("failed to read stdin");

    handle.pause().unwrap();

    std::io::stdin()
        .read_line(&mut String::new())
        .expect("failed to read stdin");

    handle.resume().unwrap();

    std::io::stdin()
        .read_line(&mut String::new())
        .expect("failed to read stdin");
}
