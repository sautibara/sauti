//! Gives the ability to run actions when errors are enountered
//!
//! This can also be used to run a custom callback when a file ends, as that is one of the
//! potential errors (see [`Builder::on_error`](super::super::Builder::on_error)).
//!
//! The [`default`] is a [logged](Log) [`Recover`], which tries to recover the error as best
//! as it can. For example,
//! [`DecoderError::UnsupportedFormat`](crate::decoder::audio::DecoderError::UnsupportedFormat)
//! returns [`Action::Stop`] instead of exiting the player.
//!
// TODO: example

use super::{super::prelude::*, Action, ActionSet, PlayerRef};

use log::trace;

#[allow(clippy::module_name_repetitions)] // on::error
pub trait OnError: Send + 'static {
    fn handle(&self, err: PlayerError, player: PlayerRef) -> impl Into<ActionSet>;

    fn logged(self) -> Log<Self>
    where
        Self: Sized,
    {
        Log { inner: self }
    }
}

#[must_use]
pub fn default() -> Default {
    Recover.logged()
}

pub type Default = Log<Recover>;

impl<A: Into<ActionSet>, F: Fn(PlayerError, PlayerRef) -> A + Send + 'static> OnError for F {
    fn handle(&self, err: PlayerError, player: PlayerRef) -> impl Into<ActionSet> {
        self(err, player)
    }
}

pub struct Log<C: OnError> {
    inner: C,
}

impl<C: OnError> OnError for Log<C> {
    fn handle(&self, err: PlayerError, player: PlayerRef) -> impl Into<ActionSet> {
        log::log!(err.log_level(), "{err}");
        let actions = self.inner.handle(err, player).into();
        trace!("actions to run: {actions:?}");
        actions
    }
}

pub struct Recover;

impl OnError for Recover {
    #[inline]
    fn handle(&self, err: PlayerError, _: PlayerRef) -> impl Into<ActionSet> {
        match err {
            // TODO: there's some more that can be recovered
            PlayerError::Decoder(_) => Action::Stop,
            _ => Action::Exit,
        }
    }
}
