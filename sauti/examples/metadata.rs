use std::{error::Error, path::PathBuf};

use gat_lending_iterator::LendingIterator;
use sauti::decoder::metadata::frame_iter::FrameIterExt;
use sauti::decoder::metadata::prelude::*;
use sauti::decoder::metadata::{DynDecoder, FrameId};

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let Some(path) = std::env::args().nth(1) else {
        println!(
            "usage: {} {{path}}",
            std::env::args()
                .next()
                .unwrap_or_else(|| "{command}".to_owned())
        );
        return Ok(());
    };

    let decoder = sauti::decoder::metadata::default();
    // Dynamic dispatch can be used if needed, and the api doesn't change at all.
    // If the line below is removed, the file will still compile and work the same.
    let decoder = Box::new(decoder) as Box<dyn DynDecoder>;

    let source = sauti::data::MediaSource::Path(PathBuf::from(&path));
    let mut metadata = decoder.read(&source)?;

    let title = metadata.get(FrameId::Title);
    let title = title.as_string().unwrap_or("<unknown>");
    let album = metadata.get(FrameId::Album);
    let album = album.as_string().unwrap_or("<unknown>");

    let iter = metadata.get_all(FrameId::Artist);
    let artists = separate_strings(iter.data(), " & ");

    println!("Track: '{title}' by '{artists}' in '{album}'");

    let duration = metadata.get(FrameId::Duration);
    if let Some(duration) = duration.as_duration() {
        println!(
            "Duration: {}:{}",
            duration.as_secs() / 60,
            duration.as_secs() % 60
        );
    }

    println!();

    for frame in metadata.frames() {
        println!("{frame:?}");
    }

    metadata.replace(FrameId::Title, Data::Text("meow".to_string()))?;
    metadata.save(path)?;

    Ok(())
}

/// Combines the strings in `iter` with commas in between.
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
