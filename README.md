# Sauti

A Rust library for playing the audio or reading or writing the metadata of audio files. Essentially, it is meant to be everything you need to write a music player. Playback uses abstractions for audio playback, audio decoding, metadata decoding and writing, and audio effects, which can all be swapped out as needed.

By default:
- Playback uses [cpal](https://github.com/RustAudio/cpal)
- Audio decoding uses [symphonia](https://github.com/pdeljanov/Symphonia)
- Metadata uses:
  - [metaflac](https://github.com/jameshurst/rust-metaflac) for `flac` tags
  - [rust-id3](https://codeberg.org/polyfloyd/rust-id3) for `id3` tags
  - [mp4ameta](https://github.com/Saecki/mp4ameta) for `m4a` tags
  - [oggvorbismeta](https://github.com/HEnquist/lib-rust-oggvorbis-meta) for `ogg` tags
  - [symphonia](https://github.com/pdeljanov/Symphonia) for song durations

# Example

## Audio Player

```rust
use sauti::player::prelude::*;

pub fn main() -> Result<(), sauti::player::Disconnected> {
    // read the path to play as the first argument
    let Some(path) = std::env::args().nth(1) else {
        println!("usage: {{command}} {{path}}");
        return Ok(());
    };

    // create and start the audio player in another thread
    let handle = Player::builder().volumehttps://github.com/sautibara/sauti/blob/main/LICENSE(0.5).run();
    // begin playing the file by the path that was given
    handle.play(path)?;

    // continue playing until the user presses enter (to exit)
    std::io::stdin()
        .read_line(&mut String::new())
        .expect("failed to read stdin");

    Ok(())
}
```

## Metadata Decoder

```rust
use std::{error::Error, path::PathBuf};

use sauti::decoder::metadata::prelude::*;
use sauti::decoder::metadata::DynDecoder;

fn main() -> Result<(), Box<dyn Error>> {
    // Read the path to read as the first argument.
    let Some(path) = std::env::args().nth(1) else {
        println!("usage: {{command}} {{path}}");
        return Ok(());
    };

    // Create a metadata decoder using the default decoders.
    let decoder = sauti::decoder::metadata::default();

    // Read the path into an opaque metadata struct.
    let source = sauti::data::MediaSource::Path(PathBuf::from(&path));
    let metadata = decoder.read(&source)?;

    // Read the title, album, and artist of the file.
    let title = metadata.get(FrameId::Title);
    let title = title.as_string().unwrap_or("<unknown>");
    let album = metadata.get(FrameId::Album);
    let album = album.as_string().unwrap_or("<unknown>");
    let album = metadata.get(FrameId::Artist);
    let album = album.as_string().unwrap_or("<unknown>");

    // Then print out the components.
    println!("Track: '{title}' by '{artists}' in '{album}'");

    Ok(())
}
```

## Others

See other examples in the [examples directory](https://github.com/sautibara/sauti/tree/main/sauti/examples).

# Publishing

This project is not fully published yet. That is, it is not on `crates.io` yet. This is because this library is still in active development, as it was created for a music player that is not finished yet (and probably won't be finished for a long while). The top-level crate documentation is not finished yet because of this, although each individual part is heavily documented.

However, if you somehow find this library and want to use it yourself, please make a Github issue. I don't see myself changing this library much anymore, so it's probably okay for release at this point.

# License

This project is licensed under [GPL3](https://github.com/sautibara/sauti/blob/main/LICENSE).
