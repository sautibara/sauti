use std::{
    fs::{File, OpenOptions},
    io::{BufReader, BufWriter, Cursor, Seek},
};

use oggvorbismeta::VorbisComments;

use crate::decoder::metadata::{self, Operation};

use super::super::prelude::*;
use base64::prelude::*;

fn map_err(err: oggvorbismeta::Error, source: SourceName) -> MetadataError {
    use oggvorbismeta::Error;

    match err {
        Error::WriteError(io_err) => MetadataError::Io(io_err),
        Error::OggReadError(ogg::OggReadError::NoCapturePatternFound) => {
            MetadataError::UnsupportedFormat {
                source,
                reason: Some("ogg data not found".to_owned()),
            }
        }
        other => MetadataError::MalformedData {
            source,
            reason: Some(other.to_string()),
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
            MediaSource::Path(path) => {
                oggvorbismeta::read_comment_header(BufReader::new(File::open(path)?))
            }
            MediaSource::Buffer(buf) => oggvorbismeta::read_comment_header(Cursor::new(buf)),
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
    tag: oggvorbismeta::CommentHeader,
    source: SourceName,
}

impl Tag {
    fn comments(&self) -> impl Iterator<Item = WrapComment<'_>> {
        self.tag.comment_list.iter().map(|(id, value)| WrapComment {
            id: WrapId { id: id.as_bytes() },
            value,
        })
    }

    const fn supports_id(id: &FrameId) -> bool {
        matches!(
            id,
            FrameId::Title
                | FrameId::Album
                | FrameId::Artist
                | FrameId::AlbumArtist
                | FrameId::Picture(_)
        )
    }

    const fn supports_data(data: DataType) -> bool {
        matches!(data, DataType::Text | DataType::Picture)
    }
}

impl metadata::Tag for Tag {
    fn get_all(&self, id: FrameId) -> impl Iterator<Item = FrameCow<'_>> {
        self.comments()
            // filter before converting the wrapping to a frame,
            // since we could skip potentially decoding a picture
            .filter(move |comment| comment.id == id)
            .map(WrapComment::frame)
    }

    fn frames(&self) -> impl Iterator<Item = FrameCow<'_>> {
        self.comments().map(WrapComment::frame)
    }

    fn supports(&self, value: metadata::Operation) -> bool {
        match value {
            Operation::Replace(id)
            | Operation::Add(id)
            | Operation::Remove(id)
            | Operation::Get(id)
            | Operation::GetAll(id) => Self::supports_id(&id),
            Operation::Frames | Operation::Save => true,
            Operation::Data(data) => Self::supports_data(data),
        }
    }

    fn remove(&mut self, id: FrameId) -> MetadataResult<()> {
        let Some(id) = WrapId::new(&id) else {
            return Err(MetadataError::Unimplemented(Operation::Remove(id)));
        };
        let id = id.as_utf8()?;
        self.tag.clear_tag(id);
        Ok(())
    }

    fn add(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        let Some(wrap_id) = WrapId::new(&id) else {
            return Err(MetadataError::Unimplemented(Operation::Add(id)));
        };

        let data = WrapDataOwned::new(data, &id)?;

        let res = match (wrap_id.is_picture(), data) {
            (true, WrapDataOwned::Picture(picture)) => {
                self.tag
                    .comment_list
                    .push((PICTURE_STR.to_owned(), picture.into_string()));
                Ok(())
            }
            (true, data) => Err(("expected Picture", data)),
            (false, WrapDataOwned::Text(text)) => {
                let id = wrap_id.as_utf8()?;
                self.tag.comment_list.push((id.to_string(), text));
                Ok(())
            }
            (false, data) => Err(("expected Text", data)),
        };

        res.map_err(|(err, data)| MetadataError::InvalidDataType {
            id,
            reason: Some(err.to_string()),
            recovered_data: Box::new(DataOpt::some(data.into_sauti())),
        })
    }

    fn save(&self, path: impl AsRef<std::path::Path>) -> MetadataResult<()> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;

        let cursor = oggvorbismeta::replace_comment_header(BufReader::new(&file), &self.tag)
            .map_err(|err| map_err(err, self.source.clone()))?;
        let vec = cursor.into_inner();

        file.set_len(vec.len() as u64)?;
        let mut writer = BufWriter::new(file);
        writer.seek(std::io::SeekFrom::Start(0))?;

        let mut cursor = Cursor::new(vec);
        std::io::copy(&mut cursor, &mut writer)?;

        Ok(())
    }
}

#[derive(Clone, Copy)]
struct WrapComment<'a> {
    id: WrapId<'a>,
    value: &'a str,
}

impl<'a> WrapComment<'a> {
    fn frame(self) -> FrameCow<'a> {
        let id = self.id.id();
        if matches!(id, FrameId::Picture(_)) {
            let data = BASE64_STANDARD.decode(self.value).map_or_else(
                |err| Data::Unsupported {
                    reason: Some(format!("failed to decode base64 picture: {err}")),
                },
                |data| {
                    Data::Picture(Picture {
                        mime_type: "application/octet-stream".to_owned(),
                        description: String::new(),
                        data,
                    })
                },
            );
            FrameCow {
                id,
                data: DataCow::Owned(data),
            }
        } else {
            FrameCow {
                id,
                data: DataCow::Ref(DataRef::Text(self.value)),
            }
        }
    }
}

const TITLE_BYTES: &[u8] = b"TITLE";
const ALBUM_BYTES: &[u8] = b"ALBUM";
const ARTIST_BYTES: &[u8] = b"ARTIST";
const ALBUMARTIST_BYTES: &[u8] = b"ALBUMARTIST";

const PICTURE_STR: &str = "METADATA_BLOCK_PICTURE";
const PICTURE_BYTES: &[u8] = PICTURE_STR.as_bytes();

#[derive(Clone, Copy)]
struct WrapId<'a> {
    id: &'a [u8],
}

impl<'a> WrapId<'a> {
    fn new(id: &'a FrameId) -> Option<Self> {
        match id {
            FrameId::Title => Some(Self { id: TITLE_BYTES }),
            FrameId::Album => Some(Self { id: ALBUM_BYTES }),
            FrameId::Artist => Some(Self { id: ARTIST_BYTES }),
            FrameId::AlbumArtist => Some(Self {
                id: ALBUMARTIST_BYTES,
            }),
            FrameId::Unknown(string) => Some(Self { id: &string.0 }),
            FrameId::CustomText(string) | FrameId::CustomLink(string) => Some(Self {
                id: string.as_bytes(),
            }),
            FrameId::Picture(_) => Some(Self { id: PICTURE_BYTES }),
            _ => None,
        }
    }

    fn id(self) -> FrameId {
        self.try_id()
            .unwrap_or_else(|| FrameId::Unknown(From::from(self.id)))
    }

    const fn try_id(self) -> Option<FrameId> {
        match self.id {
            TITLE_BYTES => Some(FrameId::Title),
            ALBUM_BYTES => Some(FrameId::Album),
            ARTIST_BYTES => Some(FrameId::Artist),
            ALBUMARTIST_BYTES => Some(FrameId::AlbumArtist),
            PICTURE_BYTES => Some(FrameId::Picture(PictureType::CoverFront)),
            _ => None,
        }
    }

    fn as_utf8(self) -> MetadataResult<&'a str> {
        str::from_utf8(self.id).map_err(|err| {
            MetadataError::Other(Some(format!(
                "expected valid utf8 for FrameId::Unknown: {err}"
            )))
        })
    }

    fn is_picture(self) -> bool {
        self.id.eq(PICTURE_BYTES)
    }
}

impl PartialEq<FrameId> for WrapId<'_> {
    fn eq(&self, other: &FrameId) -> bool {
        #[expect(clippy::option_if_let_else)] // I don't like this lint o-o
        if let Some(id) = self.try_id() {
            id == *other
        } else if let FrameId::Unknown(other) = other {
            self.id.eq(&other.0[..])
        } else {
            false
        }
    }
}

enum WrapDataOwned {
    Text(String),
    Picture(WrapPictureOwned),
}

impl WrapDataOwned {
    fn new(data: Data, id: &FrameId) -> MetadataResult<Self> {
        match data {
            Data::Text(string) | Data::Link(string) => Ok(Self::Text(string)),
            Data::Picture(picture) => Ok(Self::Picture(WrapPictureOwned(picture))),
            other => Err(other.data_type().map_or_else(
                || MetadataError::AddUnsupported {
                    id: id.clone(),
                    found: other,
                },
                |data_type| MetadataError::Unimplemented(Operation::Data(data_type)),
            )),
        }
    }

    fn into_sauti(self) -> Data {
        match self {
            Self::Text(text) => Data::Text(text),
            Self::Picture(picture) => Data::Picture(picture.0),
        }
    }
}

struct WrapPictureOwned(Picture);

impl WrapPictureOwned {
    fn into_string(self) -> String {
        BASE64_STANDARD.encode(self.0.data)
    }
}
