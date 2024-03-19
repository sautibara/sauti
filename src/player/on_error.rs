//! Gives the ability to run actions when errors are enountered
//!
//! This can also be used to run a custom callback when a file ends, as that is one of the
//! potential errors (see [`Builder::on_file_end`]).
//!
//! The [`default`] is a [logged](Log) [`Recover`], which tries to recover the error as best
//! as it can. For example, [`PlayerError::StreamEnded`] and [`DecoderError::UnsupportedFormat`]
//! both return [`Action::Stop`] instead of exiting the player.
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
//!     .on_file_end(move |_, _| {
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
// TODO: another example
use std::convert::Infallible;

use super::{prelude::*, Generic, StreamEndedReason};
use crate::decoder::DecoderError;

use flagset::{flags, FlagSet};
use log::trace;

flags! {
    pub enum Action: u8 {
        Exit,
        Stop,
        RestartOutput,
    }
}

/// A [`Generic`] player inside a trait object
pub type BoxedPlayer<'a> = Box<dyn Generic<ModifyError = PlayerError, GetError = Infallible> + 'a>;

pub trait OnError: Send + 'static {
    fn handle(&self, err: PlayerError, player: &mut BoxedPlayer) -> impl Into<FlagSet<Action>>;

    fn logged(self) -> Log<Self>
    where
        Self: Sized,
    {
        Log { inner: self }
    }

    fn on_stream_end<F>(self, func: F) -> OnStreamEnd<Self, F>
    where
        Self: Sized,
        F: Fn(&mut BoxedPlayer, StreamEndInfo) -> PlayerResult<()> + Send,
    {
        OnStreamEnd {
            on_error: self,
            func,
        }
    }
}

#[must_use]
pub fn default() -> Default {
    Recover.logged()
}

pub type Default = Log<Recover>;

pub struct Log<C: OnError> {
    inner: C,
}

impl<C: OnError> OnError for Log<C> {
    fn handle(&self, err: PlayerError, player: &mut BoxedPlayer) -> impl Into<FlagSet<Action>> {
        log::log!(err.log_level(), "{err}");
        let actions = self.inner.handle(err, player).into();
        trace!("actions to run: {actions:?}");
        actions
    }
}

pub struct StreamEndInfo {
    pub source: SourceName,
    pub reason: StreamEndedReason,
}

// TODO: this is way too much of a bodge,
// bring back to what it was please
pub struct OnStreamEnd<C, F>
where
    C: OnError,
    F: Fn(&mut BoxedPlayer, StreamEndInfo) -> PlayerResult<()> + Send,
{
    on_error: C,
    func: F,
}

impl<C, F> OnError for OnStreamEnd<C, F>
where
    C: OnError,
    F: Fn(&mut BoxedPlayer, StreamEndInfo) -> PlayerResult<()> + Send + 'static,
{
    fn handle<'a>(
        &self,
        err: PlayerError,
        player: &'a mut BoxedPlayer,
    ) -> impl Into<FlagSet<Action>> {
        match err {
            PlayerError::StreamEnded { source, reason } => {
                if let Err(err) = (self.func)(player, StreamEndInfo { source, reason }) {
                    self.on_error.handle(err, player).into()
                } else {
                    FlagSet::default()
                }
            }
            err => self.on_error.handle(err, player).into(),
        }
    }
}

pub struct Recover;

impl OnError for Recover {
    #[inline]
    fn handle(&self, err: PlayerError, _: &mut BoxedPlayer) -> impl Into<FlagSet<Action>> {
        match err {
            // TODO: there's some more that can be recovered
            PlayerError::Decoder(DecoderError::UnsupportedFormat { .. }) => Action::Stop.into(),
            PlayerError::StreamEnded { reason, .. } => match reason {
                // the player is already stopped, so there is no reason to stop it again
                super::StreamEndedReason::Stop | StreamEndedReason::Replaced => FlagSet::default(),
                super::StreamEndedReason::EndOfFile => Action::Stop.into(),
            },
            _ => Action::Exit.into(),
        }
    }
}
