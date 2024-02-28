use sauti::player::{prelude::*, Disconnected};

pub fn main() -> Result<(), Disconnected> {
    env_logger::init();

    let handle = Player::default_builder().volume(0.5).run();

    let Some(path) = std::env::args().nth(1) else {
        println!("usage: {{command}} {{path}}");
        return Ok(());
    };

    handle.play(path)?;

    // loop {
    //     println!("{:?}", handle.times());
    //     std::thread::sleep(Duration::from_millis(1));
    // }

    std::io::stdin()
        .read_line(&mut String::new())
        .expect("failed to read stdin");

    // handle.pause()?;
    handle.set_volume(0.1)?;

    std::io::stdin()
        .read_line(&mut String::new())
        .expect("failed to read stdin");

    // handle.resume()?;
    handle.set_volume(0.5)?;

    std::io::stdin()
        .read_line(&mut String::new())
        .expect("failed to read stdin");

    Ok(())
}
