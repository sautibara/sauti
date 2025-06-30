//! Decoding of audio files' audio and metadata

use std::{
    collections::HashSet, default::Default as _, ffi::OsStr, fmt::Debug, ops::BitOr, path::Path,
    time::Duration,
};

#[cfg(feature = "audio-decoder")]
pub mod audio;
#[cfg(feature = "metadata-decoder")]
pub mod metadata;

pub mod symphonia;

/// A decoder implemented using [`symphonia`](::symphonia)
pub use symphonia::Symphonia;

use crate::data::prelude::*;

#[cfg(feature = "audio-decoder")]
pub use audio::Decoder as AudioDecoder;
#[cfg(feature = "metadata-decoder")]
pub use metadata::Decoder as MetadataDecoder;

/// The maximum set of extensions that a specific decoder can decode
///
/// This should list all extensions that the decoder could decode, even if it can't decode every
/// single file with that extension. It could be used to determine if a file looks like an audio
/// file, for example.
///
/// # Examples
///
/// ```
/// use sauti::decoder::ExtensionSet;
/// use std::path::Path;
///
/// let set = ExtensionSet::from_slice(&["mp3", "flac"]);
///
/// assert!(set.contains("mp3"));
/// assert!(!set.contains("txt"));
///
/// let path = Path::new("file.mp3");
/// assert!(set.matches_path(&path));
/// let path = Path::new("text.txt");
/// assert!(!set.matches_path(&path));
/// ```
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ExtensionSet {
    set: HashSet<String>,
}

impl ExtensionSet {
    /// Build the [`ExtensionSet`] with no supported extensions
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Build the [`ExtensionSet`] from a [`HashSet`] of possible extensions.
    #[must_use]
    pub const fn new(set: HashSet<String>) -> Self {
        Self { set }
    }

    /// Build the [`ExtensionSet`] from a slice of possible extensions.
    pub fn from_slice<S: AsRef<str>>(slice: &[S]) -> Self {
        Self::from(slice)
    }

    /// Get a reference to the set of extensions that this represents
    #[must_use]
    pub const fn set(&self) -> &HashSet<String> {
        &self.set
    }

    /// Returns `true` if `path`'s extension is contained within the set
    #[must_use]
    pub fn matches_path(&self, path: &Path) -> bool {
        path.extension()
            .and_then(OsStr::to_str)
            .is_some_and(|extension| self.contains(extension))
    }

    /// Returns `true` if `extension` is contained within the set
    #[must_use]
    pub fn contains(&self, extension: &str) -> bool {
        self.set.contains(extension)
    }
}

impl<S: AsRef<str>> From<&[S]> for ExtensionSet {
    fn from(slice: &[S]) -> Self {
        Self::new(
            slice
                .iter()
                .map(AsRef::as_ref)
                .map(ToString::to_string)
                .collect(),
        )
    }
}

impl BitOr<&ExtensionSet> for &ExtensionSet {
    type Output = ExtensionSet;

    fn bitor(self, rhs: &ExtensionSet) -> Self::Output {
        let set = &self.set | &rhs.set;
        ExtensionSet { set }
    }
}

pub(crate) fn frame_to_duration(frame: usize, sample_rate: usize) -> Duration {
    let secs = frame / sample_rate;
    let remaining = frame % sample_rate;
    let nanos = remaining * 1_000_000_000 / sample_rate;

    Duration::new(
        secs as u64,
        nanos
            .try_into()
            .expect("nanos should only ever be less than 1_000_000_000"),
    )
}

pub(crate) fn duration_to_frame(duration: Duration, sample_rate: usize) -> usize {
    let secs = duration.as_secs();
    let nanos = duration.subsec_nanos();

    let secs_frames = secs * sample_rate as u64;
    let nanos_frames = nanos as usize * sample_rate / 1_000_000_000;

    usize::try_from(secs_frames).expect("duration should fit within a usize") + nanos_frames
}
#[cfg(test)]
mod tests {
    use std::time::Duration;

    #[test]
    pub fn duration_to_frame() {
        let sample_rate = 44100;
        let frames = sample_rate * 2 + sample_rate / 2;
        let duration = Duration::from_secs_f64(2.5);
        assert_eq!(frames, super::duration_to_frame(duration, sample_rate));
    }

    #[test]
    pub fn frame_to_duration() {
        let sample_rate = 44100;
        let frames = sample_rate * 2 + sample_rate / 2;
        let duration = Duration::from_secs_f64(2.5);
        assert_eq!(duration, super::frame_to_duration(frames, sample_rate));
    }

    #[test]
    pub fn mixed() {
        let sample_rate = 44100;
        let original = sample_rate * 2 + sample_rate / 2;
        let duration = super::frame_to_duration(original, sample_rate);
        let result = super::duration_to_frame(duration, sample_rate);
        // there could be rounding errors, so give it some leeway
        assert!(result - original <= 1);
    }
}
