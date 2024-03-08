//! Gives the ability to run actions after an audio file finishes playing.
//!
//! The default behavior is to [stop](Generic::stop) the player after a song ends. This can be
//! overridden if necessary, such as if there is a queue of songs.
//!
//! # Examples
//!
//! ```
//! # use sauti::player::PlayerResult;
//! # fn main() -> PlayerResult<()> {
//! use sauti::player::prelude::*;
//! use sauti::test::prelude::*;
//!
//! // A simple unit message
//! struct Exit;
//!
//! // A channel that exits the program when a message is sent
//! let (sender, reciever) = crossbeam_channel::bounded(0);
//! // An [`Empty`] player does nothing with its data
//! let player = Empty::player()
//!     // Once the song ends, tell the main thread to exit
//!     .on_file_end_run(move |_| {
//!         sender.send(Exit).expect("failed to send message");
//!         Ok(())
//!     })
//!     // Start the player in another thread
//!     .run();
//!
//! // Start decoding (and providing the data)
//! // [`Empty`] ignores the [`MediaSource`], so just send an empty path
//! player.play("")?;
//!
//! // When decoding a file, [`Empty`] returns None, so the file will end immediately.
//! let res = reciever.recv_timeout(Duration::from_secs(1));
//! // If the player takes too long, then it'll return Err
//! assert!(res.is_ok(), "empty player should end immediately");
//!
//! // When the handle is dropped, it exits the player
//! // Drop at the very end to prevent this from happening
//! drop(player);
//! # Ok(()) }
//! ```
use super::prelude::*;
use super::Generic;
use std::convert::Infallible;

/// Get the default [`OnFileEnd`] callback
#[must_use]
pub const fn default() -> Default {
    Stop
}

/// The output type of [`default`]
pub type Default = Stop;

/// A [`Generic`] player inside a trait object
pub type BoxedPlayer<'a> = Box<dyn Generic<ModifyError = PlayerError, GetError = Infallible> + 'a>;

/// A callback for when a file ends in a [`Player`]
///
/// [`Fn(&mut BoxedPlayer) -> PlayerResult<()>`](Fn) is a notable implementor of this.
pub trait OnFileEnd: Send + 'static {
    /// A callback for when a file ends in the player. `player` can be used to control the player.
    ///
    /// The default behavior is to stop, so it's often advised to call [`Generic::stop`] in this.
    ///
    /// Errors passed up in the return value are delegated to the player to handle.
    ///
    /// # Errors
    ///
    /// - Any errors that are encountered when interacting with the player
    fn file_ended(&self, player: &mut BoxedPlayer) -> PlayerResult<()>;
}

impl<F> OnFileEnd for F
where
    F: Fn(&mut BoxedPlayer) -> PlayerResult<()> + Send + 'static,
{
    fn file_ended(&self, player: &mut BoxedPlayer) -> PlayerResult<()> {
        self(player)
    }
}

/// [Stop](Generic::stop) the [`Player`] after a song ends
pub struct Stop;

impl OnFileEnd for Stop {
    fn file_ended(&self, player: &mut BoxedPlayer) -> PlayerResult<()> {
        player.stop()
    }
}
