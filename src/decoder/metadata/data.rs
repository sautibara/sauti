//! Different types of metadata that can be decoded from an audio file.

use std::{fmt::Debug, sync::Arc};

/// Any possible key for a [`Frame`] of metadata in a [`Tag`](super::Tag).
///
/// This type can be expected to be cheaply cloneable ("claimable"), using reference counting if
/// necessary.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FrameId {
    /// The text title of the song.
    Title,
    /// The album that the song is in.
    Album,
    /// The artist, including featured artists, of the specific song.
    Artist,
    /// The artist of the entire album.
    AlbumArtist,
    /// The list of everyone involved with the creation of the song as well as their roles - see
    /// [`InvolvedPeople`].
    InvolvedPeople,
    /// A specific picture associated with the song, such as the album art.
    Picture(PictureType),
    /// An application-specific binary object, identified by a key.
    CustomObject(Arc<str>),
    /// An application-specific string, identified by a key.
    CustomText(Arc<str>),
    /// An application-specific link, identified by a key.
    CustomLink(Arc<str>),
    /// An unknown id, specific to the background implementation. For example, this will likely be
    /// four letters for an id3 id.
    Unknown(Arc<str>),
}

/// An optional [key](FrameId)-[value](DataLike) pair of metadata associated with a [`Tag`](super::Tag).
///
/// See [`FrameOpt`], [`FrameOptRef`], and [`FrameOptCow`].
pub trait FrameOptLike: Sized + From<Option<Self::Some>> + Into<Option<Self::Some>> {
    type Some: FrameLike;

    /// Returns a new, empty frame.
    fn none() -> Self {
        Self::from_option(None)
    }

    /// Returns a new optional frame with a present value.
    fn some(val: impl Into<Self::Some>) -> Self {
        Self::from_option(Some(val.into()))
    }

    /// Returns a new [`FrameOptLike`] from `option`.
    fn from_option(option: Option<Self::Some>) -> Self;

    /// Convert this optional frame into an [`Option`].
    fn into_option(self) -> Option<Self::Some>;

    /// Convert this optional frame into an [`Option`]al reference.
    fn as_option(&self) -> Option<&Self::Some>;

    /// Returns `true` if the underlying option is [`None`].
    #[must_use]
    fn is_empty(&self) -> bool {
        self.as_option().is_none()
    }

    /// Returns `false` if the underlying option is [`None`].
    #[must_use]
    fn is_some(&self) -> bool {
        self.as_option().is_some()
    }

    /// Returns a [`FrameRef`] pointing to this frame.
    #[must_use]
    fn to_ref(&self) -> FrameOptRef {
        self.as_option().map(FrameLike::to_ref).into()
    }

    /// Creates an owned [`Frame`] struct by allocating this frame if necessary.
    #[must_use]
    fn into_owned(self) -> FrameOpt {
        self.into_option().map(FrameLike::into_owned).into()
    }

    /// Gets the key of the frame.
    fn id(&self) -> Option<&FrameId> {
        self.as_option().map(FrameLike::id)
    }

    /// Gets the value of the frame.
    fn data(&self) -> DataOptRef {
        self.as_option().map(FrameLike::data).into()
    }
}

/// A [key](FrameId)-[value](DataLike) pair of metadata associated with a [`Tag`](super::Tag).
///
/// See [`Frame`], [`FrameRef`], and [`FrameCow`].
pub trait FrameLike {
    /// Returns a [`FrameRef`] pointing to this frame.
    #[must_use]
    fn to_ref(&self) -> FrameRef;

    /// Creates an owned [`Frame`] struct by allocating this frame if necessary.
    #[must_use]
    fn into_owned(self) -> Frame;

    /// Gets the key of the frame.
    fn id(&self) -> &FrameId;

    /// Gets the value of the frame.
    fn data(&self) -> DataRef;
}

macro_rules! frame_opt {
    (from: $from:ident, to: $to:ident, docs: $docs:tt, lt: $($lt:tt)*) => {
        #[doc = concat!($docs, " [key](FrameId)-[value](DataLike) pair of metadata associated with a [`Tag`](super::Tag).\n")]
        #[doc = "\n"]
        #[doc = concat!("See [`", stringify!($from), "`], [`FrameOptLike`].")]
        #[derive(Clone, Debug)]
        pub struct $to $($lt)* (pub Option<$from $($lt)*>);

        impl $($lt)* FrameOptLike for $to $($lt)* {
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

frame_opt!(from: Frame, to: FrameOpt, docs: "An optional, owned", lt: <>);
frame_opt!(from: FrameRef, to: FrameOptRef, docs: "An optional reference to", lt: <'a>);
frame_opt!(from: FrameCow, to: FrameOptCow, docs: "An optional, owned or referenced", lt: <'a>);

/// An owned [key](FrameId)-[value](DataLike) pair of metadata associated with a [`Tag`](super::Tag).
///
/// See [`FrameLike`].
#[derive(Clone, Debug)]
pub struct Frame {
    pub id: FrameId,
    pub data: Data,
}

impl FrameLike for Frame {
    fn to_ref(&self) -> FrameRef {
        FrameRef {
            id: self.id.clone(),
            data: self.data.to_ref(),
        }
    }

    fn into_owned(self) -> Frame {
        self
    }

    fn id(&self) -> &FrameId {
        &self.id
    }

    fn data(&self) -> DataRef {
        self.data.to_ref()
    }
}

/// A reference to a [key](FrameId)-[value](DataLike) pair of metadata associated with a [`Tag`](super::Tag).
///
/// This type can be expected to be cheaply cloneable ("claimable"), using reference counting if
/// necessary.
///
/// See [`FrameLike`].
#[derive(Clone, Debug)]
pub struct FrameRef<'a> {
    pub id: FrameId,
    pub data: DataRef<'a>,
}

impl FrameLike for FrameRef<'_> {
    fn to_ref(&self) -> FrameRef {
        self.clone()
    }

    fn into_owned(self) -> Frame {
        Frame {
            id: self.id,
            data: self.data.into_owned(),
        }
    }

    fn id(&self) -> &FrameId {
        &self.id
    }

    fn data(&self) -> DataRef {
        self.data.to_ref()
    }
}

/// An owned or referenced [key](FrameId)-[value](DataLike) pair of metadata associated with a [`Tag`](super::Tag).
///
/// See [`FrameLike`].
#[derive(Clone, Debug)]
pub struct FrameCow<'a> {
    pub id: FrameId,
    pub data: DataCow<'a>,
}

impl FrameLike for FrameCow<'_> {
    /// Unwraps a [`FrameRef`] if the underlying data is a [`Ref`], or takes a reference if it is an
    /// [`Owned`].
    ///
    /// [`Owned`]: DataCow::Owned
    /// [`Ref`]: DataCow::Ref
    fn to_ref(&self) -> FrameRef {
        FrameRef {
            id: self.id.clone(),
            data: self.data.to_ref(),
        }
    }

    /// Unwraps [`Frame`] if the underlying data is an [`Owned`], or creates owned data by
    /// allocating if it is a [`Ref`].
    ///
    /// [`Owned`]: DataCow::Owned
    /// [`Ref`]: DataCow::Ref
    fn into_owned(self) -> Frame {
        Frame {
            id: self.id,
            data: self.data.into_owned(),
        }
    }

    fn id(&self) -> &FrameId {
        &self.id
    }

    fn data(&self) -> DataRef {
        self.data.to_ref()
    }
}

/// An optional piece of metadata associated with a [`Tag`](super::Tag).
///
/// See [`DataOpt`], [`DataOptRef`], and [`DataOptCow`].
pub trait DataOptLike: DataLike + From<Option<Self::Some>> + Into<Option<Self::Some>> {
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
    fn is_empty(&self) -> bool {
        self.as_option().is_none()
    }

    /// Returns `false` if the underlying option is [`None`].
    #[must_use]
    fn is_some(&self) -> bool {
        self.as_option().is_some()
    }

    /// Returns a [`DataOptRef`] pointing to this frame.
    #[must_use]
    fn to_ref(&self) -> DataOptRef {
        self.as_option().map(DataSomeLike::to_ref).into()
    }

    /// Creates an owned [`DataOpt`] struct by allocating this frame if necessary.
    #[must_use]
    fn into_owned(self) -> DataOpt {
        self.into_option().map(DataSomeLike::into_owned).into()
    }
}

/// A piece of metadata associated with a [`Tag`](super::Tag).
///
/// See [`Data`], [`DataRef`], and [`DataCow`].
pub trait DataSomeLike: DataLike {
    /// Returns a [`DataRef`] pointing to this data.
    #[must_use]
    fn to_ref(&self) -> DataRef<'_>;

    /// Creates an owned [`Data`] struct by allocating this data if necessary.
    #[must_use]
    fn into_owned(self) -> Data;
}

/// A potentially optional piece of metadata associated with a [`Tag`](super::Tag).
///
/// See [`Data`], [`DataRef`], [`DataCow`], [`DataOpt`], [`DataOptRef`], and [`DataOptCow`].
pub trait DataLike: Sized {
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
    fn as_picture(&self) -> Option<PictureRef>;

    /// Takes a reference to the underlying [`Data::InvolvedPeople`] if this is one, or returns [`None`].
    #[must_use]
    fn as_involved_people(&self) -> Option<InvolvedPeopleRef>;

    /// Takes a reference to the underlying [`Data::Object`] if this is one, or returns [`None`].
    #[must_use]
    fn as_object(&self) -> Option<ObjectRef>;
}

macro_rules! data_opt {
    (from: $from:ident, to: $to:ident, docs: $docs:tt, lt: $($lt:tt)*) => {
        #[doc = concat!($docs, " piece of metadata associated with a [`Tag`](super::Tag).\n")]
        #[doc = "\n"]
        #[doc = concat!("See [`", stringify!($from), "`], [`DataLike`], and [`DataOptLike`].")]
        #[derive(Clone, Debug)]
        pub struct $to $($lt)* (pub Option<$from $($lt)*>);

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

            fn as_picture(&self) -> Option<PictureRef> {
                self.as_option().and_then(|opt| opt.as_picture())
            }

            fn as_involved_people(&self) -> Option<InvolvedPeopleRef> {
                self.as_option().and_then(|opt| opt.as_involved_people())
            }

            fn as_object(&self) -> Option<ObjectRef> {
                self.as_option().and_then(|opt| opt.as_object())
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
}

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

impl DataSomeLike for Data {
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
        }
    }

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

    fn as_picture(&self) -> Option<PictureRef> {
        if let Self::Picture(v) = self {
            Some(v.to_ref())
        } else {
            None
        }
    }

    fn as_involved_people(&self) -> Option<InvolvedPeopleRef> {
        if let Self::InvolvedPeople(v) = self {
            Some(v.to_ref())
        } else {
            None
        }
    }

    fn as_object(&self) -> Option<ObjectRef> {
        if let Self::Object(v) = self {
            Some(v.to_ref())
        } else {
            None
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
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum DataRef<'a> {
    Unsupported { reason: Option<&'a str> },
    Text(&'a str),
    Link(&'a str),
    Picture(PictureRef<'a>),
    InvolvedPeople(InvolvedPeopleRef<'a>),
    Object(ObjectRef<'a>),
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

impl DataSomeLike for DataRef<'_> {
    fn to_ref(&self) -> DataRef<'_> {
        self.clone()
    }

    fn into_owned(self) -> Data {
        match self {
            Self::Unsupported { reason } => Data::Unsupported {
                reason: reason.map(str::to_owned),
            },
            Self::Text(string) => Data::Text((*string).to_owned()),
            Self::Link(string) => Data::Link((*string).to_owned()),
            Self::Picture(reference) => Data::Picture(reference.to_owned()),
            Self::InvolvedPeople(people) => Data::InvolvedPeople(people.to_owned()),
            Self::Object(object) => Data::Object(object.to_owned()),
        }
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

    fn as_picture(&self) -> Option<PictureRef> {
        if let Self::Picture(v) = self {
            Some(*v)
        } else {
            None
        }
    }

    fn as_involved_people(&self) -> Option<InvolvedPeopleRef> {
        if let Self::InvolvedPeople(v) = self {
            Some(v.clone())
        } else {
            None
        }
    }

    fn as_object(&self) -> Option<ObjectRef> {
        if let Self::Object(v) = self {
            Some(*v)
        } else {
            None
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

impl DataSomeLike for DataCow<'_> {
    fn to_ref(&self) -> DataRef<'_> {
        match self {
            Self::Owned(owned) => owned.to_ref(),
            Self::Ref(reference) => reference.clone(),
        }
    }

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

    fn as_picture(&self) -> Option<PictureRef> {
        child_call!(self.child.as_picture())
    }

    fn as_involved_people(&self) -> Option<InvolvedPeopleRef> {
        child_call!(self.child.as_involved_people())
    }

    fn as_object(&self) -> Option<ObjectRef> {
        child_call!(self.child.as_object())
    }
}

/// An owned picture that could be associated with an audio file.
#[derive(Clone)]
pub struct Picture {
    pub mime_type: String,
    pub description: String,
    pub data: Vec<u8>,
}

impl Picture {
    /// Returns a [`PictureRef`] pointing to this.
    #[must_use]
    pub fn to_ref(&self) -> PictureRef<'_> {
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
#[derive(Clone, Copy)]
pub struct PictureRef<'a> {
    pub mime_type: &'a str,
    pub description: &'a str,
    pub data: &'a [u8],
}

impl PictureRef<'_> {
    /// Creates an owned [`Picture`] struct by allocating this if necessary.
    #[must_use]
    pub fn to_owned(&self) -> Picture {
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
        value.to_owned()
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

impl InvolvedPerson {
    /// Returns a [`InvolvedPersonRef`] pointing to this.
    #[must_use]
    pub fn to_ref(&self) -> InvolvedPersonRef<'_> {
        InvolvedPersonRef {
            name: &self.name,
            involvement: &self.involvement,
        }
    }
}

/// A reference to a person that was involved in the creation of an audio file - see [`InvolvedPeople`].
#[derive(Clone, Copy, Debug)]
pub struct InvolvedPersonRef<'a> {
    pub name: &'a str,
    pub involvement: &'a str,
}

impl InvolvedPersonRef<'_> {
    /// Creates an owned [`InvolvedPerson`] struct by allocating this if necessary.
    #[must_use]
    pub fn to_owned(&self) -> InvolvedPerson {
        InvolvedPerson {
            name: self.name.to_owned(),
            involvement: self.involvement.to_owned(),
        }
    }
}

/// A list of people involved in the creation of an audio file and their involvement.
#[derive(Clone, Debug)]
pub struct InvolvedPeople(pub Box<[InvolvedPerson]>);

impl std::ops::Deref for InvolvedPeople {
    type Target = [InvolvedPerson];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl InvolvedPeople {
    /// Returns a [`InvolvedPeopleRef`] pointing to this.
    #[must_use]
    pub fn to_ref(&self) -> InvolvedPeopleRef<'_> {
        InvolvedPeopleRef::Slice(self)
    }

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
#[derive(Clone, Debug)]
pub enum InvolvedPeopleRef<'a> {
    Slice(&'a [InvolvedPerson]),
    References(Arc<[InvolvedPersonRef<'a>]>),
}

impl InvolvedPeopleRef<'_> {
    /// Creates an owned [`InvolvedPeople`] struct by allocating this if necessary.
    #[must_use]
    pub fn to_owned(&self) -> InvolvedPeople {
        match self {
            Self::Slice(slice) => InvolvedPeople((*slice).into()),
            Self::References(references) => {
                InvolvedPeople(references.iter().map(InvolvedPersonRef::to_owned).collect())
            }
        }
    }

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

impl Object {
    /// Returns a [`ObjectRef`] pointing to this.
    #[must_use]
    pub fn to_ref(&self) -> ObjectRef<'_> {
        ObjectRef {
            mime_type: self.mime_type.as_deref(),
            filename: self.filename.as_deref(),
            data: &self.data[..],
        }
    }
}

/// A reference to an application-specific binary object.
#[derive(Clone, Copy, Debug)]
pub struct ObjectRef<'a> {
    pub mime_type: Option<&'a str>,
    pub filename: Option<&'a str>,
    pub data: &'a [u8],
}

impl ObjectRef<'_> {
    /// Creates an owned [`Object`] struct by allocating this if necessary.
    #[must_use]
    pub fn to_owned(&self) -> Object {
        Object {
            mime_type: self.mime_type.map(ToOwned::to_owned),
            filename: self.filename.map(ToOwned::to_owned),
            data: self.data.to_owned(),
        }
    }
}
