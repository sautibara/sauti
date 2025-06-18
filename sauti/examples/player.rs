use sauti::player::prelude::*;

pub fn main() -> Result<(), sauti::player::Disconnected> {
    env_logger::init();

    let handle = Player::builder().volume(0.5).run();

    let Some(path) = std::env::args().nth(1) else {
        println!("usage: {{command}} {{path}}");
        return Ok(());
    };

    handle.play(path)?;

    std::io::stdin()
        .read_line(&mut String::new())
        .expect("failed to read stdin");

    Ok(())
}
