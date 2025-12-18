//! Utilities for decoding the metadata of audio files.

use std::{path::Path, str::Utf8Error};

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

    pub use super::data_iter::DataIterExt as _;
    pub use super::frame_iter::FrameIterExt as _;
    pub use gat_lending_iterator::LendingIterator as _;

    pub use gat_borrow::IntoOwnedImpl as _;
    pub use gat_borrow::ToRef as _;
}

pub mod implementations {
    pub mod flac;
    pub mod id3;
    pub mod m4a;
    pub mod ogg;

    pub use crate::decoder::symphonia;
}

pub mod data;
pub use data::{Data, DataCow, DataRef, Frame, FrameCow, FrameId, FrameRef};

pub mod data_iter;
pub mod frame_iter;

type DefaultTy = List<
    List<implementations::id3::Decoder, implementations::flac::Decoder>,
    List<
        implementations::m4a::Decoder,
        List<implementations::ogg::Decoder, super::symphonia::Symphonia>,
    >,
>;

/// A wrapper around the default [`Decoder`], see [`default`].
pub struct Default(DefaultTy);

/// A wrapper around the default [`Tag`], see [`default`].
pub struct DefaultTag(<DefaultTy as Decoder>::Tag);

impl Decoder for Default {
    type Tag = DefaultTag;

    fn read_fallible(&self, source: &MediaSource) -> MetadataResult<Option<Self::Tag>> {
        self.0.read_fallible(source).map(|opt| opt.map(DefaultTag))
    }

    fn read(&self, source: &MediaSource) -> MetadataResult<Self::Tag> {
        self.0.read(source).map(DefaultTag)
    }

    fn supported_extensions(&self) -> ExtensionSet {
        self.0.supported_extensions()
    }
}

impl Tag for DefaultTag {
    fn has(&self, id: FrameId) -> bool {
        self.0.has(id)
    }

    fn get(&self, id: FrameId) -> FrameOptCow<'_> {
        self.0.get(id)
    }

    fn get_all(&self, id: FrameId) -> impl Iterator<Item = FrameCow<'_>> {
        self.0.get_all(id)
    }

    fn replace(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        self.0.replace(id, data)
    }

    fn add(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        self.0.add(id, data)
    }

    fn remove(&mut self, id: FrameId) -> MetadataResult<()> {
        self.0.remove(id)
    }

    fn frames(&self) -> impl Iterator<Item = FrameCow<'_>> {
        self.0.frames()
    }

    fn save(&self, path: impl AsRef<Path>) -> MetadataResult<()> {
        self.0.save(path)
    }

    fn supports(&self, query: Operation) -> bool {
        self.0.supports(query)
    }
}

/// Get the default [`Decoder`]
///
/// Currently supported:
/// - `id3` through [`id3`],
/// - `flac` through [`metaflac`],
/// - [durations](FrameId::Duration) through [`symphonia`]
#[must_use]
pub fn default() -> self::Default {
    self::Default(List::new(
        List::new(
            implementations::id3::Decoder::new(),
            implementations::flac::Decoder::new(),
        ),
        List::new(
            implementations::m4a::Decoder::new(),
            List::new(
                implementations::ogg::Decoder::new(),
                super::symphonia::Symphonia::default(),
            ),
        ),
    ))
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

/// Represents an operation that can be performed on a [`Tag`]
#[derive(Debug, Clone)]
pub enum Operation {
    Get(FrameId),
    GetAll(FrameId),
    Replace(FrameId),
    Add(FrameId),
    Remove(FrameId),
    Data(DataType),
    Frames,
    Save,
}

impl Operation {
    #[must_use]
    pub const fn frame_id(&self) -> Option<&FrameId> {
        match self {
            Self::Get(id)
            | Self::GetAll(id)
            | Self::Add(id)
            | Self::Remove(id)
            | Self::Replace(id) => Some(id),
            _ => None,
        }
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

    /// Returns the [`Frame`] first frame with a specific [`FrameId`].
    ///
    /// This can be either a reference to the inner metadata (as a [`DataCow::Ref`]) or an owned
    /// object created from the inner metadata (as a [`DataCow::Owned`]).
    fn get(&self, id: FrameId) -> FrameOptCow<'_> {
        FrameOptCow::from_option(id.clone(), self.get_all(id).next())
    }

    /// Returns a iterator over all frames with a specific [`FrameId`].
    fn get_all(&self, id: FrameId) -> impl Iterator<Item = FrameCow<'_>> {
        self.frames().filter(move |frame| frame.id == id)
    }

    /// Replaces the [`Data`] for `id` with `data`. This removes any old data and adds `data` in
    /// its place.
    ///
    /// # Errors
    ///
    /// - [`MetadataError::InvalidDataType`] if `id` doesn't support `data`.
    fn replace(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        self.remove(id.clone())?;
        self.add(id, data)
    }

    /// Sets the [`Data`] for a specific [`FrameId`]. If the underlying metadata allows multiple
    /// frames with this id, this will add a frame without removing the rest. If it doesn't allow
    /// multiple frames, this will overwrite the previous frame.
    ///
    /// # Errors
    ///
    /// - [`MetadataError::InvalidDataType`] if `id` doesn't support `data`.
    /// - [`MetadataError::Unimplemented`] if the underlying tag doesn't support adding data
    fn add(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        let _ = data;
        Err(MetadataError::Unimplemented(Operation::Add(id)))
    }

    /// Removes all [`Data`] for a specific [`FrameId`].
    ///
    /// # Errors
    ///
    /// - [`MetadataError::Unimplemented`] if the underlying tag doesn't support removing data
    fn remove(&mut self, id: FrameId) -> MetadataResult<()> {
        Err(MetadataError::Unimplemented(Operation::Remove(id)))
    }

    /// Returns an iterator over the [`Frame`]s of this metadata.
    fn frames(&self) -> impl Iterator<Item = FrameCow<'_>>;

    /// Saves this tag to the file at `path`.
    ///
    /// # Errors
    ///
    /// - Any backend-specific errors.
    /// - [`MetadataError::Unimplemented`] if the underlying tag doesn't support saving
    fn save(&self, path: impl AsRef<Path>) -> MetadataResult<()> {
        let _ = path;
        Err(MetadataError::Unimplemented(Operation::Save))
    }

    /// Returns `true` if this [`Tag`] may support the operation of `query`.
    ///
    /// This should not actually perform the query itself, it should only return `false` if there
    /// is no chance of the tag supporting the query.
    fn supports(&self, query: Operation) -> bool;
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
    fn dyn_get(&self, id: FrameId) -> FrameOptCow<'_>;

    /// Returns an iterator over the [`Data`] of all frames with a specific [`FrameId`].
    fn dyn_get_all(&self, id: FrameId) -> Box<dyn Iterator<Item = FrameCow<'_>> + '_>;

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
    ///
    /// # Errors
    ///
    /// - [`MetadataError::Unimplemented`] if the underlying tag doesn't support removing data
    fn dyn_remove(&mut self, id: FrameId) -> MetadataResult<()>;

    /// Returns an iterator over the [`Frame`]s of this metadata.
    fn dyn_frames(&self) -> Box<dyn Iterator<Item = FrameCow<'_>> + '_>;

    /// Saves this tag to the file at `path`.
    ///
    /// # Errors
    ///
    /// - Any backend-specific errors.
    fn dyn_save(&self, path: &Path) -> MetadataResult<()>;

    /// Returns `true` if this [`Tag`] may support the operation of `query`.
    ///
    /// This should not actually perform the query itself, it should only return `false` if there
    /// is no chance of the tag supporting the query.
    fn dyn_supports(&self, query: Operation) -> bool;
}

impl<T: Tag> DynTag for T {
    fn dyn_has(&self, id: FrameId) -> bool {
        <Self as Tag>::has(self, id)
    }

    fn dyn_get(&self, id: FrameId) -> FrameOptCow<'_> {
        <Self as Tag>::get(self, id)
    }

    fn dyn_get_all(&self, id: FrameId) -> Box<dyn Iterator<Item = FrameCow<'_>> + '_> {
        let iter = <Self as Tag>::get_all(self, id);
        Box::new(iter)
    }

    fn dyn_replace(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        <Self as Tag>::replace(self, id, data)
    }

    fn dyn_add(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        <Self as Tag>::add(self, id, data)
    }

    fn dyn_remove(&mut self, id: FrameId) -> MetadataResult<()> {
        <Self as Tag>::remove(self, id)
    }

    fn dyn_frames(&self) -> Box<dyn Iterator<Item = FrameCow<'_>> + '_> {
        let iter = <Self as Tag>::frames(self);
        Box::new(iter)
    }

    fn dyn_save(&self, path: &Path) -> MetadataResult<()> {
        <Self as Tag>::save(self, path)
    }

    fn dyn_supports(&self, query: Operation) -> bool {
        <Self as Tag>::supports(self, query)
    }
}

impl Tag for dyn DynTag {
    fn has(&self, id: FrameId) -> bool {
        self.dyn_has(id)
    }

    fn get(&self, id: FrameId) -> FrameOptCow<'_> {
        self.dyn_get(id)
    }

    fn get_all(&self, id: FrameId) -> impl Iterator<Item = FrameCow<'_>> {
        self.dyn_get_all(id)
    }

    fn replace(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        self.dyn_replace(id, data)
    }

    fn add(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        self.dyn_add(id, data)
    }

    fn remove(&mut self, id: FrameId) -> MetadataResult<()> {
        self.dyn_remove(id)
    }

    fn frames(&self) -> impl Iterator<Item = FrameCow<'_>> {
        self.dyn_frames()
    }

    fn save(&self, path: impl AsRef<Path>) -> MetadataResult<()> {
        self.dyn_save(path.as_ref())
    }

    fn supports(&self, query: Operation) -> bool {
        self.dyn_supports(query)
    }
}

// the compiler would use the `impl DynTag for T: Tag` implementation instead,
// leading to a stack overflow
#[allow(clippy::needless_borrow)]
impl Tag for Box<dyn DynTag> {
    fn has(&self, id: FrameId) -> bool {
        (&**self).dyn_has(id)
    }

    fn get(&self, id: FrameId) -> FrameOptCow<'_> {
        (&**self).dyn_get(id)
    }

    fn get_all(&self, id: FrameId) -> impl Iterator<Item = FrameCow<'_>> {
        (&**self).dyn_get_all(id)
    }

    fn replace(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        (&mut **self).dyn_replace(id, data)
    }

    fn add(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        (&mut **self).dyn_add(id, data)
    }

    fn remove(&mut self, id: FrameId) -> MetadataResult<()> {
        (&mut **self).dyn_remove(id)
    }

    fn frames(&self) -> impl Iterator<Item = FrameCow<'_>> {
        (&**self).dyn_frames()
    }

    fn save(&self, path: impl AsRef<Path>) -> MetadataResult<()> {
        (&**self).dyn_save(path.as_ref())
    }

    fn supports(&self, query: Operation) -> bool {
        (&**self).dyn_supports(query)
    }
}

/// A combination of two [`Decoder`]s.
pub struct List<C: Decoder, N: Decoder> {
    current: C,
    next: N,
}

impl<C: Decoder, N: Decoder> List<C, N> {
    /// Create a new [`List`] between two decoders, giving `priority` the priority in methods if necessary.
    pub const fn new(priority: C, rest: N) -> Self {
        Self {
            current: priority,
            next: rest,
        }
    }

    /// Add `other` to the list, giving it the least priority.
    pub fn then<O: Decoder>(self, other: O) -> List<C, List<N, O>> {
        List {
            current: self.current,
            next: List {
                current: self.next,
                next: other,
            },
        }
    }
}

impl<C: Decoder, N: Decoder> Decoder for List<C, N> {
    type Tag = TagList<C::Tag, N::Tag>;

    fn read(&self, source: &MediaSource) -> MetadataResult<Self::Tag> {
        let current_read = self.current.read(source);
        let next_read = self.next.read(source);
        match (current_read, next_read) {
            (Err(current_err), Err(next_err)) => Err(MetadataError::List(
                Box::new(current_err),
                Box::new(next_err),
            )),
            (current_read, next_read) => Ok(TagList {
                current: current_read.ok(),
                next: next_read.ok(),
            }),
        }
    }

    fn supported_extensions(&self) -> ExtensionSet {
        &self.current.supported_extensions() | &self.next.supported_extensions()
    }
}

enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<I, L: Iterator<Item = I>, R: Iterator<Item = I>> Iterator for Either<L, R> {
    type Item = I;

    fn next(&mut self) -> Option<I> {
        match self {
            Self::Left(left) => left.next(),
            Self::Right(right) => right.next(),
        }
    }
}

/// The [`Tag`] for [`List`].
pub struct TagList<C: Tag, N: Tag> {
    current: Option<C>,
    next: Option<N>,
}

impl<C: Tag, N: Tag> Tag for TagList<C, N> {
    fn has(&self, id: FrameId) -> bool {
        self.current
            .as_ref()
            .is_some_and(|current| current.has(id.clone()))
            || self.next.as_ref().is_some_and(|next| next.has(id))
    }

    fn get(&self, id: FrameId) -> FrameOptCow<'_> {
        FrameOptCow::from_option(
            id.clone(),
            (self
                .current
                .as_ref()
                .and_then(|current| current.get(id.clone()).into_option()))
            .or_else(|| {
                self.next
                    .as_ref()
                    .and_then(|next| next.get(id).into_option())
            }),
        )
    }

    fn get_all(&self, id: FrameId) -> impl Iterator<Item = FrameCow<'_>> {
        if let Some(current) = self.current.as_ref() {
            if current.supports(Operation::GetAll(id.clone())) {
                return Either::Left(current.get_all(id));
            }
        }

        if let Some(next) = self.next.as_ref() {
            if next.supports(Operation::GetAll(id.clone())) {
                return Either::Right(Either::Left(next.get_all(id)));
            }
        }

        Either::Right(Either::Right(std::iter::empty()))
    }

    fn replace(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        let Some(data_type) = data.data_type() else {
            return Err(MetadataError::InvalidDataType {
                id,
                reason: Some("cannot replace with Unsupported data".to_string()),
                recovered_data: Box::new(data.into()),
            });
        };

        match (
            self.current.as_mut().filter(|current| {
                current.supports(Operation::Replace(id.clone()))
                    && current.supports(Operation::Data(data_type))
            }),
            self.next.as_mut().filter(|next| {
                next.supports(Operation::Replace(id.clone()))
                    && next.supports(Operation::Data(data_type))
            }),
        ) {
            (Some(current), Some(next)) => {
                current.replace(id.clone(), data.clone())?;
                next.replace(id, data)?;
                Ok(())
            }
            (Some(current), None) => {
                current.replace(id, data)?;
                Ok(())
            }
            (None, Some(next)) => {
                next.replace(id, data)?;
                Ok(())
            }
            (None, None) => Err(MetadataError::Unimplemented(Operation::Replace(id))),
        }
    }

    fn add(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        let Some(data_type) = data.data_type() else {
            return Err(MetadataError::InvalidDataType {
                id,
                reason: Some("cannot add Unsupported data".to_string()),
                recovered_data: Box::new(data.into()),
            });
        };

        match (
            self.current.as_mut().filter(|current| {
                current.supports(Operation::Add(id.clone()))
                    && current.supports(Operation::Data(data_type))
            }),
            self.next.as_mut().filter(|next| {
                next.supports(Operation::Add(id.clone()))
                    && next.supports(Operation::Data(data_type))
            }),
        ) {
            (Some(current), Some(next)) => {
                current.add(id.clone(), data.clone())?;
                next.add(id, data)?;
                Ok(())
            }
            (Some(current), None) => {
                current.add(id, data)?;
                Ok(())
            }
            (None, Some(next)) => {
                next.add(id, data)?;
                Ok(())
            }
            (None, None) => Err(MetadataError::Unimplemented(Operation::Add(id))),
        }
    }

    fn remove(&mut self, id: FrameId) -> MetadataResult<()> {
        let mut supports = false;

        if let Some(current) = self.current.as_mut() {
            if current.supports(Operation::Remove(id.clone())) {
                current.remove(id.clone())?;
                supports = true;
            }
        }

        if let Some(next) = self.next.as_mut() {
            if next.supports(Operation::Remove(id.clone())) {
                next.remove(id.clone())?;
                supports = true;
            }
        }

        if supports {
            Ok(())
        } else {
            Err(MetadataError::Unimplemented(Operation::Remove(id)))
        }
    }

    fn frames(&self) -> impl Iterator<Item = FrameCow<'_>> {
        (self.current.as_ref().into_iter().flat_map(Tag::frames))
            .chain(self.next.as_ref().into_iter().flat_map(Tag::frames))
    }

    fn save(&self, path: impl AsRef<Path>) -> MetadataResult<()> {
        let path_ref = path.as_ref();

        let mut supports = false;

        if let Some(current) = self.current.as_ref() {
            if current.supports(Operation::Save) {
                current.save(path_ref)?;
                supports = true;
            }
        }

        if let Some(next) = self.next.as_ref() {
            if next.supports(Operation::Save) {
                next.save(path_ref)?;
                supports = true;
            }
        }

        if supports {
            Ok(())
        } else {
            Err(MetadataError::Unimplemented(Operation::Save))
        }
    }

    fn supports(&self, query: Operation) -> bool {
        self.current
            .as_ref()
            .is_some_and(|current| current.supports(query.clone()))
            || self.next.as_ref().is_some_and(|next| next.supports(query))
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
    #[error("malformed data in {source}: {}", reason.as_deref().unwrap_or("unknown"))]
    MalformedData {
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
    #[error("unimplemented operation: {:?}", .0)]
    Unimplemented(Operation),
    #[error(
        "expected {expected:?} in {}, {}",
        id.as_ref().map_or_else(|| "frame".to_owned(), |id| format!("frame {id:?}")),
        found.as_ref().map_or_else(|| "was empty".to_owned(), |ty| format!("{ty:?}"))
    )]
    ExpectedData {
        id: Option<FrameId>,
        expected: DataType,
        found: Option<DataType>,
    },
    #[error("expected valid utf8 for FrameId::Unknown, found {}", String::from_utf8_lossy(&unknown.0))]
    UnknownInvalidUtf8 { err: Utf8Error, unknown: UnknownId },
    #[error("expected Data that is not Unsupported, found: {found:?}")]
    AddUnsupported { id: FrameId, found: Data },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{:?}\n{:?}", .0, .1)]
    List(Box<Self>, Box<Self>),
    #[error("decoder found error: {}", .0.as_deref().unwrap_or("unknown"))]
    Other(Option<String>),
}
