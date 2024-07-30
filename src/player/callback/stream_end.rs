//! Gives the ability to run actions after an audio file finishes playing.
//!
//! The default behavior is to [stop](super::Generic::stop) the player after a song ends. This can be
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
//!     // Once the stream ends, tell the main thread to exit
//!     .on_stream_end_run(move |_| {
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
use log::trace;
use thiserror::Error;

use super::super::prelude::*;

/// Get the default [`OnStreamEnd`] callback
#[must_use]
pub const fn default() -> Default {
    Stop
}

/// The output type of [`default`]
pub type Default = Stop;

/// A callback for when a file ends in a [`Player`]
///
/// [`Fn(&mut BoxedPlayer) -> PlayerResult<()>`](Fn) is a notable implementor of this.
#[allow(clippy::module_name_repetitions)] // on::file_end
pub trait OnStreamEnd: Send + 'static {
    /// A callback for when a file ends in the player.
    ///
    /// The default behavior is to stop when the file ends, so it's often advised to call
    /// [`Generic::stop`](super::Generic::stop) when `info.reason == Reason::FileEnded`
    /// in this.
    ///
    /// Notably, this callback is run before the stream is stopped decoding. This allows methods
    /// like [`Generic::times`](super::Generic::times) to be run on the previous stream.
    ///
    /// Errors passed up in the return value are delegated to the player to handle.
    ///
    /// # Errors
    ///
    /// - Any errors that are encountered when interacting with the player
    fn stream_ended(&self, info: Info<'_>) -> PlayerResult<()>;
}

impl<F> OnStreamEnd for F
where
    F: Fn(Info<'_>) -> PlayerResult<()> + Send + 'static,
{
    fn stream_ended(&self, info: Info<'_>) -> PlayerResult<()> {
        self(info)
    }
}

/// [Stop](super::Generic::stop) the [`Player`] after a file ends
pub struct Stop;

impl OnStreamEnd for Stop {
    fn stream_ended(&self, mut info: Info<'_>) -> PlayerResult<()> {
        if info.reason.is_end_of_file() {
            trace!("stream ended because {}; stopping player", info.reason);
            info.player.stop()?;
        }
        Ok(())
    }
}

pub struct Info<'a> {
    pub source: SourceName,
    pub reason: Reason,
    pub player: PlayerRef<'a>,
}

#[derive(Error, Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Reason {
    #[error("it was stopped")]
    Stop,
    #[error("the file ended")]
    EndOfFile,
    #[error("it was replaced")]
    Replaced,
}

impl Reason {
    /// Returns `true` if the stream ended reason is [`Stop`].
    ///
    /// [`Stop`]: Reason::Stop
    #[must_use]
    pub const fn is_stop(&self) -> bool {
        matches!(self, Self::Stop)
    }

    /// Returns `true` if the stream ended reason is [`EndOfFile`].
    ///
    /// [`EndOfFile`]: Reason::EndOfFile
    #[must_use]
    pub const fn is_end_of_file(&self) -> bool {
        matches!(self, Self::EndOfFile)
    }

    /// Returns `true` if the reason is [`Replaced`].
    ///
    /// [`Replaced`]: Reason::Replaced
    #[must_use]
    pub const fn is_replaced(&self) -> bool {
        matches!(self, Self::Replaced)
    }
}
