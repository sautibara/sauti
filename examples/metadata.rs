use std::{error::Error, path::PathBuf};

use sauti::decoder::metadata::prelude::*;
use sauti::decoder::metadata::DynDecoder;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let Some(path) = std::env::args().nth(1) else {
        println!(
            "usage: {} {{path}}",
            std::env::args().nth(0).unwrap_or("{command}".to_owned())
        );
        return Ok(());
    };

    let decoder = sauti::decoder::metadata::default();
    // Dynamic dispatch can be used if needed, and the api doesn't change at all.
    // If the line below is removed, the file will still compile and work the same.
    let decoder = Box::new(decoder) as Box<dyn DynDecoder>;

    let source = sauti::data::MediaSource::Path(PathBuf::from(&path));
    let metadata = decoder.read(&source)?;

    let title = metadata.get(FrameId::Title);
    let title = title.as_string().unwrap_or("<unknown>");
    let artist = metadata.get(FrameId::Artist);
    let artist = artist.as_string().unwrap_or("<unknown>");
    let album = metadata.get(FrameId::Album);
    let album = album.as_string().unwrap_or("<unknown>");

    println!("{title} by {artist} in {album}");

    for frame in metadata.frames() {
        println!("{frame:?}");
    }

    Ok(())
}
