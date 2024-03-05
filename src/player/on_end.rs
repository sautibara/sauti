//! Provides a method for running actions after an audio file finishes playing.
//!
//! This could be used to stop the player after the file ends (which is what it is by [`default`]) or to
//! queue another file.
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
//!     .on_end_run(move |_| {
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

#[must_use]
pub const fn default() -> Default {
    Stop
}

pub type Default = Stop;

pub type BoxedPlayer<'a> = Box<dyn Generic<ModifyError = PlayerError, GetError = Infallible> + 'a>;

pub trait OnFileEnd: Send + 'static {
    /// # Errors
    ///
    /// - Any errors that are encountered when interacting with the player
    /// - Note: Errors are passed up to the player for it to handle
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

pub struct Stop;

impl OnFileEnd for Stop {
    fn file_ended(&self, player: &mut BoxedPlayer) -> PlayerResult<()> {
        player.stop()
    }
}
