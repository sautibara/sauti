//! Different types of metadata that can be decoded from an audio file.

use std::{fmt::Debug, str::Utf8Error, sync::Arc, time::Duration};

use gat_borrow::{IntoOwnedImpl, Reborrow, ToRef};

use sealed::Sealed;

use crate::decoder::metadata::MetadataResult;

use super::MetadataError;
mod sealed {
    pub trait Sealed {}
}

/// Any possible key for a [`Frame`] of metadata in a [`Tag`](super::Tag).
///
/// This type can be expected to be cheaply cloneable ("claimable"), using reference counting if
/// necessary.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FrameId {
    /// The text title of the song ([`DataType::Text`]).
    Title,
    /// The album that the song is in ([`DataType::Text`]).
    Album,
    /// The artist, including featured artists, of the specific song ([`DataType::Text`]).
    Artist,
    /// The artist of the entire album ([`DataType::Text`]).
    AlbumArtist,
    /// The duration of the media file ([`DataType::Duration`]).
    Duration,
    /// The list of everyone involved with the creation of the song as well as their roles - see
    /// [`InvolvedPeople`] ([`DataType::InvolvedPeople`]).
    InvolvedPeople,
    /// A specific picture associated with the song, such as the album art ([`DataType::Picture`]).
    Picture(PictureType),
    /// An application-specific binary object, identified by a key ([`DataType::Object`]).
    CustomObject(Arc<str>),
    /// An application-specific string, identified by a key ([`DataType::Text`]).
    CustomText(Arc<str>),
    /// An application-specific link, identified by a key ([`DataType::Link`]).
    CustomLink(Arc<str>),
    /// An unknown id, specific to the background implementation. For example, this will likely be
    /// four letters for an id3 id (arbitrary datatype).
    Unknown(UnknownId),
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct UnknownId(pub Arc<[u8]>);

impl UnknownId {
    fn error(&self, err: Utf8Error) -> MetadataError {
        MetadataError::UnknownInvalidUtf8 {
            err,
            unknown: self.clone(),
        }
    }

    /// Attempts to convert the byte slice into utf8, returning [`Ok`] if the conversion succeeded.
    ///
    /// # Errors
    ///
    /// - [`MetadataError::UnknownInvalidUtf8`] - If the byte slice is not valid utf8.
    pub fn as_utf8(&self) -> MetadataResult<&str> {
        str::from_utf8(&self.0).map_err(|err| self.error(err))
    }

    /// Attempts to convert the byte slice into an owned utf8 [`String`], returning [`Ok`] if the
    /// conversion succeeded.
    ///
    /// # Errors
    ///
    /// - [`MetadataError::UnknownInvalidUtf8`] - If the byte slice is not valid utf8.
    pub fn into_utf8(self) -> MetadataResult<String> {
        String::from_utf8(self.0.to_vec())
            .map_err(|err| err.utf8_error())
            .map_err(|err| self.error(err))
    }
}

impl From<Arc<[u8]>> for UnknownId {
    fn from(value: Arc<[u8]>) -> Self {
        Self(value)
    }
}

impl<'a> From<&'a [u8]> for UnknownId {
    fn from(value: &'a [u8]) -> Self {
        Self(value.into())
    }
}

impl FromIterator<u8> for UnknownId {
    fn from_iter<T: IntoIterator<Item = u8>>(iter: T) -> Self {
        Self(Arc::from_iter(iter))
    }
}

// Taken from crate `byte_string`
impl Debug for UnknownId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "b\"")?;

        for byte in self.0.iter() {
            for ch in std::ascii::escape_default(*byte) {
                write!(f, "{}", ch as char)?;
            }
        }

        write!(f, "\"")
    }
}

/// A generalized implementation for all [`FrameId`] - [`Data`] pairs - see [`Frame`], [`FrameRef`],
/// [`FrameCow`], [`FrameOpt`], [`FrameOptRef`], and [`FrameOptCow`].
///
/// This implements [`DataLike`], so you can access the data directly.
#[derive(Debug, Clone)]
pub struct AnyFrame<AnyData: DataLike> {
    pub id: FrameId,
    pub data: AnyData,
}

/// Implementations for [`Frame`], [`FrameRef`], and [`FrameCow`].
///
/// Not shown: [`ToRef::to_ref`], which takes `&Self` and returns [`FrameRef`].
impl<AnyDataSome: DataSomeLike> AnyFrame<AnyDataSome> {
    /// Obtain an owned [`Frame`] from this, potentially by cloning.
    pub fn into_owned(self) -> Frame {
        Frame {
            id: self.id,
            data: self.data.into_owned(),
        }
    }
}

impl<'a, AnyDataSome: DataSomeLike> ToRef<'a, FrameRef<'a>> for AnyFrame<AnyDataSome> {
    fn to_ref(&'a self) -> FrameRef<'a> {
        FrameRef {
            id: self.id.clone(),
            data: self.data.to_ref(),
        }
    }
}

/// Implementations for [`FrameOpt`], [`FrameOptRef`], and [`FrameOptCow`]
///
/// Not shown: [`ToRef::to_ref`], which takes `&Self` and returns [`FrameOptRef`].
impl<AnyDataOpt: DataOptLike> AnyFrame<AnyDataOpt> {
    /// Create a new present optional frame from `frame`.
    #[must_use]
    pub fn some(frame: AnyFrame<AnyDataOpt::Some>) -> Self {
        Self {
            id: frame.id,
            data: AnyDataOpt::some(frame.data),
        }
    }

    /// Create a new empty optional frame with a [`FrameId`] of `id`.
    #[must_use]
    pub fn none(id: FrameId) -> Self {
        Self {
            id,
            data: AnyDataOpt::none(),
        }
    }

    /// Returns `true` if this has present [data](DataLike), or `false` if this has no data.
    #[must_use]
    pub fn is_some(&self) -> bool {
        self.data.is_some()
    }

    /// Returns `true` if this has no [data](DataLike), or `false` if this has present data.
    #[must_use]
    pub fn is_none(&self) -> bool {
        self.data.is_none()
    }

    /// Creates a new optional frame from an [`Option`] of the present frame type and a [`FrameId`]
    /// for if the option is [`None`].
    ///
    /// See [`DataOptLike::from_option`].
    #[must_use]
    pub fn from_option(id: FrameId, opt: Option<AnyFrame<AnyDataOpt::Some>>) -> Self {
        opt.map_or_else(|| Self::none(id), |frame| Self::some(frame))
    }

    /// Turn this optional frame into an [`Option`] of the present frame type.
    ///
    /// See [`DataOptLike::into_option`].
    #[must_use]
    pub fn into_option(self) -> Option<AnyFrame<AnyDataOpt::Some>> {
        let Self { id, data } = self;
        data.into_option().map(|data| AnyFrame { id, data })
    }

    /// Turn this into a [`FrameOpt`] by cloning if necessary.
    #[must_use]
    pub fn into_owned_opt(self) -> FrameOpt {
        FrameOpt {
            id: self.id,
            data: self.data.into_owned(),
        }
    }

    /// Returns `self` if [`Self::is_some`]. Otherwise, returns the output of `func`.
    #[must_use]
    pub fn or_else(self, func: impl FnOnce() -> Self) -> Self {
        if self.is_some() {
            self
        } else {
            (func)()
        }
    }
}

impl<'a, AnyDataOpt: DataOptLike> ToRef<'a, FrameOptRef<'a>> for AnyFrame<AnyDataOpt> {
    fn to_ref(&'a self) -> FrameOptRef<'a> {
        FrameOptRef {
            id: self.id.clone(),
            data: self.data.to_ref(),
        }
    }
}

/// An owned frame; a [`FrameId`]-[`Data`] pair.
///
/// This implements [`DataLike`], so you can access the data directly.
pub type Frame = AnyFrame<Data>;
/// A referenced frame; a [`FrameId`]-[`DataRef`] pair.
///
/// This implements [`DataLike`], so you can access the data directly.
pub type FrameRef<'a> = AnyFrame<DataRef<'a>>;
/// An owned or referenced frame; a [`FrameId`]-[`DataCow`] pair.
///
/// This implements [`DataLike`], so you can access the data directly.
pub type FrameCow<'a> = AnyFrame<DataCow<'a>>;
/// An optional, owned frame; a [`FrameId`]-[`DataOpt`] pair.
///
/// This implements [`DataLike`], so you can access the data directly.
pub type FrameOpt = AnyFrame<DataOpt>;
/// An optional, referenced frame; a [`FrameId`]-[`DataOptRef`] pair.
///
/// This implements [`DataLike`], so you can access the data directly.
pub type FrameOptRef<'a> = AnyFrame<DataOptRef<'a>>;
/// An optional, owned or referenced frame; a [`FrameId`]-[`DataOptCow`] pair.
///
/// This implements [`DataLike`], so you can access the data directly.
pub type FrameOptCow<'a> = AnyFrame<DataOptCow<'a>>;

impl<AnyData: DataLike> sealed::Sealed for AnyFrame<AnyData> {}

impl<AnyData: DataLike> DataLike for AnyFrame<AnyData> {
    fn as_string(&self) -> Option<&str> {
        self.data.as_string()
    }

    fn as_text(&self) -> Option<&str> {
        self.data.as_text()
    }

    fn as_link(&self) -> Option<&str> {
        self.data.as_link()
    }

    fn as_picture(&self) -> Option<PictureRef<'_>> {
        self.data.as_picture()
    }

    fn as_involved_people(&self) -> Option<InvolvedPeopleRef<'_>> {
        self.data.as_involved_people()
    }

    fn as_object(&self) -> Option<ObjectRef<'_>> {
        self.data.as_object()
    }

    fn as_duration(&self) -> Option<Duration> {
        self.data.as_duration()
    }

    fn expect_string(&self) -> MetadataResult<&str> {
        match self.data.expect_string() {
            Err(MetadataError::ExpectedData {
                expected, found, ..
            }) => Err(MetadataError::ExpectedData {
                id: Some(self.id.clone()),
                expected,
                found,
            }),
            other => other,
        }
    }

    fn expect_text(&self) -> MetadataResult<&str> {
        match self.data.expect_text() {
            Err(MetadataError::ExpectedData {
                expected, found, ..
            }) => Err(MetadataError::ExpectedData {
                id: Some(self.id.clone()),
                expected,
                found,
            }),
            other => other,
        }
    }

    fn expect_link(&self) -> MetadataResult<&str> {
        match self.data.expect_link() {
            Err(MetadataError::ExpectedData {
                expected, found, ..
            }) => Err(MetadataError::ExpectedData {
                id: Some(self.id.clone()),
                expected,
                found,
            }),
            other => other,
        }
    }

    fn expect_picture(&self) -> MetadataResult<PictureRef<'_>> {
        match self.data.expect_picture() {
            Err(MetadataError::ExpectedData {
                expected, found, ..
            }) => Err(MetadataError::ExpectedData {
                id: Some(self.id.clone()),
                expected,
                found,
            }),
            other => other,
        }
    }

    fn expect_involved_people(&self) -> MetadataResult<InvolvedPeopleRef<'_>> {
        match self.data.expect_involved_people() {
            Err(MetadataError::ExpectedData {
                expected, found, ..
            }) => Err(MetadataError::ExpectedData {
                id: Some(self.id.clone()),
                expected,
                found,
            }),
            other => other,
        }
    }

    fn expect_object(&self) -> MetadataResult<ObjectRef<'_>> {
        match self.data.expect_object() {
            Err(MetadataError::ExpectedData {
                expected, found, ..
            }) => Err(MetadataError::ExpectedData {
                id: Some(self.id.clone()),
                expected,
                found,
            }),
            other => other,
        }
    }

    fn expect_duration(&self) -> MetadataResult<Duration> {
        match self.data.expect_duration() {
            Err(MetadataError::ExpectedData {
                expected, found, ..
            }) => Err(MetadataError::ExpectedData {
                id: Some(self.id.clone()),
                expected,
                found,
            }),
            other => other,
        }
    }

    fn data_type(&self) -> Option<DataType> {
        self.data.data_type()
    }
}

impl From<Frame> for FrameCow<'_> {
    fn from(value: Frame) -> Self {
        Self {
            id: value.id,
            data: DataCow::Owned(value.data),
        }
    }
}

impl<'a> From<FrameRef<'a>> for FrameCow<'a> {
    fn from(value: FrameRef<'a>) -> Self {
        Self {
            id: value.id,
            data: DataCow::Ref(value.data),
        }
    }
}

impl From<FrameOpt> for FrameOptCow<'_> {
    fn from(value: FrameOpt) -> Self {
        Self {
            id: value.id,
            data: DataOptCow::from(value.data),
        }
    }
}

impl<'a> From<FrameOptRef<'a>> for FrameOptCow<'a> {
    fn from(value: FrameOptRef<'a>) -> Self {
        Self {
            id: value.id,
            data: DataOptCow::from(value.data),
        }
    }
}

/// An optional piece of metadata associated with a [`Tag`](super::Tag).
///
/// See [`DataOpt`], [`DataOptRef`], and [`DataOptCow`].
pub trait DataOptLike:
    Sealed + DataLike + From<Option<Self::Some>> + Into<Option<Self::Some>>
{
    type Some: DataSomeLike;

    /// Returns some new, empty data.
    fn none() -> Self {
        Self::from_option(None)
    }

    /// Returns some new optional data with a present value.
    fn some(val: impl Into<Self::Some>) -> Self {
        Self::from_option(Some(val.into()))
    }

    /// Returns a new [`DataOptLike`] from `option`.
    fn from_option(option: Option<Self::Some>) -> Self;

    /// Convert this optional data into an [`Option`].
    fn into_option(self) -> Option<Self::Some>;

    /// Convert a reference to this optional data into an [`Option`]al reference.
    fn as_option(&self) -> Option<&Self::Some>;

    /// Returns `true` if the underlying option is [`None`].
    #[must_use]
    fn is_none(&self) -> bool {
        self.as_option().is_none()
    }

    /// Returns `false` if the underlying option is [`None`].
    #[must_use]
    fn is_some(&self) -> bool {
        self.as_option().is_some()
    }

    /// Returns a [`DataOptRef`] pointing to this frame.
    #[must_use]
    fn to_ref(&self) -> DataOptRef<'_> {
        self.as_option().map(ToRef::to_ref).into()
    }

    /// Creates an owned [`DataOpt`] struct by allocating this frame if necessary.
    #[must_use]
    fn into_owned(self) -> DataOpt {
        self.into_option().map(DataSomeLike::into_owned).into()
    }

    /// Returns `self` if [`Self::is_some`]. Otherwise, calls `func` and returns its value.
    #[must_use]
    fn or_else(self, func: impl FnOnce() -> Self) -> Self {
        if self.is_some() {
            self
        } else {
            (func)()
        }
    }
}

/// A piece of metadata associated with a [`Tag`](super::Tag).
///
/// See [`Data`], [`DataRef`], and [`DataCow`].
pub trait DataSomeLike: Sealed + DataLike + for<'a> ToRef<'a, DataRef<'a>> {
    /// Creates an owned [`Data`] struct by allocating this data if necessary.
    #[must_use]
    fn into_owned(self) -> Data;
}

/// A potentially optional piece of metadata associated with a [`Tag`](super::Tag).
///
/// See [`Data`], [`DataRef`], [`DataCow`], [`DataOpt`], [`DataOptRef`], and [`DataOptCow`].
pub trait DataLike: Sealed + Sized {
    /// Takes a reference to the underlying [`Data::Text`] or [`Data::Link`] if this is one, or returns [`None`].
    #[must_use]
    fn as_string(&self) -> Option<&str>;

    /// Takes a reference to the underlying [`Data::Text`] if this is one, or returns [`None`].
    #[must_use]
    fn as_text(&self) -> Option<&str>;

    /// Takes a reference to the underlying [`Data::Link`] if this is one, or returns [`None`].
    #[must_use]
    fn as_link(&self) -> Option<&str>;

    /// Takes a reference to the underlying [`Data::Picture`] if this is one, or returns [`None`].
    #[must_use]
    fn as_picture(&self) -> Option<PictureRef<'_>>;

    /// Takes a reference to the underlying [`Data::InvolvedPeople`] if this is one, or returns [`None`].
    #[must_use]
    fn as_involved_people(&self) -> Option<InvolvedPeopleRef<'_>>;

    /// Takes a reference to the underlying [`Data::Object`] if this is one, or returns [`None`].
    #[must_use]
    fn as_object(&self) -> Option<ObjectRef<'_>>;

    /// Copies the underlying [`Data::Duration`] if this is one, or returns [`None`].
    #[must_use]
    fn as_duration(&self) -> Option<Duration>;

    /// Takes a reference to the underlying [`Data::Text`] or [`Data::Link`] if this is one, or returns [`Err`].
    ///
    /// # Errors
    ///
    /// - [`MetadataError::ExpectedData`] if the correct data type was not found.
    fn expect_string(&self) -> MetadataResult<&str> {
        self.as_string().ok_or_else(|| MetadataError::ExpectedData {
            id: None,
            expected: DataType::Text,
            found: self.data_type(),
        })
    }

    /// Takes a reference to the underlying [`Data::Text`] if this is one, or returns [`Err`].
    ///
    /// # Errors
    ///
    /// - [`MetadataError::ExpectedData`] if the correct data type was not found.
    fn expect_text(&self) -> MetadataResult<&str> {
        self.as_text().ok_or_else(|| MetadataError::ExpectedData {
            id: None,
            expected: DataType::Text,
            found: self.data_type(),
        })
    }

    /// Takes a reference to the underlying [`Data::Link`] if this is one, or returns [`Err`].
    ///
    /// # Errors
    ///
    /// - [`MetadataError::ExpectedData`] if the correct data type was not found.
    fn expect_link(&self) -> MetadataResult<&str> {
        self.as_link().ok_or_else(|| MetadataError::ExpectedData {
            id: None,
            expected: DataType::Link,
            found: self.data_type(),
        })
    }

    /// Takes a reference to the underlying [`Data::Picture`] if this is one, or returns [`Err`].
    ///
    /// # Errors
    ///
    /// - [`MetadataError::ExpectedData`] if the correct data type was not found.
    fn expect_picture(&self) -> MetadataResult<PictureRef<'_>> {
        self.as_picture()
            .ok_or_else(|| MetadataError::ExpectedData {
                id: None,
                expected: DataType::Picture,
                found: self.data_type(),
            })
    }

    /// Takes a reference to the underlying [`Data::InvolvedPeople`] if this is one, or returns [`Err`].
    ///
    /// # Errors
    ///
    /// - [`MetadataError::ExpectedData`] if the correct data type was not found.
    fn expect_involved_people(&self) -> MetadataResult<InvolvedPeopleRef<'_>> {
        self.as_involved_people()
            .ok_or_else(|| MetadataError::ExpectedData {
                id: None,
                expected: DataType::InvolvedPeople,
                found: self.data_type(),
            })
    }

    /// Takes a reference to the underlying [`Data::Object`] if this is one, or returns [`Err`].
    ///
    /// # Errors
    ///
    /// - [`MetadataError::ExpectedData`] if the correct data type was not found.
    fn expect_object(&self) -> MetadataResult<ObjectRef<'_>> {
        self.as_object().ok_or_else(|| MetadataError::ExpectedData {
            id: None,
            expected: DataType::Object,
            found: self.data_type(),
        })
    }

    /// Copies the underlying [`Data::Duration`] if this is one, or returns [`Err`].
    ///
    /// # Errors
    ///
    /// - [`MetadataError::ExpectedData`] if the correct data type was not found.
    fn expect_duration(&self) -> MetadataResult<Duration> {
        self.as_duration()
            .ok_or_else(|| MetadataError::ExpectedData {
                id: None,
                expected: DataType::Duration,
                found: self.data_type(),
            })
    }

    /// Gets the [`DataType`] of this data, or [`None`] if the data is
    /// [`Unsupported`](Data::Unsupported) or empty.
    #[must_use]
    fn data_type(&self) -> Option<DataType>;
}

macro_rules! data_opt {
    (from: $from:ident, to: $to:ident, docs: $docs:tt, lt: $($lt:tt)*) => {
        #[doc = concat!($docs, " piece of metadata associated with a [`Tag`](super::Tag).\n")]
        #[doc = "\n"]
        #[doc = concat!("See [`", stringify!($from), "`], [`DataLike`], and [`DataOptLike`].")]
        #[derive(Clone, Debug)]
        pub struct $to $($lt)* (pub Option<$from $($lt)*>);

        impl $($lt)* Sealed for $to $($lt)* {}

        impl $($lt)* DataOptLike for $to $($lt)* {
            type Some = $from $($lt)*;

            fn from_option(option: Option<$from $($lt)*>) -> Self {
                Self(option)
            }

            fn into_option(self) -> Option<$from $($lt)*> {
                self.0
            }

            fn as_option(&self) -> Option<&$from $($lt)*> {
                self.0.as_ref()
            }
        }

        impl $($lt)* DataLike for $to $($lt)* {
            fn as_string(&self) -> Option<&str> {
                self.as_option().and_then(|opt| opt.as_string())
            }

            fn as_text(&self) -> Option<&str> {
                self.as_option().and_then(|opt| opt.as_text())
            }

            fn as_link(&self) -> Option<&str> {
                self.as_option().and_then(|opt| opt.as_link())
            }

            fn as_picture(&self) -> Option<PictureRef<'_>> {
                self.as_option().and_then(|opt| opt.as_picture())
            }

            fn as_involved_people(&self) -> Option<InvolvedPeopleRef<'_>> {
                self.as_option().and_then(|opt| opt.as_involved_people())
            }

            fn as_object(&self) -> Option<ObjectRef<'_>> {
                self.as_option().and_then(|opt| opt.as_object())
            }

            fn as_duration(&self) -> Option<Duration> {
                self.as_option().and_then(|opt| opt.as_duration())
            }

            fn data_type(&self) -> Option<DataType> {
                self.as_option().and_then(|opt| opt.data_type())
            }
        }

        impl $($lt)* From<$from $($lt)*> for $to $($lt)* {
            fn from(val: $from $($lt)*) -> Self {
                Self::from_option(Some(val))
            }
        }

        impl $($lt)* From<Option<$from $($lt)*>> for $to $($lt)* {
            fn from(val: Option<$from $($lt)*>) -> Self {
                Self::from_option(val)
            }
        }

        impl $($lt)* From<$to $($lt)*> for Option<$from $($lt)*> {
            fn from(val: $to $($lt)*) -> Self {
                val.into_option()
            }
        }
    };
}

data_opt!(from: Data, to: DataOpt, docs: "An optional, owned", lt: <>);
data_opt!(from: DataRef, to: DataOptRef, docs: "An optional reference to", lt: <'a>);
data_opt!(from: DataCow, to: DataOptCow, docs: "An optional, owned or referenced", lt: <'a>);

impl From<Data> for DataOptCow<'_> {
    fn from(value: Data) -> Self {
        Self::from(DataCow::Owned(value))
    }
}

impl<'a> From<DataRef<'a>> for DataOptCow<'a> {
    fn from(value: DataRef<'a>) -> Self {
        Self::from(DataCow::Ref(value))
    }
}

impl From<DataOpt> for DataOptCow<'_> {
    fn from(value: DataOpt) -> Self {
        let opt = value.into_option().map(Into::into);
        Self::from_option(opt)
    }
}

impl<'a> From<DataOptRef<'a>> for DataOptCow<'a> {
    fn from(value: DataOptRef<'a>) -> Self {
        let opt = value.into_option().map(Into::into);
        Self::from_option(opt)
    }
}

/// Represents the different variants that [`Data`] can be, without concrete values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DataType {
    Text,
    Link,
    Picture,
    InvolvedPeople,
    Object,
    Duration,
}

/// An owned piece of metadata associated with a [`Tag`](super::Tag).
///
/// See [`DataOpt`], [`DataLike`] and [`DataSomeLike`].
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Data {
    Unsupported { reason: Option<String> },
    Text(String),
    Link(String),
    Picture(Picture),
    InvolvedPeople(InvolvedPeople),
    Object(Object),
    Duration(Duration),
}

impl Sealed for Data {}

impl From<Object> for Data {
    fn from(v: Object) -> Self {
        Self::Object(v)
    }
}

impl From<InvolvedPeople> for Data {
    fn from(v: InvolvedPeople) -> Self {
        Self::InvolvedPeople(v)
    }
}

impl From<Picture> for Data {
    fn from(v: Picture) -> Self {
        Self::Picture(v)
    }
}

impl From<String> for Data {
    fn from(v: String) -> Self {
        Self::Text(v)
    }
}

impl From<Duration> for Data {
    fn from(v: Duration) -> Self {
        Self::Duration(v)
    }
}

impl<'a> ToRef<'a, DataRef<'a>> for Data {
    fn to_ref(&self) -> DataRef<'_> {
        match self {
            Self::Unsupported { reason } => DataRef::Unsupported {
                reason: reason.as_deref(),
            },
            Self::Text(string) => DataRef::Text(string),
            Self::Link(string) => DataRef::Link(string),
            Self::Picture(owned) => DataRef::Picture(owned.to_ref()),
            Self::InvolvedPeople(people) => DataRef::InvolvedPeople(people.to_ref()),
            Self::Object(object) => DataRef::Object(object.to_ref()),
            Self::Duration(duration) => DataRef::Duration(*duration),
        }
    }
}

impl DataSomeLike for Data {
    fn into_owned(self) -> Data {
        self
    }
}

impl DataLike for Data {
    fn as_string(&self) -> Option<&str> {
        if let Self::Text(v) | Self::Link(v) = self {
            Some(v)
        } else {
            None
        }
    }

    fn as_text(&self) -> Option<&str> {
        if let Self::Text(v) = self {
            Some(v)
        } else {
            None
        }
    }

    fn as_link(&self) -> Option<&str> {
        if let Self::Link(v) = self {
            Some(v)
        } else {
            None
        }
    }

    fn as_picture(&self) -> Option<PictureRef<'_>> {
        if let Self::Picture(v) = self {
            Some(v.to_ref())
        } else {
            None
        }
    }

    fn as_involved_people(&self) -> Option<InvolvedPeopleRef<'_>> {
        if let Self::InvolvedPeople(v) = self {
            Some(v.to_ref())
        } else {
            None
        }
    }

    fn as_object(&self) -> Option<ObjectRef<'_>> {
        if let Self::Object(v) = self {
            Some(v.to_ref())
        } else {
            None
        }
    }

    fn as_duration(&self) -> Option<Duration> {
        if let Self::Duration(v) = self {
            Some(*v)
        } else {
            None
        }
    }

    fn data_type(&self) -> Option<DataType> {
        match self {
            Self::Unsupported { .. } => None,
            Self::Text(_) => Some(DataType::Text),
            Self::Link(_) => Some(DataType::Link),
            Self::Picture(_) => Some(DataType::Picture),
            Self::InvolvedPeople(_) => Some(DataType::InvolvedPeople),
            Self::Object(_) => Some(DataType::Object),
            Self::Duration(_) => Some(DataType::Duration),
        }
    }
}

impl Data {
    /// Attempts to unwrap this as a [`Data::Text`] or [`Data::Link`], returning `Err(self)` if the conversion
    /// fails.
    #[allow(clippy::missing_errors_doc)]
    pub fn try_into_string(self) -> Result<String, Self> {
        if let Self::Text(v) | Self::Link(v) = self {
            Ok(v)
        } else {
            Err(self)
        }
    }

    /// Attempts to unwrap this as a [`Data::Text`], returning `Err(self)` if the conversion
    /// fails.
    #[allow(clippy::missing_errors_doc)]
    pub fn try_into_text(self) -> Result<String, Self> {
        if let Self::Text(v) = self {
            Ok(v)
        } else {
            Err(self)
        }
    }

    /// Attempts to unwrap this as a [`Data::Link`], returning `Err(self)` if the conversion
    /// fails.
    #[allow(clippy::missing_errors_doc)]
    pub fn try_into_link(self) -> Result<String, Self> {
        if let Self::Link(v) = self {
            Ok(v)
        } else {
            Err(self)
        }
    }

    /// Attempts to unwrap this as a [`Data::Picture`], returning `Err(self)` if the conversion
    /// fails.
    #[allow(clippy::missing_errors_doc)]
    pub fn try_into_picture(self) -> Result<Picture, Self> {
        if let Self::Picture(v) = self {
            Ok(v)
        } else {
            Err(self)
        }
    }

    /// Attempts to unwrap this as a [`Data::InvolvedPeople`], returning `Err(self)` if the conversion
    /// fails.
    #[allow(clippy::missing_errors_doc)]
    pub fn try_into_involved_people(self) -> Result<InvolvedPeople, Self> {
        if let Self::InvolvedPeople(v) = self {
            Ok(v)
        } else {
            Err(self)
        }
    }

    /// Attempts to unwrap this as a [`Data::Object`], returning `Err(self)` if the conversion
    /// fails.
    #[allow(clippy::missing_errors_doc)]
    pub fn try_into_object(self) -> Result<Object, Self> {
        if let Self::Object(v) = self {
            Ok(v)
        } else {
            Err(self)
        }
    }
}

/// A reference to a piece of metadata associated with a [`Tag`](super::Tag).
///
/// This type can be expected to be cheaply cloneable ("claimable"), using reference counting if
/// necessary.
///
/// See [`DataOptRef`], [`DataLike`], and [`DataSomeLike`].
#[derive(Clone, Debug, Reborrow)]
#[non_exhaustive]
pub enum DataRef<'a> {
    Unsupported { reason: Option<&'a str> },
    Text(&'a str),
    Link(&'a str),
    Picture(PictureRef<'a>),
    InvolvedPeople(InvolvedPeopleRef<'a>),
    Object(ObjectRef<'a>),
    Duration(Duration),
}

impl Sealed for DataRef<'_> {}

impl DataRef<'_> {
    pub fn into_owned(self) -> Data {
        match self {
            Self::Unsupported { reason } => Data::Unsupported {
                reason: reason.map(str::to_owned),
            },
            Self::Text(string) => Data::Text((*string).to_owned()),
            Self::Link(string) => Data::Link((*string).to_owned()),
            Self::Picture(reference) => Data::Picture(reference.into_owned()),
            Self::InvolvedPeople(people) => Data::InvolvedPeople(people.into_owned()),
            Self::Object(object) => Data::Object(object.into_owned()),
            Self::Duration(duration) => Data::Duration(duration),
        }
    }
}

impl<'a> From<ObjectRef<'a>> for DataRef<'a> {
    fn from(v: ObjectRef<'a>) -> Self {
        Self::Object(v)
    }
}

impl<'a> From<InvolvedPeopleRef<'a>> for DataRef<'a> {
    fn from(v: InvolvedPeopleRef<'a>) -> Self {
        Self::InvolvedPeople(v)
    }
}

impl<'a> From<PictureRef<'a>> for DataRef<'a> {
    fn from(v: PictureRef<'a>) -> Self {
        Self::Picture(v)
    }
}

impl<'a> From<&'a str> for DataRef<'a> {
    fn from(v: &'a str) -> Self {
        Self::Text(v)
    }
}

impl From<Duration> for DataRef<'_> {
    fn from(v: Duration) -> Self {
        Self::Duration(v)
    }
}

impl<'a> ToRef<'a, DataRef<'a>> for DataRef<'_> {
    fn to_ref(&self) -> DataRef<'_> {
        self.clone()
    }
}

impl<'a> IntoOwnedImpl<'a> for DataRef<'a> {
    type Owned = Data;

    fn into_owned(self) -> Self::Owned {
        self.into_owned()
    }
}

impl DataSomeLike for DataRef<'_> {
    fn into_owned(self) -> Data {
        self.into_owned()
    }
}

impl DataLike for DataRef<'_> {
    fn as_string(&self) -> Option<&str> {
        if let Self::Text(v) | Self::Link(v) = self {
            Some(v)
        } else {
            None
        }
    }

    fn as_text(&self) -> Option<&str> {
        if let Self::Text(v) = self {
            Some(v)
        } else {
            None
        }
    }

    fn as_link(&self) -> Option<&str> {
        if let Self::Link(v) = self {
            Some(v)
        } else {
            None
        }
    }

    fn as_picture(&self) -> Option<PictureRef<'_>> {
        if let Self::Picture(v) = self {
            Some(*v)
        } else {
            None
        }
    }

    fn as_involved_people(&self) -> Option<InvolvedPeopleRef<'_>> {
        if let Self::InvolvedPeople(v) = self {
            Some(v.clone())
        } else {
            None
        }
    }

    fn as_object(&self) -> Option<ObjectRef<'_>> {
        if let Self::Object(v) = self {
            Some(*v)
        } else {
            None
        }
    }

    fn as_duration(&self) -> Option<Duration> {
        if let Self::Duration(v) = self {
            Some(*v)
        } else {
            None
        }
    }

    fn data_type(&self) -> Option<DataType> {
        match self {
            Self::Unsupported { .. } => None,
            Self::Text(_) => Some(DataType::Text),
            Self::Link(_) => Some(DataType::Link),
            Self::Picture(_) => Some(DataType::Picture),
            Self::InvolvedPeople(_) => Some(DataType::InvolvedPeople),
            Self::Object(_) => Some(DataType::Object),
            Self::Duration(_) => Some(DataType::Duration),
        }
    }
}

/// An owned or referenced piece of metadata associated with a [`Tag`](super::Tag).
///
/// See [`DataOptCow`], [`DataLike`], and [`DataSomeLike`].
#[derive(Clone, Debug)]
pub enum DataCow<'a> {
    Owned(Data),
    Ref(DataRef<'a>),
}

impl Sealed for DataCow<'_> {}

impl<'a> From<DataRef<'a>> for DataCow<'a> {
    fn from(v: DataRef<'a>) -> Self {
        Self::Ref(v)
    }
}

impl From<Data> for DataCow<'_> {
    fn from(v: Data) -> Self {
        Self::Owned(v)
    }
}

macro_rules! child_call {
    ($self:ident.child.$method:ident($($tt:tt)*)) => {
        match $self {
            Self::Owned(data) => data.$method($($tt)*),
            Self::Ref(data) => data.$method($($tt)*),
        }
    };
}

impl<'a> ToRef<'a, DataRef<'a>> for DataCow<'_> {
    fn to_ref(&self) -> DataRef<'_> {
        match self {
            Self::Owned(owned) => owned.to_ref(),
            Self::Ref(reference) => reference.clone(),
        }
    }
}

impl DataSomeLike for DataCow<'_> {
    fn into_owned(self) -> Data {
        match self {
            Self::Owned(owned) => owned,
            Self::Ref(reference) => reference.into_owned(),
        }
    }
}

impl DataLike for DataCow<'_> {
    fn as_string(&self) -> Option<&str> {
        child_call!(self.child.as_string())
    }

    fn as_text(&self) -> Option<&str> {
        child_call!(self.child.as_text())
    }

    fn as_link(&self) -> Option<&str> {
        child_call!(self.child.as_link())
    }

    fn as_picture(&self) -> Option<PictureRef<'_>> {
        child_call!(self.child.as_picture())
    }

    fn as_involved_people(&self) -> Option<InvolvedPeopleRef<'_>> {
        child_call!(self.child.as_involved_people())
    }

    fn as_object(&self) -> Option<ObjectRef<'_>> {
        child_call!(self.child.as_object())
    }

    fn as_duration(&self) -> Option<Duration> {
        child_call!(self.child.as_duration())
    }

    fn data_type(&self) -> Option<DataType> {
        child_call!(self.child.data_type())
    }
}

/// An owned picture that could be associated with an audio file.
#[derive(Clone)]
pub struct Picture {
    pub mime_type: String,
    pub description: String,
    pub data: Vec<u8>,
}

impl<'a> ToRef<'a, PictureRef<'a>> for Picture {
    /// Returns a [`PictureRef`] pointing to this.
    fn to_ref(&self) -> PictureRef<'_> {
        PictureRef {
            mime_type: &self.mime_type,
            description: &self.description,
            data: &self.data,
        }
    }
}

impl Debug for Picture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Picture")
            .field("mime_type", &self.mime_type)
            .field("description", &self.description)
            .field("data", &"<bytes>")
            .finish()
    }
}

/// A reference to a picture that could be associated with an audio file.
#[derive(Clone, Copy, Reborrow)]
pub struct PictureRef<'a> {
    pub mime_type: &'a str,
    pub description: &'a str,
    pub data: &'a [u8],
}

impl<'a> IntoOwnedImpl<'a> for PictureRef<'a> {
    type Owned = Picture;

    /// Creates an owned [`Picture`] struct by allocating this if necessary.
    fn into_owned(self) -> Self::Owned {
        Picture {
            mime_type: self.mime_type.to_owned(),
            description: self.description.to_owned(),
            data: self.data.to_owned(),
        }
    }
}

impl Debug for PictureRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Picture")
            .field("mime_type", &self.mime_type)
            .field("description", &self.description)
            .field("data", &"<bytes>")
            .finish()
    }
}

impl<'a> From<PictureRef<'a>> for Picture {
    fn from(value: PictureRef<'a>) -> Self {
        value.into_owned()
    }
}

impl<'a> From<&'a Picture> for PictureRef<'a> {
    fn from(value: &'a Picture) -> Self {
        value.to_ref()
    }
}

/// The type of a picture within an audio file.
///
/// This is taken from [id3 specification](https://hexdocs.pm/id3/ID3.Picture.html), since both `id3`
/// and `flac` have similar picture types. `m4a` is different, but that's okay.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PictureType {
    Other,
    Icon,
    OtherIcon,
    CoverFront,
    CoverBack,
    Leaflet,
    Media,
    LeadArtist,
    Artist,
    Conductor,
    Band,
    Composer,
    Lyricist,
    RecordingLocation,
    DuringRecording,
    DuringPerformance,
    ScreenCapture,
    BrightFish,
    Illustration,
    BandLogo,
    PublisherLogo,
}

/// A person that was involved in the creation of an audio file - see [`InvolvedPeople`].
#[derive(Clone, Debug)]
pub struct InvolvedPerson {
    pub name: String,
    pub involvement: String,
}

impl<'a> ToRef<'a, InvolvedPersonRef<'a>> for InvolvedPerson {
    /// Returns a [`InvolvedPersonRef`] pointing to this.
    fn to_ref(&'a self) -> InvolvedPersonRef<'a> {
        InvolvedPersonRef {
            name: &self.name,
            involvement: &self.involvement,
        }
    }
}

/// A reference to a person that was involved in the creation of an audio file - see [`InvolvedPeople`].
#[derive(Clone, Copy, Debug, Reborrow)]
pub struct InvolvedPersonRef<'a> {
    pub name: &'a str,
    pub involvement: &'a str,
}

impl<'a> IntoOwnedImpl<'a> for InvolvedPersonRef<'a> {
    type Owned = InvolvedPerson;

    /// Creates an owned [`InvolvedPerson`] struct by allocating this if necessary.
    fn into_owned(self) -> Self::Owned {
        InvolvedPerson {
            name: self.name.to_owned(),
            involvement: self.involvement.to_owned(),
        }
    }
}

/// A list of people involved in the creation of an audio file and their involvement.
#[derive(Clone, Debug)]
pub struct InvolvedPeople(pub Box<[InvolvedPerson]>);

impl<'a> ToRef<'a, InvolvedPeopleRef<'a>> for InvolvedPeople {
    /// Returns a [`InvolvedPeopleRef`] pointing to this.
    fn to_ref(&'a self) -> InvolvedPeopleRef<'a> {
        InvolvedPeopleRef::Slice(self)
    }
}

impl std::ops::Deref for InvolvedPeople {
    type Target = [InvolvedPerson];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl InvolvedPeople {
    /// Returns an iterator over the [`InvolvedPersonRef`]s in this list.
    #[must_use]
    pub fn iter(&self) -> InvolvedPeopleRefIter<'_> {
        InvolvedPeopleRefIter::Slice(self.0.iter())
    }
}

impl<'a> IntoIterator for &'a InvolvedPeople {
    type Item = InvolvedPersonRef<'a>;
    type IntoIter = InvolvedPeopleRefIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// A reference to a list of people involved in the creation of an audio file and their involvement.
#[derive(Clone, Debug, Reborrow)]
pub enum InvolvedPeopleRef<'a> {
    Slice(&'a [InvolvedPerson]),
    References(Arc<[InvolvedPersonRef<'a>]>),
}

impl<'a> IntoOwnedImpl<'a> for InvolvedPeopleRef<'a> {
    type Owned = InvolvedPeople;

    /// Creates an owned [`InvolvedPeople`] struct by allocating this if necessary.
    fn to_own(&self) -> Self::Owned {
        match self {
            Self::Slice(slice) => InvolvedPeople((*slice).into()),
            Self::References(references) => InvolvedPeople(
                references
                    .iter()
                    .copied()
                    .map(InvolvedPersonRef::into_owned)
                    .collect(),
            ),
        }
    }
}

impl InvolvedPeopleRef<'_> {
    /// Returns an iterator over the [`InvolvedPersonRef`]s in this list.
    #[must_use]
    pub fn iter(&self) -> InvolvedPeopleRefIter<'_> {
        match self {
            Self::Slice(slice) => InvolvedPeopleRefIter::Slice(slice.iter()),
            Self::References(references) => InvolvedPeopleRefIter::References(references.iter()),
        }
    }
}

impl<'a> IntoIterator for &'a InvolvedPeopleRef<'a> {
    type Item = InvolvedPersonRef<'a>;
    type IntoIter = InvolvedPeopleRefIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// An iterator over each person in an [`InvolvedPeople`].
pub enum InvolvedPeopleRefIter<'a> {
    Slice(std::slice::Iter<'a, InvolvedPerson>),
    References(std::slice::Iter<'a, InvolvedPersonRef<'a>>),
}

impl<'a> Iterator for InvolvedPeopleRefIter<'a> {
    type Item = InvolvedPersonRef<'a>;

    fn next(&mut self) -> Option<InvolvedPersonRef<'a>> {
        match self {
            Self::Slice(slice) => slice.next().map(InvolvedPerson::to_ref),
            Self::References(references) => references.next().copied(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Slice(slice) => slice.size_hint(),
            Self::References(references) => references.size_hint(),
        }
    }
}

/// An application-specific binary object.
#[derive(Clone, Debug)]
pub struct Object {
    pub mime_type: Option<String>,
    pub filename: Option<String>,
    pub data: Vec<u8>,
}

impl<'a> ToRef<'a, ObjectRef<'a>> for Object {
    /// Returns a [`ObjectRef`] pointing to this.
    fn to_ref(&'a self) -> ObjectRef<'a> {
        ObjectRef {
            mime_type: self.mime_type.as_deref(),
            filename: self.filename.as_deref(),
            data: &self.data[..],
        }
    }
}

/// A reference to an application-specific binary object.
#[derive(Clone, Copy, Debug, Reborrow)]
pub struct ObjectRef<'a> {
    pub mime_type: Option<&'a str>,
    pub filename: Option<&'a str>,
    pub data: &'a [u8],
}

impl<'a> IntoOwnedImpl<'a> for ObjectRef<'a> {
    type Owned = Object;

    /// Creates an owned [`Object`] struct by allocating this if necessary.
    fn into_owned(self) -> Self::Owned {
        Object {
            mime_type: self.mime_type.map(ToOwned::to_owned),
            filename: self.filename.map(ToOwned::to_owned),
            data: self.data.to_owned(),
        }
    }
}
