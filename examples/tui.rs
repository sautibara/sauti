use itertools::Itertools;
use regex::Regex;
use std::{io::Write, ops::ControlFlow, sync::OnceLock};

use sauti::player::{prelude::*, Disconnected, Handle};

pub fn main() -> Result<(), Disconnected> {
    env_logger::init();

    let handle = Player::builder().volume(0.5).run();

    loop {
        let line = read_line();
        if message(&handle, line.trim().split(' '))?.is_break() {
            break;
        }
    }

    Ok(())
}

/// # Panics
///
/// - If stdin is unreadable
#[must_use]
pub fn read_line() -> String {
    query();
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .expect("failed to read stdin");
    line
}

/// # Panics
///
/// - If [`std::io::stdout`] failed flushing
pub fn query() {
    print!("> ");
    std::io::stdout().flush().expect("failed to flush stdout");
}

/// # Errors
///
/// - If the player disconnects
pub fn message<'a>(
    handle: &Handle,
    mut message: impl Iterator<Item = &'a str>,
) -> Result<ControlFlow<()>, Disconnected> {
    match message.next() {
        Some("exit") => return Ok(ControlFlow::Break(())),
        Some("help") => println!(
            "A simple implementation of a player

Commands:
- play <path>
- state [<state>]
    - set_state <state>
        - pause
        - resume
        - stop
    - get_state
- volume [<volume>]
    - get_volume
    - set_volume <volume>
- seek_to <duration>
- seek_by <duration> <direction>
- get_times
    - position
    - duration"
        ),
        Some("play") => {
            // make all other parts into a string
            #[allow(unstable_name_collisions)] // it'll do the same thing anyways
            let path: String = message.intersperse(" ").collect();
            play(handle, &path)?;
        }
        Some("state") => {
            if let Some(state) = message.next() {
                set_parsed_state(handle, Some(state))?;
            } else {
                get_state(handle)?;
            }
        }
        Some("set_state") => set_parsed_state(handle, message.next())?,
        Some("pause") => set_state(handle, PlayState::Paused)?,
        Some("resume") => set_state(handle, PlayState::Playing)?,
        Some("stop") => set_state(handle, PlayState::Stopped)?,
        Some("get_state") => get_state(handle)?,
        Some("volume") => {
            if let Some(volume) = message.next() {
                set_volume(handle, Some(volume))?;
            } else {
                get_volume(handle)?;
            }
        }
        Some("get_volume") => get_volume(handle)?,
        Some("set_volume") => set_volume(handle, message.next())?,
        Some("seek_to") => seek_to(handle, message.next())?,
        Some("seek_by") => seek_by(handle, message.next(), message.next())?,
        Some("get_times" | "times" | "position" | "duration") => get_times(handle)?,
        Some(unrecognized) => println!("unrecognized command: {unrecognized}"),
        None => (),
    }
    Ok(ControlFlow::Continue(()))
}

fn play(handle: &Handle, path: &str) -> Result<(), Disconnected> {
    if path.is_empty() {
        println!("Usage: <play> <path>");
    } else {
        handle.play(path)?;
    }
    Ok(())
}

fn set_parsed_state(handle: &Handle, state: Option<&str>) -> Result<(), Disconnected> {
    if let Some(state) = state.and_then(parse_state) {
        set_state(handle, state)?;
    } else {
        println!("Usage: set_state <state>");
    }
    Ok(())
}

fn set_state(handle: &Handle, state: PlayState) -> Result<(), Disconnected> {
    if !handle.set_state(state)? {
        println!("failed to change state to {state:?}: player is stopped");
    }
    Ok(())
}

fn get_state(handle: &Handle) -> Result<(), Disconnected> {
    println!("{:?}", handle.play_state()?);
    Ok(())
}

fn get_volume(handle: &Handle) -> Result<(), Disconnected> {
    println!("{}", handle.volume()?);
    Ok(())
}

fn set_volume(handle: &Handle, volume: Option<&str>) -> Result<(), Disconnected> {
    if let Some(volume) = volume.and_then(|val| val.parse().ok()) {
        handle.set_volume(volume)?;
    } else {
        println!("Usage: <volume> <float>");
    }
    Ok(())
}

fn seek_to(handle: &Handle, duration: Option<&str>) -> Result<(), Disconnected> {
    if let Some(duration) = duration.and_then(parse_duration) {
        handle.seek_to(duration)?;
    } else {
        println!(
            "Usage: seek_to <duration>
- {{duration}} can either be in the form of
    - '1.8'
    - '5m1.0s'"
        );
    }
    Ok(())
}

fn seek_by(
    handle: &Handle,
    duration: Option<&str>,
    direction: Option<&str>,
) -> Result<(), Disconnected> {
    if let Some((duration, direction)) =
        (duration.and_then(parse_duration)).zip(direction.and_then(parse_direction))
    {
        handle.seek_by(duration, direction)?;
    } else {
        println!(
            "Usage: seek_by <duration> [<direction>]
- <duration> can either be in the form of
    - '1.8'
    - '5m1.0s'
- <direction> can be either be
    - 'f' or 'forward'
    - 'b' or 'backward'"
        );
    }
    Ok(())
}

fn get_times(handle: &Handle) -> Result<(), Disconnected> {
    #[allow(clippy::option_if_let_else)] // too big of an if statement
    if let Some(times) = handle.times()? {
        println!(
            "pos: {:?}, dur: {:?} ({:.1}%)",
            times.position(),
            times.duration(),
            times.progress() * 100.0
        );
    } else {
        println!("no song playing :(");
    }
    Ok(())
}

fn parse_state(val: &str) -> Option<PlayState> {
    match val.to_lowercase().trim_start_matches("playstate::") {
        "playing" | "play" => Some(PlayState::Playing),
        "paused" | "pause" => Some(PlayState::Paused),
        "stopped" | "stop" => Some(PlayState::Stopped),
        _ => None,
    }
}

fn parse_duration(val: &str) -> Option<Duration> {
    static RE: OnceLock<Regex> = OnceLock::new();

    // if it's just a simple number, treat it as seconds
    if let Ok(secs) = val.parse() {
        return Some(Duration::from_secs_f64(secs));
    }

    // lazily create the regex to avoid creating it every time
    let regex = RE.get_or_init(|| {
        // regex matches things like 5m, 1m1s, or 1.5m
        Regex::new(r"^\s*(?:(\d+(?:\.\d+)?)m)?(?:(\d+(?:\.\d+)?)s)?\s*$")
            .expect("regex is already guaranteed to compile; it should compile")
    });

    let matched = regex.captures(val)?;
    // parse each capture group or return None
    let seconds: f64 = (matched.get(2))
        .map_or(Ok(0.0), |matched| matched.as_str().parse())
        .ok()?;
    let minutes: f64 = (matched.get(1))
        .map_or(Ok(0.0), |matched| matched.as_str().parse())
        .ok()?;

    Some(Duration::from_secs_f64(minutes.mul_add(60.0, seconds)))
}

fn parse_direction(val: &str) -> Option<Direction> {
    match val {
        "f" | "forward" => Some(Direction::Forward),
        "b" | "backward" => Some(Direction::Backward),
        _ => None,
    }
}
