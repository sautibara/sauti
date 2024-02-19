use super::prelude::*;

#[derive(Debug)]
pub enum Message {
    Play(MediaSource),
    SetState(PlayState),
}

#[derive(Debug)]
pub enum AudioControl {
    Flush,
    SetState(PlayState),
}
