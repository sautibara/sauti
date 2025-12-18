use sauti::player::prelude::*;

pub fn main() -> Result<(), sauti::player::Disconnected> {
    // read the path to play as the first argument
    let Some(path) = std::env::args().nth(1) else {
        println!("usage: {{command}} {{path}}");
        return Ok(());
    };

    // create and start the audio player in another thread
    let handle = Player::builder().volume(0.5).run();
    // begin playing the file by the path that was given
    handle.play(path)?;

    // continue playing until the user presses enter to exit
    std::io::stdin()
        .read_line(&mut String::new())
        .expect("failed to read stdin");

    Ok(())
}
