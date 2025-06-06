//! Utilities for decoding the metadata of audio files.

use std::path::Path;

use prelude::*;
use thiserror::Error;

/// Useful types for interacting with metadata decoders.
pub mod prelude {
    pub use super::super::{ExtensionSet, MetadataDecoder};
    pub use super::data::*;
    pub use super::{FrameId, MetadataError, MetadataResult};
    pub use crate::data::prelude::*;

    pub use super::Decoder as _;
    pub use super::Tag as _;
}

pub mod implementations {
    pub mod id3;
}

pub mod data;
pub use data::{Data, DataCow, DataRef, Frame, FrameCow, FrameId, FrameRef};

/// The output type of [`default`]
pub type Default = implementations::id3::Decoder;

/// Get the default [`Decoder`]
#[must_use]
pub fn default() -> self::Default {
    implementations::id3::Decoder::new()
}

/// A type that can [`read`](Self::read) and decode the metadata of a [`MediaSource`], returning a
/// [`Tag`].
///
/// For an object-safe version of this trait, see [`DynDecoder`], which is implemented for all types
/// that this is implemented for.
///
/// # Implementors
///
/// [`Self::read`] and [`Self::read_fallible`] are reflexively defined in terms of each other. It's
/// expected for the implementor to implement one of them and let the default implementation handle
/// the other.
pub trait Decoder: Send + 'static {
    /// The concrete [`Tag`] that this decoder will return.
    type Tag: Tag;

    /// Try to decode and read this file, returning `Ok(None)` if the format isn't supported
    ///
    /// # Errors
    ///
    /// - If there is some error with IO
    /// - If there is a backend-specific error
    fn read_fallible(&self, source: &MediaSource) -> MetadataResult<Option<Self::Tag>> {
        let res = self.read(source);
        if matches!(res, Err(MetadataError::UnsupportedFormat { .. })) {
            Ok(None)
        } else {
            res.map(Some)
        }
    }

    /// Try to decode and read this file, returning `Err(UnsupportedFormat)` if the format isn't supported
    ///
    /// # Errors
    ///
    /// - If the format isn't supported
    /// - If there is an error with IO
    /// - If there is a backend-specific error
    fn read(&self, source: &MediaSource) -> MetadataResult<Self::Tag> {
        self.read_fallible(source)
            .transpose()
            .unwrap_or(Err(MetadataError::UnsupportedFormat {
                source: source.into(),
                reason: None,
            }))
    }

    /// Get a [set](ExtensionSet) of all file extensions that this decoder can possibly decode.
    ///
    /// This does not mean that the decoder must be able to decode every file with this extension -
    /// only that it may be able to.
    fn supported_extensions(&self) -> ExtensionSet;
}

/// A object safe version of [`Decoder`] - a trait that can read the metadata of files.
///
/// This is implemented for all types that have implementations for [`Decoder`] by wrapping their
/// concrete tag types into [`DynTag`] objects. Then, [`Decoder`] is implemented for both
/// `Box<dyn DynDecoder>` and `dyn DynDecoder`, so they can be used natively as decoders.
pub trait DynDecoder: Send + 'static {
    /// Try to decode and read this file, returning `Ok(None)` if the format isn't supported
    ///
    /// # Errors
    ///
    /// - If there is some error with IO
    /// - If there is a backend-specific error
    fn dyn_read_fallible(&self, source: &MediaSource) -> MetadataResult<Option<Box<dyn DynTag>>>;

    /// Try to decode and read this file, returning `Err(UnsupportedFormat)` if the format isn't supported
    ///
    /// # Errors
    ///
    /// - If the format isn't supported
    /// - If there is an error with IO
    /// - If there is a backend-specific error
    fn dyn_read(&self, source: &MediaSource) -> MetadataResult<Box<dyn DynTag>>;

    /// Get a [set](ExtensionSet) of all file extensions that this decoder can possibly decode.
    ///
    /// This does not mean that the decoder must be able to decode every file with this extension -
    /// only that it may be able to.
    fn dyn_supported_extensions(&self) -> ExtensionSet;
}

impl<D: Decoder> DynDecoder for D {
    fn dyn_read_fallible(&self, source: &MediaSource) -> MetadataResult<Option<Box<dyn DynTag>>> {
        <Self as Decoder>::read_fallible(self, source)
            .map(|opt| opt.map(|tag| Box::new(tag) as Box<dyn DynTag>))
    }

    fn dyn_read(&self, source: &MediaSource) -> MetadataResult<Box<dyn DynTag>> {
        <Self as Decoder>::read(self, source).map(|tag| Box::new(tag) as Box<dyn DynTag>)
    }

    fn dyn_supported_extensions(&self) -> ExtensionSet {
        <Self as Decoder>::supported_extensions(self)
    }
}

impl Decoder for dyn DynDecoder {
    type Tag = Box<dyn DynTag>;

    fn read_fallible(&self, source: &MediaSource) -> MetadataResult<Option<Self::Tag>> {
        self.dyn_read_fallible(source)
    }

    fn read(&self, source: &MediaSource) -> MetadataResult<Self::Tag> {
        self.dyn_read(source)
    }

    fn supported_extensions(&self) -> ExtensionSet {
        self.dyn_supported_extensions()
    }
}

// the compiler would use the `impl DynDecoder for D: Decoder` implementation instead,
// leading to a stack overflow
#[allow(clippy::needless_borrow)]
impl Decoder for Box<dyn DynDecoder> {
    type Tag = Box<dyn DynTag>;

    fn read_fallible(&self, source: &MediaSource) -> MetadataResult<Option<Self::Tag>> {
        (&**self).dyn_read_fallible(source)
    }

    fn read(&self, source: &MediaSource) -> MetadataResult<Self::Tag> {
        (&**self).dyn_read(source)
    }

    fn supported_extensions(&self) -> ExtensionSet {
        (&**self).dyn_supported_extensions()
    }
}

/// A type that represents the metadata of a file, allowing for both reading and writing.
///
/// For an object-safe version of this trait, see [`DynTag`], which is implemented for all types
/// that this is implemented for.
pub trait Tag {
    /// Returns `true` if this tag contains corresponding [`Data`] for a specific [`FrameId`].
    fn has(&self, id: FrameId) -> bool {
        self.get(id).is_some()
    }

    /// Returns the [`Data`] of the first frame with a specific [`FrameId`].
    ///
    /// This can be either a reference to the inner metadata (as a [`DataCow::Ref`]) or an owned
    /// object created from the inner metadata (as a [`DataCow::Owned`]).
    fn get(&self, id: FrameId) -> DataOptCow<'_> {
        DataOptCow::from_option(self.get_all(id).next())
    }

    /// Returns a iterator over the [`Data`] of all frames with a specific [`FrameId`].
    fn get_all(&self, id: FrameId) -> impl Iterator<Item = DataCow>;

    /// Replaces the [`Data`] for `id` with `data`. This removes any old data and adds `data` in
    /// its place.
    ///
    /// # Errors
    ///
    /// - [`MetadataError::InvalidDataType`] if `id` doesn't support `data`.
    fn replace(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        self.remove(id.clone());
        self.add(id, data)
    }

    /// Sets the [`Data`] for a specific [`FrameId`]. If the underlying metadata allows multiple
    /// frames with this id, this will add a frame without removing the rest. If it doesn't allow
    /// multiple frames, this will overwrite the previous frame.
    ///
    /// # Errors
    ///
    /// - [`MetadataError::InvalidDataType`] if `id` doesn't support `data`.
    fn add(&mut self, id: FrameId, data: Data) -> MetadataResult<()>;

    /// Removes all [`Data`] for a specific [`FrameId`].
    fn remove(&mut self, id: FrameId);

    /// Returns an iterator over the [`Frame`]s of this metadata.
    fn frames(&self) -> impl Iterator<Item = FrameCow>;

    /// Saves this tag to the file at `path`.
    ///
    /// # Errors
    ///
    /// - Any backend-specific errors.
    fn save(&self, path: impl AsRef<Path>) -> MetadataResult<()>;
}

/// A object safe version of [`Tag`] - a trait that represents the read metadata of a file.
///
/// This is implemented for all types that have implementations for [`Tag`] by wrapping their
/// iterators into objects. Then, [`Tag`] is implemented for both `Box<dyn DynTag>` and
/// `dyn DynTag`, so they can be used natively as tags.
pub trait DynTag {
    /// Returns `true` if this tag contains corresponding [`Data`] for a specific [`FrameId`].
    fn dyn_has(&self, id: FrameId) -> bool;

    /// Returns the [`Data`] of the first frame with a specific [`FrameId`].
    ///
    /// This can be either a reference to the inner metadata (as a [`DataCow::Ref`]) or an owned
    /// object created from the inner metadata (as a [`DataCow::Owned`]).
    fn dyn_get(&self, id: FrameId) -> DataOptCow<'_>;

    /// Returns an iterator over the [`Data`] of all frames with a specific [`FrameId`].
    fn dyn_get_all(&self, id: FrameId) -> Box<dyn Iterator<Item = DataCow> + '_>;

    /// Replaces the [`Data`] for `id` with `data`. This removes any old data and adds `data` in
    /// its place.
    ///
    /// # Errors
    ///
    /// - [`MetadataError::InvalidDataType`] if `id` doesn't support `data`.
    fn dyn_replace(&mut self, id: FrameId, data: Data) -> MetadataResult<()>;

    /// Sets the [`Data`] for a specific [`FrameId`]. If the underlying metadata allows multiple
    /// frames with this id, this will add a frame without removing the rest. If it doesn't allow
    /// multiple frames, this may overwrite the previous frame.
    ///
    /// # Errors
    ///
    /// - [`MetadataError::InvalidDataType`] if `id` doesn't support `data`.
    fn dyn_add(&mut self, id: FrameId, data: Data) -> MetadataResult<()>;

    /// Removes all [`Data`] for a specific [`FrameId`].
    fn dyn_remove(&mut self, id: FrameId);

    /// Returns an iterator over the [`Frame`]s of this metadata.
    fn dyn_frames(&self) -> Box<dyn Iterator<Item = FrameCow> + '_>;

    /// Saves this tag to the file at `path`.
    ///
    /// # Errors
    ///
    /// - Any backend-specific errors.
    fn dyn_save(&self, path: &Path) -> MetadataResult<()>;
}

impl<T: Tag> DynTag for T {
    fn dyn_has(&self, id: FrameId) -> bool {
        <Self as Tag>::has(self, id)
    }

    fn dyn_get(&self, id: FrameId) -> DataOptCow<'_> {
        <Self as Tag>::get(self, id)
    }

    fn dyn_get_all(&self, id: FrameId) -> Box<dyn Iterator<Item = DataCow> + '_> {
        let iter = <Self as Tag>::get_all(self, id);
        Box::new(iter)
    }

    fn dyn_replace(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        <Self as Tag>::replace(self, id, data)
    }

    fn dyn_add(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        <Self as Tag>::add(self, id, data)
    }

    fn dyn_remove(&mut self, id: FrameId) {
        <Self as Tag>::remove(self, id);
    }

    fn dyn_frames(&self) -> Box<dyn Iterator<Item = FrameCow> + '_> {
        let iter = <Self as Tag>::frames(self);
        Box::new(iter)
    }

    fn dyn_save(&self, path: &Path) -> MetadataResult<()> {
        <Self as Tag>::save(self, path)
    }
}

impl Tag for dyn DynTag {
    fn has(&self, id: FrameId) -> bool {
        self.dyn_has(id)
    }

    fn get(&self, id: FrameId) -> DataOptCow<'_> {
        self.dyn_get(id)
    }

    fn get_all(&self, id: FrameId) -> impl Iterator<Item = DataCow> {
        self.dyn_get_all(id)
    }

    fn replace(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        self.dyn_replace(id, data)
    }

    fn add(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        self.dyn_add(id, data)
    }

    fn remove(&mut self, id: FrameId) {
        self.dyn_remove(id);
    }

    fn frames(&self) -> impl Iterator<Item = FrameCow> {
        self.dyn_frames()
    }

    fn save(&self, path: impl AsRef<Path>) -> MetadataResult<()> {
        self.dyn_save(path.as_ref())
    }
}

// the compiler would use the `impl DynTag for T: Tag` implementation instead,
// leading to a stack overflow
#[allow(clippy::needless_borrow)]
impl Tag for Box<dyn DynTag> {
    fn has(&self, id: FrameId) -> bool {
        (&**self).dyn_has(id)
    }

    fn get(&self, id: FrameId) -> DataOptCow<'_> {
        (&**self).dyn_get(id)
    }

    fn get_all(&self, id: FrameId) -> impl Iterator<Item = DataCow> {
        (&**self).dyn_get_all(id)
    }

    fn replace(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        (&mut **self).dyn_replace(id, data)
    }

    fn add(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        (&mut **self).dyn_add(id, data)
    }

    fn remove(&mut self, id: FrameId) {
        (&mut **self).dyn_remove(id);
    }

    fn frames(&self) -> impl Iterator<Item = FrameCow> {
        (&**self).dyn_frames()
    }

    fn save(&self, path: impl AsRef<Path>) -> MetadataResult<()> {
        (&**self).dyn_save(path.as_ref())
    }
}

/// A result of an operation on a [`Decoder`]
pub type MetadataResult<T> = Result<T, MetadataError>;

/// Any error that can be occurred while decoding metadata
#[derive(Error, Debug)]
pub enum MetadataError {
    #[error(
        "format of given {source} is not supported{}",
        reason.as_ref().map_or(
            String::new(),
            |reason| format!(": {reason}")
        ),
    )]
    UnsupportedFormat {
        source: SourceName,
        reason: Option<String>,
    },
    #[error(
        "frame {id:?} given invalid data{}{}",
        reason.as_ref().map_or(
            String::new(),
            |reason| format!(": {reason}")
        ),
        recovered_data.as_option().map_or(
            String::new(),
            |recovered_data| format!(" (recovered: {recovered_data:?})")
        ),
    )]
    InvalidDataType {
        id: FrameId,
        reason: Option<String>,
        recovered_data: Box<DataOpt>,
    },
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("decoder found error: {}", .0.as_deref().unwrap_or("unknown"))]
    Other(Option<String>),
}
