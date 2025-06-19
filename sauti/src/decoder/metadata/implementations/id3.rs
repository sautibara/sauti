use std::sync::Arc;

use id3::{Content, TagLike};

use super::super::prelude::*;

#[derive(Default)]
pub struct Decoder;

impl Decoder {
    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // don't want to be restrained by const
    pub fn new() -> Self {
        Self
    }
}

impl super::super::Decoder for Decoder {
    type Tag = Tag;

    fn read_fallible(&self, source: &MediaSource) -> MetadataResult<Option<Self::Tag>> {
        let res = match source {
            MediaSource::Path(path) => id3::Tag::read_from_path(path),
            MediaSource::Buffer(buf) => id3::Tag::read_from2(std::io::Cursor::new(buf)),
        };

        match res {
            Result::Ok(tag) => Ok(Some(Tag { tag })),
            Result::Err(id3::Error {
                kind: id3::ErrorKind::Parsing | id3::ErrorKind::NoTag,
                ..
            }) => Ok(None),
            Result::Err(err) => Err(MetadataError::Other(Some(err.description))),
        }
    }

    fn supported_extensions(&self) -> ExtensionSet {
        // TODO: fill with all possible extensions
        ExtensionSet::from_slice(&["mp3", "id3", "wav", "wave", "aiff", "aif", "aifc"])
    }
}

enum Id {
    Text(&'static str),
    Picture(id3::frame::PictureType),
    InvolvedPeople,
    Object(Arc<str>),
    CustomText(Arc<str>),
    CustomLink(Arc<str>),
    Unknown(Arc<str>),
}

fn convert_sauti_to_id3_id(id: FrameId) -> Id {
    match id {
        FrameId::Title => Id::Text("TIT2"),
        FrameId::Album => Id::Text("TALB"),
        FrameId::Artist => Id::Text("TPE1"),
        FrameId::AlbumArtist => Id::Text("TPE2"),
        FrameId::InvolvedPeople => Id::InvolvedPeople,
        FrameId::Picture(picture_type) => {
            Id::Picture(convert_sauti_to_id3_picture_type(picture_type))
        }
        FrameId::CustomObject(string) => Id::Object(string),
        FrameId::CustomText(string) => Id::CustomText(string),
        FrameId::CustomLink(string) => Id::CustomLink(string),
        FrameId::Unknown(string) => Id::Unknown(string),
    }
}

fn convert_sauti_to_id3_data(data: Data) -> Result<id3::Content, Data> {
    match data {
        unsupported @ Data::Unsupported { .. } => Err(unsupported),
        Data::Text(string) => Ok(id3::Content::Text(string)),
        Data::Link(string) => Ok(id3::Content::Link(string)),
        Data::Picture(picture) => {
            let picture = convert_sauti_to_id3_picture(picture, id3::frame::PictureType::Other);
            Ok(id3::Content::Picture(picture))
        }
        Data::InvolvedPeople(ipl) => {
            let ipl = convert_sauti_to_id3_ipl(ipl);
            Ok(id3::Content::InvolvedPeopleList(ipl))
        }
        Data::Object(object) => {
            let object = convert_sauti_to_id3_object(object, "unknown");
            Ok(id3::Content::EncapsulatedObject(object))
        }
    }
}

fn convert_id3_to_sauti_data_owned(content: id3::Content) -> Data {
    match content {
        Content::Text(string)
        | Content::ExtendedText(id3::frame::ExtendedText { value: string, .. }) => {
            Data::Text(string)
        }
        Content::Link(string)
        | Content::ExtendedLink(id3::frame::ExtendedLink { link: string, .. }) => {
            Data::Link(string)
        }
        Content::Picture(picture) => Data::Picture(convert_id3_to_sauti_picture_owned(picture)),
        Content::InvolvedPeopleList(list) => {
            Data::InvolvedPeople(convert_id3_to_sauti_ipl_owned(list))
        }
        other => Data::Unsupported {
            reason: Some(format!("unsupported id3 content: {other:?}")),
        },
    }
}

fn convert_id3_to_sauti_data_optional(content: Option<&id3::Content>) -> DataOptCow {
    content.map(convert_id3_to_sauti_data).into()
}

fn convert_id3_to_sauti_data(content: &id3::Content) -> DataCow {
    convert_id3_to_sauti_data_all(content)
        .next()
        .expect("should always return at least one piece of data")
}

fn convert_id3_to_sauti_data_all(content: &id3::Content) -> DataResult {
    match content {
        Content::Text(string)
        | Content::ExtendedText(id3::frame::ExtendedText { value: string, .. }) => {
            DataResult::SplitStrText(string.split('\0'))
        }
        Content::Link(string)
        | Content::ExtendedLink(id3::frame::ExtendedLink { link: string, .. }) => {
            DataResult::SplitStrLink(string.split('\0'))
        }
        Content::Picture(picture) => DataRef::Picture(convert_id3_to_sauti_picture(picture)).into(),
        Content::InvolvedPeopleList(list) => {
            DataRef::InvolvedPeople(convert_id3_to_sauti_ipl(list)).into()
        }
        other => Data::Unsupported {
            reason: Some(format!("unsupported id3 content: {other:?}")),
        }
        .into(),
    }
}

enum DataResult<'a> {
    SplitStrText(std::str::Split<'a, char>),
    SplitStrLink(std::str::Split<'a, char>),
    Single(std::iter::Once<DataCow<'a>>),
}

impl<'a, T: Into<DataCow<'a>>> From<T> for DataResult<'a> {
    fn from(value: T) -> Self {
        DataResult::Single(std::iter::once(value.into()))
    }
}

impl<'a> Iterator for DataResult<'a> {
    type Item = DataCow<'a>;

    fn next(&mut self) -> Option<DataCow<'a>> {
        match self {
            Self::SplitStrText(split) => split.next().map(|string| DataRef::Text(string).into()),
            Self::SplitStrLink(split) => split.next().map(|string| DataRef::Link(string).into()),
            Self::Single(single) => single.next(),
        }
    }
}

fn convert_id3_to_sauti_frame_all(frame: &id3::frame::Frame) -> impl Iterator<Item = FrameCow> {
    let id = match (frame.id(), frame.content()) {
        ("TIT2", _) => FrameId::Title,
        ("TALB", _) => FrameId::Album,
        ("TPE1", _) => FrameId::Artist,
        ("TPE2", _) => FrameId::AlbumArtist,
        ("APIC", id3::Content::Picture(pic)) => {
            let picture_type = convert_id3_to_sauti_picture_type(pic.picture_type);
            FrameId::Picture(picture_type)
        }
        ("IPLS" | "TIPL" | "TMCL", _) => FrameId::InvolvedPeople,
        ("GEOB", id3::Content::EncapsulatedObject(object)) => {
            FrameId::CustomObject(object.description.clone().into())
        }
        (_, id3::Content::ExtendedText(id3::frame::ExtendedText { description, .. })) => {
            FrameId::CustomText(description.clone().into())
        }
        (_, id3::Content::ExtendedLink(id3::frame::ExtendedLink { description, .. })) => {
            FrameId::CustomLink(description.clone().into())
        }
        (other, _) => FrameId::Unknown(other.to_owned().into()),
    };

    let data = convert_id3_to_sauti_data_all(frame.content());

    data.map(move |data| FrameCow {
        id: id.clone(),
        data,
    })
}

fn convert_id3_to_sauti_picture(from: &id3::frame::Picture) -> PictureRef<'_> {
    let id3::frame::Picture {
        mime_type,
        description,
        data,
        ..
    } = from;
    PictureRef {
        mime_type,
        description,
        data,
    }
}

fn convert_id3_to_sauti_picture_owned(from: id3::frame::Picture) -> Picture {
    let id3::frame::Picture {
        mime_type,
        description,
        data,
        ..
    } = from;
    Picture {
        mime_type,
        description,
        data,
    }
}

fn convert_sauti_to_id3_picture(
    from: Picture,
    picture_type: id3::frame::PictureType,
) -> id3::frame::Picture {
    let Picture {
        mime_type,
        description,
        data,
    } = from;
    id3::frame::Picture {
        mime_type,
        picture_type,
        description,
        data,
    }
}

const fn convert_sauti_to_id3_picture_type(picture_type: PictureType) -> id3::frame::PictureType {
    match picture_type {
        PictureType::Other => id3::frame::PictureType::Other,
        PictureType::Icon => id3::frame::PictureType::Icon,
        PictureType::OtherIcon => id3::frame::PictureType::OtherIcon,
        PictureType::CoverFront => id3::frame::PictureType::CoverFront,
        PictureType::CoverBack => id3::frame::PictureType::CoverBack,
        PictureType::Leaflet => id3::frame::PictureType::Leaflet,
        PictureType::Media => id3::frame::PictureType::Media,
        PictureType::LeadArtist => id3::frame::PictureType::LeadArtist,
        PictureType::Artist => id3::frame::PictureType::Artist,
        PictureType::Conductor => id3::frame::PictureType::Conductor,
        PictureType::Band => id3::frame::PictureType::Band,
        PictureType::Composer => id3::frame::PictureType::Composer,
        PictureType::Lyricist => id3::frame::PictureType::Lyricist,
        PictureType::RecordingLocation => id3::frame::PictureType::RecordingLocation,
        PictureType::DuringRecording => id3::frame::PictureType::DuringRecording,
        PictureType::DuringPerformance => id3::frame::PictureType::DuringPerformance,
        PictureType::ScreenCapture => id3::frame::PictureType::ScreenCapture,
        PictureType::BrightFish => id3::frame::PictureType::BrightFish,
        PictureType::Illustration => id3::frame::PictureType::Illustration,
        PictureType::BandLogo => id3::frame::PictureType::BandLogo,
        PictureType::PublisherLogo => id3::frame::PictureType::PublisherLogo,
    }
}

const fn convert_id3_to_sauti_picture_type(picture_type: id3::frame::PictureType) -> PictureType {
    match picture_type {
        id3::frame::PictureType::Other | id3::frame::PictureType::Undefined(_) => {
            PictureType::Other
        }
        id3::frame::PictureType::Icon => PictureType::Icon,
        id3::frame::PictureType::OtherIcon => PictureType::OtherIcon,
        id3::frame::PictureType::CoverFront => PictureType::CoverFront,
        id3::frame::PictureType::CoverBack => PictureType::CoverBack,
        id3::frame::PictureType::Leaflet => PictureType::Leaflet,
        id3::frame::PictureType::Media => PictureType::Media,
        id3::frame::PictureType::LeadArtist => PictureType::LeadArtist,
        id3::frame::PictureType::Artist => PictureType::Artist,
        id3::frame::PictureType::Conductor => PictureType::Conductor,
        id3::frame::PictureType::Band => PictureType::Band,
        id3::frame::PictureType::Composer => PictureType::Composer,
        id3::frame::PictureType::Lyricist => PictureType::Lyricist,
        id3::frame::PictureType::RecordingLocation => PictureType::RecordingLocation,
        id3::frame::PictureType::DuringRecording => PictureType::DuringRecording,
        id3::frame::PictureType::DuringPerformance => PictureType::DuringPerformance,
        id3::frame::PictureType::ScreenCapture => PictureType::ScreenCapture,
        id3::frame::PictureType::BrightFish => PictureType::BrightFish,
        id3::frame::PictureType::Illustration => PictureType::Illustration,
        id3::frame::PictureType::BandLogo => PictureType::BandLogo,
        id3::frame::PictureType::PublisherLogo => PictureType::PublisherLogo,
    }
}

fn convert_id3_to_sauti_ipl(ipl: &id3::frame::InvolvedPeopleList) -> InvolvedPeopleRef<'_> {
    let list = convert_id3_to_iter_ipl(ipl).collect();
    InvolvedPeopleRef::References(list)
}

fn convert_id3_to_iter_ipl(
    ipl: &id3::frame::InvolvedPeopleList,
) -> impl Iterator<Item = InvolvedPersonRef> {
    ipl.items.iter().map(|item| InvolvedPersonRef {
        name: &item.involvee,
        involvement: &item.involvement,
    })
}

fn convert_id3_to_sauti_ipl_owned(ipl: id3::frame::InvolvedPeopleList) -> InvolvedPeople {
    let list = ipl
        .items
        .into_iter()
        .map(|item| InvolvedPerson {
            name: item.involvee,
            involvement: item.involvement,
        })
        .collect();
    InvolvedPeople(list)
}

fn convert_sauti_to_id3_ipl(ipl: InvolvedPeople) -> id3::frame::InvolvedPeopleList {
    let iter = Box::<[InvolvedPerson]>::into_iter(ipl.0);
    let items = iter
        .map(|item| id3::frame::InvolvedPeopleListItem {
            involvement: item.involvement,
            involvee: item.name,
        })
        .collect();
    id3::frame::InvolvedPeopleList { items }
}

fn convert_id3_to_sauti_object(object: &id3::frame::EncapsulatedObject) -> ObjectRef {
    let id3::frame::EncapsulatedObject {
        mime_type,
        filename,
        data,
        ..
    } = object;
    ObjectRef {
        mime_type: Some(mime_type),
        filename: Some(filename),
        data: &data[..],
    }
}

fn convert_sauti_to_id3_object(
    object: Object,
    description: &str,
) -> id3::frame::EncapsulatedObject {
    let Object {
        mime_type,
        filename,
        data,
    } = object;
    id3::frame::EncapsulatedObject {
        mime_type: mime_type.unwrap_or_else(|| "application/octet-stream".to_owned()),
        filename: filename.unwrap_or_else(|| "data.dat".to_owned()),
        description: description.to_owned(),
        data,
    }
}

pub struct Tag {
    tag: id3::Tag,
}

impl Tag {
    fn pictures(&self, picture_type: id3::frame::PictureType) -> impl Iterator<Item = DataCow<'_>> {
        self.tag
            .pictures()
            .filter(move |picture| picture.picture_type == picture_type)
            .map(convert_id3_to_sauti_picture)
            .map(|picture| DataCow::Ref(DataRef::Picture(picture)))
    }

    #[allow(clippy::map_unwrap_or)]
    fn object<'s>(&'s self, description: &'_ str) -> Option<DataCow<'s>> {
        self.tag
            .encapsulated_objects()
            .find(|obj| obj.description == description)
            .map(convert_id3_to_sauti_object)
            .map(DataRef::Object)
            .map(DataCow::Ref)
    }

    #[allow(clippy::map_unwrap_or)]
    fn custom_text<'s>(&'s self, description: &'_ str) -> Option<DataCow<'s>> {
        self.tag
            .extended_texts()
            .find(|obj| obj.description == description)
            .map(|text| &*text.value)
            .map(DataRef::Text)
            .map(DataCow::Ref)
    }

    #[allow(clippy::map_unwrap_or)]
    fn custom_link<'s>(&'s self, description: &'_ str) -> Option<DataCow<'s>> {
        self.tag
            .extended_links()
            .find(|obj| obj.description == description)
            .map(|text| &*text.link)
            .map(DataRef::Text)
            .map(DataCow::Ref)
    }

    fn get_text<'a>(&'a self, id: &str) -> DataOptCow<'a> {
        let frame = self.tag.get(id);
        let content = frame.map(id3::Frame::content);
        convert_id3_to_sauti_data_optional(content)
    }

    fn get_text_all(&self, id: impl AsRef<str>) -> impl Iterator<Item = DataCow> {
        self.tag
            .frames()
            .filter(move |frame| frame.id() == id.as_ref())
            .map(id3::Frame::content)
            .flat_map(convert_id3_to_sauti_data_all)
    }
}

impl super::super::Tag for Tag {
    fn get(&self, id: FrameId) -> DataOptCow<'_> {
        match convert_sauti_to_id3_id(id) {
            Id::Text(ident) => self.get_text(ident),
            Id::Unknown(ident) => self.get_text(&ident),
            Id::Picture(picture_type) => {
                let mut iter = self.pictures(picture_type);
                DataOptCow::from_option(iter.next())
            }
            Id::InvolvedPeople => {
                if self.tag.involved_people_lists().next().is_some() {
                    let references: Arc<[_]> = self
                        .tag
                        .involved_people_lists()
                        .flat_map(convert_id3_to_iter_ipl)
                        .collect();
                    let data_ref: DataRef = InvolvedPeopleRef::References(references).into();
                    DataOptCow::some(data_ref)
                } else {
                    DataOptCow::none()
                }
            }
            Id::CustomText(description) => self.custom_text(&description).into(),
            Id::CustomLink(description) => self.custom_link(&description).into(),
            Id::Object(description) => self.object(&description).into(),
        }
    }

    fn get_all(&self, id: FrameId) -> impl Iterator<Item = DataCow> {
        let id = convert_sauti_to_id3_id(id);
        match id {
            Id::Text(id) => {
                let iter = self.get_text_all(id);
                Box::new(iter) as Box<dyn Iterator<Item = _>>
            }
            Id::Unknown(id) => {
                let iter = self.get_text_all(id);
                Box::new(iter) as Box<dyn Iterator<Item = _>>
            }
            Id::Picture(picture_type) => {
                let iter = self.pictures(picture_type);
                Box::new(iter) as Box<dyn Iterator<Item = _>>
            }
            Id::InvolvedPeople => {
                let iter = self
                    .tag
                    .involved_people_lists()
                    .map(convert_id3_to_sauti_ipl)
                    .map(|ipl| DataCow::Ref(DataRef::InvolvedPeople(ipl)));
                Box::new(iter) as Box<dyn Iterator<Item = _>>
            }
            Id::CustomText(description) => {
                let value = self.custom_text(&description);
                let iter = value.into_iter();
                Box::new(iter) as Box<dyn Iterator<Item = _>>
            }
            Id::CustomLink(description) => {
                let value = self.custom_link(&description);
                let iter = value.into_iter();
                Box::new(iter) as Box<dyn Iterator<Item = _>>
            }
            Id::Object(description) => {
                let value = self.object(&description);
                let iter = value.into_iter();
                Box::new(iter) as Box<dyn Iterator<Item = _>>
            }
        }
    }

    fn replace(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        let res = match (
            convert_sauti_to_id3_id(id.clone()),
            convert_sauti_to_id3_data(data),
        ) {
            (Id::Text(ident), Ok(content @ (Content::Text(_) | Content::Link(_)))) => {
                self.tag.add_frame(id3::Frame::with_content(ident, content));
                Ok(())
            }
            (Id::Picture(picture_type), Ok(Content::Picture(mut picture))) => {
                picture.picture_type = picture_type;
                self.tag.add_frame(picture);
                Ok(())
            }
            (Id::InvolvedPeople, Ok(Content::InvolvedPeopleList(ipl))) => {
                self.tag.add_frame(ipl);
                Ok(())
            }
            (Id::CustomText(description), Ok(Content::Text(value) | Content::Link(value))) => {
                self.tag.add_frame(id3::frame::ExtendedText {
                    description: description.to_string(),
                    value,
                });
                Ok(())
            }
            (Id::CustomLink(description), Ok(Content::Text(link) | Content::Link(link))) => {
                self.tag.add_frame(id3::frame::ExtendedLink {
                    description: description.to_string(),
                    link,
                });
                Ok(())
            }
            (Id::Object(description), Ok(Content::EncapsulatedObject(mut object))) => {
                object.description = description.to_string();
                self.tag.add_frame(object);
                Ok(())
            }
            (Id::Unknown(id), Ok(content)) => {
                self.tag.add_frame(id3::Frame::with_content(id, content));
                Ok(())
            }
            (Id::Text(_) | Id::CustomText(_) | Id::CustomLink(_), content @ Ok(_)) => {
                Err(("expected Data::Text or Data::Link", content))
            }
            (Id::Picture(_), content @ Ok(_)) => Err(("expected Data::Picture", content)),
            (Id::InvolvedPeople, content @ Ok(_)) => {
                Err(("expected Data::InvolvedPeople", content))
            }
            (Id::Object(_), content @ Ok(_)) => Err(("expected Data::Object", content)),
            (_, content @ Err(_)) => Err(("cannot add unsupported data back to a tag", content)),
        };

        res.map_err(move |(reason, content)| {
            let data = content.map_or_else(std::convert::identity, convert_id3_to_sauti_data_owned);
            MetadataError::InvalidDataType {
                id,
                reason: Some(reason.to_owned()),
                recovered_data: Box::new(DataOpt::some(data)),
            }
        })
    }

    fn add(&mut self, id: FrameId, data: Data) -> MetadataResult<()> {
        fn add_to_previous_string(string: String, tag: &id3::Tag, id: &str) -> String {
            let prev_text = tag.get(id).and_then(|frame| {
                let content = frame.content();
                content.text().or_else(|| content.link())
            });

            if let Some(prev_text) = prev_text {
                format!("{prev_text}\0{string}")
            } else {
                string
            }
        }

        let data = match (convert_sauti_to_id3_id(id.clone()), data) {
            (Id::Text(ident), Data::Text(text)) => {
                let text = add_to_previous_string(text, &self.tag, ident);
                Data::Text(text)
            }
            (Id::Text(ident), Data::Link(link)) => {
                let link = add_to_previous_string(link, &self.tag, ident);
                Data::Link(link)
            }
            (_, other) => other,
        };

        self.replace(id, data)
    }

    fn remove(&mut self, id: FrameId) {
        match convert_sauti_to_id3_id(id) {
            Id::Text(ident) => {
                self.tag.remove(ident);
            }
            Id::Picture(picture_type) => {
                self.tag.remove_picture_by_type(picture_type);
            }
            Id::InvolvedPeople => {
                let ipl_ids: Vec<_> = self
                    .tag
                    .frames()
                    .filter(|frame| frame.content().involved_people_list().is_some())
                    .map(id3::Frame::id)
                    .map(ToOwned::to_owned)
                    .collect();
                for id in ipl_ids {
                    self.tag.remove(id);
                }
            }
            Id::CustomText(description) => {
                self.tag.remove_extended_text(Some(&description), None);
            }
            Id::CustomLink(description) => {
                // for some reason the id3 crate doesn't have a remove_extended_link method
                self.tag.frames_vec_mut().retain(|val| {
                    val.content()
                        .extended_link()
                        .is_none_or(|link| *link.description != *description)
                });
            }
            Id::Object(description) => {
                self.tag
                    .remove_encapsulated_object(Some(&description), None, None, None);
            }
            Id::Unknown(id) => {
                self.tag.remove(id);
            }
        }
    }

    fn frames(&self) -> impl Iterator<Item = FrameCow> {
        self.tag.frames().flat_map(convert_id3_to_sauti_frame_all)
    }

    fn save(&self, path: impl AsRef<std::path::Path>) -> MetadataResult<()> {
        self.tag
            .write_to_path(path, self.tag.version())
            .map_err(|err| MetadataError::Other(Some(err.description)))
    }

    fn supports(&self, _: crate::decoder::metadata::Supports) -> bool {
        true
    }
}
