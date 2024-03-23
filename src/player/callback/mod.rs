use std::convert::Infallible;

use flagset::{flags, FlagSet};

use super::{prelude::*, Generic};

pub mod prelude {
    pub use super::error::OnError;
    pub use super::stream_end::OnStreamEnd;
    pub use super::{Action, ActionSet};
    pub use crate::player::callback;
}

pub mod error;
pub mod stream_end;

flags! {
    pub enum Action: u8 {
        Exit,
        Stop,
        // RestartOutput,
    }
}

pub type ActionSet = FlagSet<Action>;

/// A [`Generic`] player inside a trait object
pub type PlayerRef<'a> = Box<dyn Generic<ModifyError = PlayerError, GetError = Infallible> + 'a>;
