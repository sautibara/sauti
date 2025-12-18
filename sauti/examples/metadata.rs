use std::{error::Error, path::PathBuf};

use sauti::decoder::metadata::prelude::*;
use sauti::decoder::metadata::DynDecoder;

fn main() -> Result<(), Box<dyn Error>> {
    // Read the path to read as the first argument.
    let Some(path) = std::env::args().nth(1) else {
        println!(
            "usage: {} {{path}}",
            std::env::args()
                .next()
                .unwrap_or_else(|| "{command}".to_owned())
        );
        return Ok(());
    };

    // Create a metadata decoder using the default decoders.
    let decoder = sauti::decoder::metadata::default();
    // Dynamic dispatch can be used if needed, and the api doesn't change at all.
    // If the line below is added or removed, the file will still compile and work the same.
    // let decoder = Box::new(decoder) as Box<dyn DynDecoder>;

    // Read the path into an opaque metadata struct.
    let source = sauti::data::MediaSource::Path(PathBuf::from(&path));
    let metadata = decoder.read(&source)?;

    // Read the title, album, and artist of the file.
    let title = metadata.get(FrameId::Title);
    let title = title.as_string().unwrap_or("<unknown>");
    let album = metadata.get(FrameId::Album);
    let album = album.as_string().unwrap_or("<unknown>");

    // Tags with multiple values are supported.
    let iter = metadata.get_all(FrameId::Artist);
    let artists = separate_strings(iter.data(), " & ");

    println!("Track: '{title}' by '{artists}' in '{album}'");

    // Read the length of the file. This will work for any file that symphonia supports.
    let duration = metadata.get(FrameId::Duration);
    if let Some(duration) = duration.as_duration() {
        println!(
            "Duration: {}:{}",
            duration.as_secs() / 60,
            duration.as_secs() % 60
        );
    }

    println!();

    // Then iterate through and print out all of the frames.
    for frame in metadata.frames() {
        println!("{frame:?}");
    }

    Ok(())
}

/// Combines the strings in `iter` with a certain separator in between.
fn separate_strings<'a>(iter: impl Iterator<Item = DataCow<'a>>, separator: &str) -> String {
    let mut iter = iter.strings();
    let mut output: Option<String> = None;
    // due to an issue with the language, `iter.strings()` cannot use iterator combinators, so we
    // have to do a imperative loop here (instead of [`LendingIterator::fold`], for example).
    while let Some(next) = iter.next() {
        if let Some(artists) = output.as_mut() {
            artists.push_str(separator);
            artists.push_str(next);
        } else {
            output = Some(next.to_owned());
        }
    }
    output.unwrap_or_else(|| "<unknown>".to_owned())
}
