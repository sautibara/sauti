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

```rust
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

    // continue playing until the user presses enter (to exit)
    std::io::stdin()
        .read_line(&mut String::new())
        .expect("failed to read stdin");

    Ok(())
}
```

See other examples in the [examples directory](https://github.com/sautibara/sauti/tree/main/sauti/examples).

# Publishing

This project is not fully published yet. That is, it is not on `crates.io` yet. This is because this library is still in active development, as it was created for a music player that is not finished yet (and probably won't be finished for a long while). The top-level crate documentation is not finished yet because of this, although each individual part is heavily documented.

However, if you somehow find this library and want to use it yourself, please make a Github issue. I don't see myself changing this library much anymore, so it's probably okay for release at this point.

# License

This project is licensed under [GPL3](https://github.com/sautibara/sauti/blob/main/LICENSE).
