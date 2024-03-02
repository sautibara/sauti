/// Provides a method for doing actions after a song ends.
///
/// You could use this to stop the player after a song (which is what it is by [`default`]) or
/// queue the next song.
use crate::decoder::Decoder;

use super::{Inner, PlayerResult};

#[must_use]
pub const fn default() -> Default {
    Stop
}

pub type Default = Stop;

pub trait OnEnd<D: Decoder>: Send + 'static {
    /// # Errors
    ///
    /// - Any errors that are encountered when interacting with the player
    /// - Errors are passed up to the player for it to handle
    fn on_end(&self, player: &mut Inner<D, Self>) -> PlayerResult<()>
    where
        Self: Sized;
}

pub struct Stop;

impl<D: Decoder> OnEnd<D> for Stop {
    fn on_end(&self, player: &mut Inner<D, Self>) -> PlayerResult<()> {
        player.set_state(super::PlayState::Stopped)
    }
}
