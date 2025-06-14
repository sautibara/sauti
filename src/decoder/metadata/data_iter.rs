//! Convenience functions for iterators over [`DataCow`]s - see [`DataIterExt`].

use super::prelude::*;
use gat_lending_iterator::LendingIterator;

/// An enumeration over owned or borrowed data, allowing for arbitrary referenced types.
///
/// Notice that this isn't a very elegant API; it's only meant to be used internally by
/// [`DataIterExt`], so it doesn't try to be elegant, but it has to be public because it's a trait.
pub enum Cow<'a, C: CowTypes> {
    Borrowed(C::Borrowed<'a>),
    Owned(C::Owned),
}

/// A trait that offers types for [`Cow`] to use.
pub trait CowTypes {
    type Borrowed<'a>;
    type Owned;

    fn to_ref(owned: &Self::Owned) -> Self::Borrowed<'_>;
}

type StringCow<'a> = Cow<'a, StringCowTypes>;
/// [`CowTypes`] for a string: [`String`] and [`&str`](str)
pub struct StringCowTypes;
impl CowTypes for StringCowTypes {
    type Owned = String;
    type Borrowed<'a> = &'a str;
    fn to_ref(owned: &Self::Owned) -> Self::Borrowed<'_> {
        owned
    }
}

type PictureCow<'a> = Cow<'a, PictureCowTypes>;
/// [`CowTypes`] for a picture: [`Picture`] and [`PictureRef`]
pub struct PictureCowTypes;
impl CowTypes for PictureCowTypes {
    type Owned = Picture;
    type Borrowed<'a> = PictureRef<'a>;
    fn to_ref(owned: &Self::Owned) -> Self::Borrowed<'_> {
        owned.to_ref()
    }
}

type InvolvedPeopleCow<'a> = Cow<'a, InvolvedPeopleCowTypes>;
/// [`CowTypes`] for an involved people list: [`InvolvedPeople`] and [`InvolvedPeopleRef`]
pub struct InvolvedPeopleCowTypes;
impl CowTypes for InvolvedPeopleCowTypes {
    type Owned = InvolvedPeople;
    type Borrowed<'a> = InvolvedPeopleRef<'a>;
    fn to_ref(owned: &Self::Owned) -> Self::Borrowed<'_> {
        owned.to_ref()
    }
}

type ObjectCow<'a> = Cow<'a, ObjectCowTypes>;
/// [`CowTypes`] for a binary object: [`Object`] and [`ObjectRef`]
pub struct ObjectCowTypes;
impl CowTypes for ObjectCowTypes {
    type Owned = Object;
    type Borrowed<'a> = ObjectRef<'a>;
    fn to_ref(owned: &Self::Owned) -> Self::Borrowed<'_> {
        owned.to_ref()
    }
}

/// A [`LendingIterator`] that returns [`Cow`]s filtered from an iterator of [`DataCow`]s
pub struct Filtered<'d, C, I>
where
    C: CowTypes,
    I: Iterator<Item = DataCow<'d>>,
{
    iter: I,
    item: Option<C::Owned>,
    func: for<'a> fn(DataCow<'a>) -> Option<Cow<'a, C>>,
}

impl<'d, C, I> Filtered<'d, C, I>
where
    C: CowTypes,
    I: Iterator<Item = DataCow<'d>>,
{
    fn new(iter: I, func: for<'a> fn(DataCow<'a>) -> Option<Cow<'a, C>>) -> Self {
        Self {
            iter,
            item: None,
            func,
        }
    }
}

impl<'d, C, I> LendingIterator for Filtered<'d, C, I>
where
    C: CowTypes,
    I: Iterator<Item = DataCow<'d>>,
{
    type Item<'i>
        = C::Borrowed<'i>
    where
        Self: 'i;

    fn next(&mut self) -> Option<Self::Item<'_>> {
        for val in &mut self.iter {
            match (self.func)(val) {
                Some(Cow::Borrowed(borrow)) => {
                    return Some(borrow);
                }
                Some(Cow::Owned(owned)) => {
                    self.item = Some(owned);
                    return self.item.as_ref().map(C::to_ref);
                }
                _ => (),
            }
        }
        None
    }
}

/// Extra convenience methods for iterators over [`DataCow`]s.
///
/// NOTE: iterator adaptors will not work with any returned lending iterators due to an issue
/// with the language itself (see rust issue #91693). For these, you must explitictly iterate with
/// `while let` instead.
pub trait DataIterExt<'d>: Iterator<Item = DataCow<'d>> {
    /// Creates an iterator over references to yielded [string](DataLike::as_string) objects.
    ///
    /// The return type of this can be thought of as `impl LendingIterator<Item<'_> = &str>`.
    fn strings(self) -> Filtered<'d, StringCowTypes, Self>
    where
        Self: Sized,
    {
        fn func(data: DataCow) -> Option<StringCow<'_>> {
            match data {
                DataCow::Owned(Data::Text(owned) | Data::Link(owned)) => Some(Cow::Owned(owned)),
                DataCow::Ref(DataRef::Text(borrow) | DataRef::Link(borrow)) => {
                    Some(Cow::Borrowed(borrow))
                }
                _ => None,
            }
        }
        Filtered::new(self, func)
    }

    /// Creates an iterator over references to yielded [text](DataLike::as_text) objects.
    ///
    /// The return type of this can be thought of as `impl LendingIterator<Item<'_> = &str>`.
    fn texts(self) -> Filtered<'d, StringCowTypes, Self>
    where
        Self: Sized,
    {
        fn func(data: DataCow) -> Option<StringCow<'_>> {
            match data {
                DataCow::Owned(Data::Text(owned)) => Some(Cow::Owned(owned)),
                DataCow::Ref(DataRef::Text(borrow)) => Some(Cow::Borrowed(borrow)),
                _ => None,
            }
        }
        Filtered::new(self, func)
    }

    /// Creates an iterator over references to yielded [link](DataLike::as_link) objects.
    fn links(self) -> Filtered<'d, StringCowTypes, Self>
    where
        Self: Sized,
    {
        fn func(data: DataCow) -> Option<StringCow<'_>> {
            match data {
                DataCow::Owned(Data::Text(owned)) => Some(Cow::Owned(owned)),
                DataCow::Ref(DataRef::Text(borrow)) => Some(Cow::Borrowed(borrow)),
                _ => None,
            }
        }
        Filtered::new(self, func)
    }

    /// Creates an iterator over references to yielded [picture](DataLike::as_picture) objects.
    ///
    /// The return type of this can be thought of as `impl LendingIterator<Item<'_> = PictureRef<'_>>`.
    fn pictures(self) -> Filtered<'d, PictureCowTypes, Self>
    where
        Self: Sized,
    {
        fn func(data: DataCow) -> Option<PictureCow<'_>> {
            match data {
                DataCow::Owned(Data::Picture(owned)) => Some(Cow::Owned(owned)),
                DataCow::Ref(DataRef::Picture(borrow)) => Some(Cow::Borrowed(borrow)),
                _ => None,
            }
        }
        Filtered::new(self, func)
    }

    /// Creates an iterator over references to yielded
    /// [involved people](DataLike::as_involved_people) objects.
    ///
    /// The return type of this can be thought of as
    /// `impl LendingIterator<Item<'_> = InvolvedPeopleRef<'_>>`.
    fn involved_people_lists(self) -> Filtered<'d, InvolvedPeopleCowTypes, Self>
    where
        Self: Sized,
    {
        fn func(data: DataCow) -> Option<InvolvedPeopleCow<'_>> {
            match data {
                DataCow::Owned(Data::InvolvedPeople(owned)) => Some(Cow::Owned(owned)),
                DataCow::Ref(DataRef::InvolvedPeople(borrow)) => Some(Cow::Borrowed(borrow)),
                _ => None,
            }
        }
        Filtered::new(self, func)
    }

    /// Creates an iterator over references to yielded [binary objects](DataLike::as_object).
    ///
    /// The return type of this can be thought of as
    /// `impl LendingIterator<Item<'_> = ObjectRef<'_>>`.
    fn objects(self) -> Filtered<'d, ObjectCowTypes, Self>
    where
        Self: Sized,
    {
        fn func(data: DataCow) -> Option<ObjectCow<'_>> {
            match data {
                DataCow::Owned(Data::Object(owned)) => Some(Cow::Owned(owned)),
                DataCow::Ref(DataRef::Object(borrow)) => Some(Cow::Borrowed(borrow)),
                _ => None,
            }
        }
        Filtered::new(self, func)
    }
}

impl<'d, I: Iterator<Item = DataCow<'d>>> DataIterExt<'d> for I {}
