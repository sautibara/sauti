use std::{borrow::Cow, sync::Arc};

use mp4ameta::{ident, Fourcc};

use crate::decoder::metadata::{self, Operation};

use super::super::prelude::*;

fn map_err(err: mp4ameta::Error, source: SourceName) -> MetadataError {
    use mp4ameta::ErrorKind;

    match err.kind {
        ErrorKind::Io(io_err) => MetadataError::Io(io_err),
        ErrorKind::NoFtyp => MetadataError::UnsupportedFormat {
            source,
            reason: Some(err.to_string()),
        },
        _ => MetadataError::MalformedData {
            source,
            reason: Some(err.to_string()),
        },
    }
}

#[derive(Default)]
pub struct Decoder;

impl Decoder {
    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // don't want to be restrained by const
    pub fn new() -> Self {
        Self
    }
}

impl metadata::Decoder for Decoder {
    type Tag = Tag;

    fn read(&self, source: &MediaSource) -> MetadataResult<Self::Tag> {
        let res = match source {
            MediaSource::Path(path) => mp4ameta::Tag::read_from_path(path),
            MediaSource::Buffer(buf) => mp4ameta::Tag::read_from(&mut std::io::Cursor::new(buf)),
        };

        res.map(|tag| Tag {
            tag,
            source: source.name(),
        })
        .map_err(|err| map_err(err, source.name()))
    }

    fn supported_extensions(&self) -> ExtensionSet {
        ExtensionSet::from_slice(&["mp4", "m4a"])
    }
}

pub struct Tag {
    tag: mp4ameta::Tag,
    source: SourceName,
}

impl Tag {
    fn wrapped_frames(&self) -> impl Iterator<Item = WrapFrame<'_>> {
        self.tag.data().map(WrapFrame::from_tuple)
    }

    const fn supports_id(id: &FrameId) -> bool {
        matches!(
            id,
            FrameId::Title
                | FrameId::Album
                | FrameId::Artist
                | FrameId::AlbumArtist
                | FrameId::Picture(PictureType::CoverFront)
                | FrameId::CustomText(_)
                | FrameId::CustomLink(_)
                | FrameId::CustomObject(_)
        )
    }

    const fn supports_data(data_type: DataType) -> bool {
        matches!(
            data_type,
            DataType::Text | DataType::Link | DataType::Picture | DataType::Object
        )
    }
}

impl metadata::Tag for Tag {
    fn frames(&self) -> impl Iterator<Item = FrameCow<'_>> {
        self.wrapped_frames().flat_map(WrapFrame::frames)
    }

    #[inline]
    fn supports(&self, query: Operation) -> bool {
        match query {
            Operation::Get(id)
            | Operation::GetAll(id)
            | Operation::Replace(id)
            | Operation::Add(id)
            | Operation::Remove(id) => Self::supports_id(&id),
            Operation::Data(data_type) => Self::supports_data(data_type),
            Operation::Frames | Operation::Save => true,
        }
    }

    fn remove(&mut self, id: FrameId) -> MetadataResult<()> {
        let Some(wrap_id) = WrapSautiId::new(&id)? else {
            return Err(MetadataError::Unimplemented(Operation::Remove(id)));
        };
        match wrap_id {
            WrapSautiId::Ident(ident) => {
                self.tag.remove_data_of(&ident);
            }
        }
        Ok(())
    }

    fn add(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        let Some(wrap_id) = WrapSautiId::new(&id)? else {
            return Err(MetadataError::Unimplemented(Operation::Add(id)));
        };
        match wrap_id {
            WrapSautiId::Ident(ident) => {
                let data = WrapDataOwned::new(data, &id)?;
                self.tag.add_data(ident, data.into_m4a());
            }
        }
        Ok(())
    }

    fn replace(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        let Some(wrap_id) = WrapSautiId::new(&id)? else {
            return Err(MetadataError::Unimplemented(Operation::Add(id)));
        };
        match wrap_id {
            WrapSautiId::Ident(ident) => {
                let data = WrapDataOwned::new(data, &id)?;
                self.tag.set_data(ident, data.into_m4a());
            }
        }
        Ok(())
    }

    fn save(&self, path: impl AsRef<std::path::Path>) -> MetadataResult<()> {
        self.tag
            .write_to_path(path)
            .map_err(|err| map_err(err, self.source.clone()))
    }
}

#[derive(Clone, Copy)]
struct WrapFrame<'a> {
    ident: WrapIdent<'a>,
    data: WrapData<'a>,
}

impl<'a> WrapFrame<'a> {
    fn from_tuple((ident, data): (&'a mp4ameta::DataIdent, &'a mp4ameta::Data)) -> Self {
        Self {
            ident: WrapIdent::new(ident),
            data: WrapData::new(data),
        }
    }

    fn frames(self) -> impl Iterator<Item = FrameCow<'a>> {
        self.data.frames(self.ident)
    }
}

enum WrapSautiId<'a> {
    Ident(WrapIdent<'a>),
}

impl<'a> WrapSautiId<'a> {
    fn new(id: &'a FrameId) -> MetadataResult<Option<Self>> {
        match id {
            FrameId::Title => Ok(Some(Self::Ident(WrapIdent::TITLE))),
            FrameId::Album => Ok(Some(Self::Ident(WrapIdent::ALBUM))),
            FrameId::Artist => Ok(Some(Self::Ident(WrapIdent::ARTIST))),
            FrameId::AlbumArtist => Ok(Some(Self::Ident(WrapIdent::ALBUM_ARTIST))),
            FrameId::Picture(PictureType::CoverFront) => Ok(Some(Self::Ident(WrapIdent::ARTWORK))),
            FrameId::CustomText(name) => Ok(Some(Self::Ident(WrapIdent::custom_text(name)))),
            FrameId::CustomLink(name) => Ok(Some(Self::Ident(WrapIdent::custom_link(name)))),
            FrameId::CustomObject(name) => Ok(Some(Self::Ident(WrapIdent::custom_object(name)))),
            FrameId::Unknown(id) => WrapIdent::parse_unknown(id).map(Self::Ident).map(Some),
            _ => Ok(None),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WrapIdent<'a> {
    Fourcc(Fourcc),
    Freeform { mean: &'a str, name: &'a str },
}

impl<'a> WrapIdent<'a> {
    fn new(ident: &'a mp4ameta::DataIdent) -> Self {
        match ident {
            mp4ameta::DataIdent::Fourcc(fourcc) => Self::Fourcc(*fourcc),
            mp4ameta::DataIdent::Freeform { mean, name } => Self::Freeform { mean, name },
        }
    }

    fn into_bytes(self) -> Arc<[u8]> {
        match self {
            // <fourcc>
            Self::Fourcc(fourcc) => fourcc.into_iter().collect(),
            // ----:<mean>:<name>
            Self::Freeform { mean, name } => (b"----:".iter().copied())
                .chain(mean.bytes())
                .chain(b":".iter().copied())
                .chain(name.bytes())
                .collect(),
        }
    }

    const TITLE: Self = Self::Fourcc(ident::TITLE);
    const ALBUM: Self = Self::Fourcc(ident::ALBUM);
    const ARTIST: Self = Self::Fourcc(ident::ARTIST);
    const ALBUM_ARTIST: Self = Self::Fourcc(ident::ALBUM_ARTIST);
    const ARTWORK: Self = Self::Fourcc(ident::ARTWORK);

    fn id(self) -> FrameId {
        match self {
            Self::TITLE => FrameId::Title,
            Self::ALBUM => FrameId::Album,
            Self::ARTIST => FrameId::Artist,
            Self::ALBUM_ARTIST => FrameId::AlbumArtist,
            Self::ARTWORK => FrameId::Picture(PictureType::CoverFront),
            Self::Freeform {
                mean: Self::CUSTOM_TEXT_MEAN,
                name,
            } => FrameId::CustomText(Arc::from(name)),
            Self::Freeform {
                mean: Self::CUSTOM_LINK_MEAN,
                name,
            } => FrameId::CustomLink(Arc::from(name)),
            Self::Freeform {
                mean: Self::CUSTOM_OBJECT_MEAN,
                name,
            } => FrameId::CustomObject(Arc::from(name)),
            other => FrameId::Unknown(UnknownId(other.into_bytes())),
        }
    }

    const CUSTOM_TEXT_MEAN: &'static str = "com.sauti.custom_text";
    const CUSTOM_LINK_MEAN: &'static str = "com.sauti.custom_link";
    const CUSTOM_OBJECT_MEAN: &'static str = "com.sauti.custom_object";

    const fn custom_text(name: &'a str) -> Self {
        Self::Freeform {
            mean: Self::CUSTOM_TEXT_MEAN,
            name,
        }
    }

    const fn custom_link(name: &'a str) -> Self {
        Self::Freeform {
            mean: Self::CUSTOM_LINK_MEAN,
            name,
        }
    }

    const fn custom_object(name: &'a str) -> Self {
        Self::Freeform {
            mean: Self::CUSTOM_OBJECT_MEAN,
            name,
        }
    }

    fn parse_unknown(bytes: &'a UnknownId) -> MetadataResult<Self> {
        fn parse_bytes_opt(bytes: &[u8]) -> Option<WrapIdent<'_>> {
            if let Ok(arr) = bytes.try_into() {
                Some(WrapIdent::Fourcc(Fourcc(arr)))
            } else {
                let mut bytes = bytes.iter();
                for _ in 0..4 {
                    if !matches!(bytes.next(), Some(b'-')) {
                        return None;
                    }
                }

                if !matches!(bytes.next(), Some(b'-')) {
                    return None;
                }

                let rest = bytes.as_slice();
                let mid = rest.iter().position(|&byte| byte == b':')?;

                let mean = &rest[..mid];
                let name = &rest[(mid + 1)..];

                if name.contains(&b':') {
                    return None;
                }

                let mean = str::from_utf8(mean).ok()?;
                let name = str::from_utf8(name).ok()?;

                Some(WrapIdent::Freeform { mean, name })
            }
        }

        parse_bytes_opt(&bytes.0).ok_or_else(|| {
            MetadataError::Other(Some(format!(
                concat!(
                    "expected FrameId::Unknown to be a four byte ident",
                    " or a freeform ident in the form of \"----:<mean>:<name>\"",
                    " with valid utf8, found {:?}"
                ),
                bytes
            )))
        })
    }
}

impl mp4ameta::Ident for WrapIdent<'_> {
    fn fourcc(&self) -> Option<Fourcc> {
        match self {
            Self::Fourcc(fourcc) => Some(*fourcc),
            Self::Freeform { .. } => None,
        }
    }

    fn freeform(&self) -> Option<ident::FreeformIdentBorrowed<'_>> {
        match self {
            Self::Freeform { mean, name } => Some(ident::FreeformIdent::new_borrowed(mean, name)),
            Self::Fourcc(_) => None,
        }
    }
}

impl PartialEq<mp4ameta::DataIdent> for WrapIdent<'_> {
    fn eq(&self, other: &mp4ameta::DataIdent) -> bool {
        match (self, other) {
            (Self::Fourcc(this), mp4ameta::DataIdent::Fourcc(other)) => this == other,
            (
                Self::Freeform {
                    mean: this_mean,
                    name: this_name,
                },
                mp4ameta::DataIdent::Freeform {
                    mean: other_mean,
                    name: other_name,
                },
            ) => this_mean == other_mean && this_name == other_name,
            _ => false,
        }
    }
}

impl From<WrapIdent<'_>> for mp4ameta::DataIdent {
    fn from(value: WrapIdent<'_>) -> Self {
        match value {
            WrapIdent::Fourcc(fourcc) => Self::Fourcc(fourcc),
            WrapIdent::Freeform { mean, name } => Self::Freeform {
                mean: Cow::Owned(mean.to_owned()),
                name: Cow::Owned(name.to_owned()),
            },
        }
    }
}

#[derive(Clone, Copy)]
enum WrapData<'a> {
    Text(WrapText<'a>),
    Image(WrapImage<'a>),
    Object(WrapReserved<'a>),
    Unknown(&'a mp4ameta::Data),
}

impl<'a> WrapData<'a> {
    fn new(data: &'a mp4ameta::Data) -> Self {
        match data {
            mp4ameta::Data::Utf8(text) | mp4ameta::Data::Utf16(text) => Self::Text(WrapText(text)),
            mp4ameta::Data::Png(buf) => Self::Image(WrapImage::png(buf)),
            mp4ameta::Data::Jpeg(buf) => Self::Image(WrapImage::jpeg(buf)),
            mp4ameta::Data::Bmp(buf) => Self::Image(WrapImage::bmp(buf)),
            mp4ameta::Data::Reserved(buf) => Self::Object(WrapReserved(buf)),
            other => Self::Unknown(other),
        }
    }

    fn frames(self, id: WrapIdent<'a>) -> impl Iterator<Item = FrameCow<'a>> {
        match self {
            Self::Text(text) => Left(std::iter::once(text.frame(id))),
            Self::Image(image) => Left(std::iter::once(image.frame(id))),
            Self::Object(object) => Right(object.frames(id)),
            Self::Unknown(data) => Left(std::iter::once(FrameCow {
                id: id.id(),
                data: DataCow::Owned(Data::Unsupported {
                    reason: Some(format!("unsupported m4a data: {data:?}")),
                }),
            })),
        }
    }
}

enum WrapDataOwned {
    Text(String),
    Picture(WrapPictureOwned),
    Object(WrapObjectOwned),
}

impl WrapDataOwned {
    fn new(data: Data, id: &FrameId) -> MetadataResult<Self> {
        match data {
            Data::Text(text) | Data::Link(text) => Ok(Self::Text(text)),
            Data::Picture(picture) => WrapPictureOwned::new(picture).map(Self::Picture),
            Data::Object(object) => Ok(Self::Object(WrapObjectOwned(object.data))),
            other => Err(MetadataError::InvalidDataType {
                id: id.clone(),
                reason: Some("expected Text, Link, or Picture".to_string()),
                recovered_data: Box::new(DataOpt::some(other)),
            }),
        }
    }

    fn into_m4a(self) -> mp4ameta::Data {
        match self {
            Self::Text(text) => mp4ameta::Data::Utf8(text),
            Self::Picture(WrapPictureOwned(mp4ameta::Img { fmt, data })) => match fmt {
                mp4ameta::ImgFmt::Png => mp4ameta::Data::Png(data),
                mp4ameta::ImgFmt::Jpeg => mp4ameta::Data::Jpeg(data),
                mp4ameta::ImgFmt::Bmp => mp4ameta::Data::Bmp(data),
            },
            Self::Object(object) => object.into_m4a(),
        }
    }
}

#[derive(Clone, Copy)]
struct WrapText<'a>(&'a str);

impl<'a> WrapText<'a> {
    fn frame(self, id: WrapIdent<'a>) -> FrameCow<'a> {
        let id = id.id();
        let data = if matches!(id, FrameId::CustomLink(_)) {
            DataRef::Link(self.0)
        } else {
            DataRef::Text(self.0)
        };
        FrameCow {
            id,
            data: DataCow::Ref(data),
        }
    }
}

#[derive(Clone, Copy)]
struct WrapImage<'a> {
    fmt: ImgFmt,
    buf: &'a [u8],
}

impl<'a> WrapImage<'a> {
    const fn png(buf: &'a [u8]) -> Self {
        Self {
            fmt: ImgFmt::Png,
            buf,
        }
    }

    const fn jpeg(buf: &'a [u8]) -> Self {
        Self {
            fmt: ImgFmt::Jpeg,
            buf,
        }
    }

    const fn bmp(buf: &'a [u8]) -> Self {
        Self {
            fmt: ImgFmt::Bmp,
            buf,
        }
    }

    const fn picture(self) -> PictureRef<'a> {
        PictureRef {
            mime_type: self.fmt.mime_type(),
            description: "",
            data: self.buf,
        }
    }

    fn frame(self, id: WrapIdent<'a>) -> FrameCow<'a> {
        FrameCow {
            id: id.id(),
            data: DataCow::Ref(DataRef::Picture(self.picture())),
        }
    }
}

struct WrapPictureOwned(mp4ameta::ImgBuf);

impl WrapPictureOwned {
    fn new(picture: Picture) -> MetadataResult<Self> {
        let Picture {
            mime_type, data, ..
        } = picture;
        let fmt = ImgFmt::from_mime_type(&mime_type)?.into();
        Ok(Self(mp4ameta::Img { data, fmt }))
    }
}

#[derive(Clone, Copy)]
enum ImgFmt {
    Bmp,
    Jpeg,
    Png,
}

impl ImgFmt {
    const MIME_TYPE_BMP: &str = "image/bmp";
    const MIME_TYPE_JPEG: &str = "image/jpeg";
    const MIME_TYPE_PNG: &str = "image/png";

    fn from_mime_type(mime_type: &str) -> MetadataResult<Self> {
        match mime_type {
            Self::MIME_TYPE_BMP => Ok(Self::Bmp),
            Self::MIME_TYPE_JPEG => Ok(Self::Jpeg),
            Self::MIME_TYPE_PNG => Ok(Self::Png),
            other => Err(MetadataError::Other(Some(format!(
                "unsupported mime type: '{other}', expected '{}', '{}', or '{}'",
                Self::MIME_TYPE_PNG,
                Self::MIME_TYPE_JPEG,
                Self::MIME_TYPE_BMP
            )))),
        }
    }

    const fn mime_type(self) -> &'static str {
        match self {
            Self::Bmp => Self::MIME_TYPE_BMP,
            Self::Jpeg => Self::MIME_TYPE_JPEG,
            Self::Png => Self::MIME_TYPE_PNG,
        }
    }
}

impl From<mp4ameta::ImgFmt> for ImgFmt {
    fn from(value: mp4ameta::ImgFmt) -> Self {
        match value {
            mp4ameta::ImgFmt::Bmp => Self::Bmp,
            mp4ameta::ImgFmt::Jpeg => Self::Jpeg,
            mp4ameta::ImgFmt::Png => Self::Png,
        }
    }
}

impl From<ImgFmt> for mp4ameta::ImgFmt {
    fn from(value: ImgFmt) -> Self {
        match value {
            ImgFmt::Bmp => Self::Bmp,
            ImgFmt::Jpeg => Self::Jpeg,
            ImgFmt::Png => Self::Png,
        }
    }
}

#[derive(Clone, Copy)]
struct WrapReserved<'a>(&'a [u8]);

impl<'a> WrapReserved<'a> {
    fn frames(self, id: WrapIdent<'a>) -> impl Iterator<Item = FrameCow<'a>> {
        std::iter::once(FrameCow {
            id: id.id(),
            data: DataCow::Ref(DataRef::Object(ObjectRef {
                mime_type: None,
                filename: None,
                data: self.0,
            })),
        })
    }
}

struct WrapObjectOwned(Vec<u8>);

impl WrapObjectOwned {
    fn into_m4a(self) -> mp4ameta::Data {
        mp4ameta::Data::Reserved(self.0)
    }
}

use Either::{Left, Right};
enum Either<Left, Right> {
    Left(Left),
    Right(Right),
}

impl<Left, Right, T> Iterator for Either<Left, Right>
where
    Left: Iterator<Item = T>,
    Right: Iterator<Item = T>,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Left(left) => left.next(),
            Self::Right(right) => right.next(),
        }
    }
}
