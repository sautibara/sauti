use std::{ops::Deref, sync::Arc};

use metaflac::{block::Application, Block};

use crate::decoder::metadata::{self, Operation};

use super::super::prelude::*;

fn map_err(err: metaflac::Error, source: SourceName) -> MetadataError {
    let metaflac::Error { kind, description } = err;
    match kind {
        metaflac::ErrorKind::Io(io_err) => metadata::MetadataError::IoError(io_err),
        metaflac::ErrorKind::StringDecoding(from_utf8_err) => {
            metadata::MetadataError::MalformedData {
                source,
                reason: Some(format!("{description}: {from_utf8_err}")),
            }
        }
        metaflac::ErrorKind::InvalidInput => metadata::MetadataError::UnsupportedFormat {
            source,
            reason: Some(description.to_string()),
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
            MediaSource::Path(path) => metaflac::Tag::read_from_path(path),
            MediaSource::Buffer(buf) => metaflac::Tag::read_from(&mut std::io::Cursor::new(buf)),
        };

        res.map(|tag| Tag {
            tag,
            source_name: source.name(),
        })
        .map_err(|err| map_err(err, source.name()))
    }

    fn supported_extensions(&self) -> ExtensionSet {
        // TODO: check if ogg and mka are supported
        ExtensionSet::from_slice(&["flac"])
    }
}

pub struct Tag {
    tag: metaflac::Tag,
    source_name: SourceName,
}

impl Tag {
    fn blocks(&self) -> impl Iterator<Item = WrapBlock<'_>> {
        self.tag.blocks().filter_map(WrapBlock::new)
    }

    const fn supports_id(id: &FrameId) -> bool {
        !matches!(id, FrameId::Duration | FrameId::InvolvedPeople)
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
        self.blocks().flat_map(WrapBlock::frames)
    }

    fn remove(&mut self, id: FrameId) -> MetadataResult<()> {
        let Some(id) = WrapSautiId::new(&id) else {
            return Err(MetadataError::Unimplemented(Operation::Remove(id)));
        };
        match id {
            WrapSautiId::Text(text) => {
                let text = text.id;
                self.tag.vorbis_comments_mut().comments.remove(text);
            }
            WrapSautiId::Picture(picture_type) => {
                let picture_type = picture_type.ty;
                self.tag.remove_picture_type(picture_type);
            }
            WrapSautiId::Object(object) => {
                // For some reason, metaflac doesn't allow removing only one object id,
                // so instead we have to clone all the other blocks ;-;
                let other_blocks: Vec<Block> = self
                    .tag
                    .blocks()
                    .filter(|block| {
                        matches!(
                            block,
                            Block::Application(application)
                            if !application.id.iter().copied().eq(object.bytes()),
                        )
                    })
                    .cloned()
                    .collect();
                self.tag.remove_blocks(metaflac::BlockType::Application);
                for block in other_blocks {
                    self.tag.push_block(block);
                }
            }
        }
        Ok(())
    }

    fn add(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        let Some(wrap_id) = WrapSautiId::new(&id) else {
            return Err(MetadataError::Unimplemented(Operation::Add(id)));
        };
        let res = match wrap_id {
            WrapSautiId::Text(text) => {
                if let Data::Text(string) | Data::Link(string) = data {
                    let id = text.id;
                    self.tag
                        .vorbis_comments_mut()
                        .comments
                        .entry(id.to_owned())
                        .or_default()
                        .push(string);
                    Ok(())
                } else {
                    Err(("expected Text or Link", data))
                }
            }
            WrapSautiId::Picture(picture_type) => {
                if let Data::Picture(picture) = data {
                    let picture_type = picture_type.ty;
                    self.tag
                        .add_picture(picture.mime_type, picture_type, picture.data);
                    Ok(())
                } else {
                    Err(("expected Picture", data))
                }
            }
            WrapSautiId::Object(object_id) => {
                if let Data::Object(object) = data {
                    let application_block = Application {
                        id: object_id.bytes().collect(),
                        data: object.data,
                    };
                    let block = Block::Application(application_block);
                    self.tag.push_block(block);
                    Ok(())
                } else {
                    Err(("expected Object", data))
                }
            }
        };
        res.map_err(|(reason, data)| MetadataError::InvalidDataType {
            id,
            reason: Some(reason.to_owned()),
            recovered_data: Box::new(DataOpt::some(data)),
        })
    }

    fn save(&self, path: impl AsRef<std::path::Path>) -> MetadataResult<()> {
        self.tag
            .clone() // for some odd reason write_to_path requires a mutable reference, so we clone
            .write_to_path(path)
            .map_err(|err| map_err(err, self.source_name.clone()))
    }

    fn supports(&self, query: metadata::Operation) -> bool {
        match query {
            metadata::Operation::Get(id)
            | metadata::Operation::GetAll(id)
            | metadata::Operation::Replace(id)
            | metadata::Operation::Add(id)
            | metadata::Operation::Remove(id) => Self::supports_id(&id),
            metadata::Operation::Data(data_type) => Self::supports_data(data_type),
            metadata::Operation::Frames | metadata::Operation::Save => true,
        }
    }
}

#[derive(Clone, Copy)]
enum WrapBlock<'a> {
    Comments(WrapComments<'a>),
    Picture(WrapPicture<'a>),
    Application(WrapApplication<'a>),
}

impl<'a> WrapBlock<'a> {
    const fn new(block: &'a metaflac::Block) -> Option<Self> {
        match block {
            metaflac::Block::VorbisComment(comments) => {
                Some(Self::Comments(WrapComments { comments }))
            }
            metaflac::Block::Picture(picture) => Some(Self::Picture(WrapPicture { picture })),
            metaflac::Block::Application(application_data) => {
                Some(Self::Application(WrapApplication { application_data }))
            }
            _ => None,
        }
    }

    fn frames(self) -> impl Iterator<Item = FrameCow<'a>> {
        match self {
            Self::Comments(comments) => Either::Left(comments.frames()),
            Self::Picture(picture) => Either::Right(Either::Left(std::iter::once(picture.frame()))),
            Self::Application(application) => {
                Either::Right(Either::Right(application.frame().into_iter()))
            }
        }
    }
}

#[derive(Clone, Copy)]
struct WrapComments<'a> {
    comments: &'a metaflac::block::VorbisComment,
}

impl<'a> WrapComments<'a> {
    fn comments(self) -> impl Iterator<Item = WrapComment<'a>> {
        self.comments
            .comments
            .iter()
            .map(|(id, values)| WrapComment {
                id: WrapId { id },
                values,
            })
    }

    fn frames(self) -> impl Iterator<Item = FrameCow<'a>> {
        self.comments().flat_map(WrapComment::frames)
    }
}

#[derive(Clone, Copy)]
struct WrapComment<'a> {
    id: WrapId<'a>,
    values: &'a [String],
}

impl<'a> WrapComment<'a> {
    fn frames(self) -> impl Iterator<Item = FrameCow<'a>> {
        let id = self.id.id();
        self.values
            .iter()
            .map(Deref::deref)
            .map(DataRef::Text)
            .map(DataCow::Ref)
            .map(move |data| FrameCow {
                id: id.clone(),
                data,
            })
    }
}

enum WrapSautiId<'a> {
    Text(WrapId<'a>),
    Picture(WrapPictureType),
    Object(&'a str),
}

impl<'a> WrapSautiId<'a> {
    fn new(id: &'a FrameId) -> Option<Self> {
        match id {
            FrameId::Title => Some(Self::Text(WrapId { id: "TITLE" })),
            FrameId::Album => Some(Self::Text(WrapId { id: "ALBUM" })),
            FrameId::Artist => Some(Self::Text(WrapId { id: "ARTIST" })),
            FrameId::AlbumArtist => Some(Self::Text(WrapId { id: "ALBUMARTIST" })),
            FrameId::Unknown(string)
            | FrameId::CustomText(string)
            | FrameId::CustomLink(string) => Some(Self::Text(WrapId { id: string })),
            FrameId::Picture(picture_type) => {
                Some(Self::Picture(WrapPictureType::from_sauti(*picture_type)))
            }
            FrameId::CustomObject(string) => Some(Self::Object(string)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
struct WrapId<'a> {
    id: &'a str,
}

impl WrapId<'_> {
    fn id(self) -> FrameId {
        match self.id {
            "TITLE" => FrameId::Title,
            "ALBUM" => FrameId::Album,
            "ARTIST" => FrameId::Artist,
            "ALBUMARTIST" => FrameId::AlbumArtist,
            other_str => FrameId::Unknown(Arc::from(other_str)),
        }
    }
}

#[derive(Clone, Copy)]
struct WrapPicture<'a> {
    picture: &'a metaflac::block::Picture,
}

impl<'a> WrapPicture<'a> {
    const fn id(self) -> FrameId {
        let ty = WrapPictureType {
            ty: self.picture.picture_type,
        };
        let ty = ty.picture_type();
        FrameId::Picture(ty)
    }

    fn picture(self) -> PictureRef<'a> {
        PictureRef {
            mime_type: &self.picture.mime_type,
            description: &self.picture.description,
            data: &self.picture.data,
        }
    }

    fn frame(self) -> FrameCow<'a> {
        let id = self.id();
        let picture = self.picture();
        let data = DataCow::Ref(DataRef::Picture(picture));
        FrameCow { id, data }
    }
}

#[derive(Clone, Copy)]
struct WrapPictureType {
    ty: metaflac::block::PictureType,
}

impl WrapPictureType {
    const fn from_sauti(picture_type: PictureType) -> Self {
        let ty = match picture_type {
            PictureType::Other => metaflac::block::PictureType::Other,
            PictureType::Icon => metaflac::block::PictureType::Icon,
            PictureType::OtherIcon => metaflac::block::PictureType::OtherIcon,
            PictureType::CoverFront => metaflac::block::PictureType::CoverFront,
            PictureType::CoverBack => metaflac::block::PictureType::CoverBack,
            PictureType::Leaflet => metaflac::block::PictureType::Leaflet,
            PictureType::Media => metaflac::block::PictureType::Media,
            PictureType::LeadArtist => metaflac::block::PictureType::LeadArtist,
            PictureType::Artist => metaflac::block::PictureType::Artist,
            PictureType::Conductor => metaflac::block::PictureType::Conductor,
            PictureType::Band => metaflac::block::PictureType::Band,
            PictureType::Composer => metaflac::block::PictureType::Composer,
            PictureType::Lyricist => metaflac::block::PictureType::Lyricist,
            PictureType::RecordingLocation => metaflac::block::PictureType::RecordingLocation,
            PictureType::DuringRecording => metaflac::block::PictureType::DuringRecording,
            PictureType::DuringPerformance => metaflac::block::PictureType::DuringPerformance,
            PictureType::ScreenCapture => metaflac::block::PictureType::ScreenCapture,
            PictureType::BrightFish => metaflac::block::PictureType::BrightFish,
            PictureType::Illustration => metaflac::block::PictureType::Illustration,
            PictureType::BandLogo => metaflac::block::PictureType::BandLogo,
            PictureType::PublisherLogo => metaflac::block::PictureType::PublisherLogo,
        };
        Self { ty }
    }

    const fn picture_type(self) -> PictureType {
        match self.ty {
            metaflac::block::PictureType::Other => PictureType::Other,
            metaflac::block::PictureType::Icon => PictureType::Icon,
            metaflac::block::PictureType::OtherIcon => PictureType::OtherIcon,
            metaflac::block::PictureType::CoverFront => PictureType::CoverFront,
            metaflac::block::PictureType::CoverBack => PictureType::CoverBack,
            metaflac::block::PictureType::Leaflet => PictureType::Leaflet,
            metaflac::block::PictureType::Media => PictureType::Media,
            metaflac::block::PictureType::LeadArtist => PictureType::LeadArtist,
            metaflac::block::PictureType::Artist => PictureType::Artist,
            metaflac::block::PictureType::Conductor => PictureType::Conductor,
            metaflac::block::PictureType::Band => PictureType::Band,
            metaflac::block::PictureType::Composer => PictureType::Composer,
            metaflac::block::PictureType::Lyricist => PictureType::Lyricist,
            metaflac::block::PictureType::RecordingLocation => PictureType::RecordingLocation,
            metaflac::block::PictureType::DuringRecording => PictureType::DuringRecording,
            metaflac::block::PictureType::DuringPerformance => PictureType::DuringPerformance,
            metaflac::block::PictureType::ScreenCapture => PictureType::ScreenCapture,
            metaflac::block::PictureType::BrightFish => PictureType::BrightFish,
            metaflac::block::PictureType::Illustration => PictureType::Illustration,
            metaflac::block::PictureType::BandLogo => PictureType::BandLogo,
            metaflac::block::PictureType::PublisherLogo => PictureType::PublisherLogo,
        }
    }
}

#[derive(Clone, Copy)]
struct WrapApplication<'a> {
    application_data: &'a Application,
}

impl<'a> WrapApplication<'a> {
    fn id(self) -> Option<FrameId> {
        str::from_utf8(&self.application_data.id)
            .ok()
            .map(Arc::from)
            .map(FrameId::CustomObject)
    }

    fn data(self) -> DataCow<'a> {
        DataCow::Ref(DataRef::Object(ObjectRef {
            mime_type: None,
            filename: None,
            data: &self.application_data.data,
        }))
    }

    fn frame(self) -> Option<FrameCow<'a>> {
        let id = self.id()?;
        let data = self.data();
        Some(FrameCow { id, data })
    }
}

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
