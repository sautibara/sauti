//! Convenience functions for iterators over [`FrameCow`]s - see [`FrameIterExt`].

use super::prelude::*;
use gat_borrow::{Boo, IntoOwned, ToRef};
use gat_lending_iterator::LendingIterator;

/// A [`LendingIterator`] that returns lent borrows filtered from an iterator of [`FrameCow`]s
pub struct Filtered<'d, R, I>
where
    R: IntoOwned<'d>,
    I: Iterator<Item = FrameCow<'d>>,
{
    iter: I,
    item: Option<R::Owned>,
    func: for<'a> fn(DataCow<'a>) -> Option<Boo<'a, R::Reborrow<'a>>>,
}

impl<'d, R, I> Filtered<'d, R, I>
where
    R: IntoOwned<'d>,
    I: Iterator<Item = FrameCow<'d>>,
{
    fn new(iter: I, func: for<'a> fn(DataCow<'a>) -> Option<Boo<'a, R::Reborrow<'a>>>) -> Self {
        Self {
            iter,
            item: None,
            func,
        }
    }
}

impl<'d, R, I> LendingIterator for Filtered<'d, R, I>
where
    R: IntoOwned<'d>,
    I: Iterator<Item = FrameCow<'d>>,
{
    type Item<'i>
        = (FrameId, R::Reborrow<'i>)
    where
        Self: 'i;

    fn next(&mut self) -> Option<Self::Item<'_>> {
        for FrameCow { id, data } in &mut self.iter {
            match (self.func)(data) {
                Some(Boo::Borrowed(borrow)) => {
                    return Some((id, borrow));
                }
                Some(Boo::Owned(owned)) => {
                    self.item = Some(owned.into());
                    return self
                        .item
                        .as_ref()
                        .map(ToRef::to_ref)
                        .map(|borrow| (id, borrow));
                }
                _ => (),
            }
        }
        None
    }
}

/// Extra convenience methods for iterators over [`FrameCow`]s.
///
/// NOTE: iterator adaptors will not work with any returned lending iterators due to an issue
/// with the language itself (see rust issue #91693). For these, you must explitictly iterate with
/// `while let` instead.
pub trait FrameIterExt<'d>: Iterator<Item = FrameCow<'d>> {
    /// Creates an iterator over references to yielded [string](DataLike::as_string) objects.
    ///
    /// The return type of this can be thought of as
    /// `impl LendingIterator<Item<'_> = (FrameId, &str)>`.
    fn strings(self) -> Filtered<'d, &'d str, Self>
    where
        Self: Sized,
    {
        fn func(data: DataCow<'_>) -> Option<Boo<'_, &str>> {
            match data {
                DataCow::Owned(Data::Text(owned) | Data::Link(owned)) => Some(Boo::Owned(owned)),
                DataCow::Ref(DataRef::Text(borrow) | DataRef::Link(borrow)) => {
                    Some(Boo::Borrowed(borrow))
                }
                _ => None,
            }
        }
        Filtered::new(self, func)
    }

    /// Creates an iterator over references to yielded [text](DataLike::as_text) objects.
    ///
    /// The return type of this can be thought of as
    /// `impl LendingIterator<Item<'_> = (FrameId, &str)>`.
    fn texts(self) -> Filtered<'d, &'d str, Self>
    where
        Self: Sized,
    {
        fn func(data: DataCow<'_>) -> Option<Boo<'_, &str>> {
            match data {
                DataCow::Owned(Data::Text(owned)) => Some(Boo::Owned(owned)),
                DataCow::Ref(DataRef::Text(borrow)) => Some(Boo::Borrowed(borrow)),
                _ => None,
            }
        }
        Filtered::new(self, func)
    }

    /// Creates an iterator over references to yielded [link](DataLike::as_link) objects.
    ///
    /// The return type of this can be thought of as
    /// `impl LendingIterator<Item<'_> = (FrameId, &str)>`.
    fn links(self) -> Filtered<'d, &'d str, Self>
    where
        Self: Sized,
    {
        fn func(data: DataCow<'_>) -> Option<Boo<'_, &str>> {
            match data {
                DataCow::Owned(Data::Text(owned)) => Some(Boo::Owned(owned)),
                DataCow::Ref(DataRef::Text(borrow)) => Some(Boo::Borrowed(borrow)),
                _ => None,
            }
        }
        Filtered::new(self, func)
    }

    /// Creates an iterator over references to yielded [picture](DataLike::as_picture) objects.
    ///
    /// The return type of this can be thought of as
    /// `impl LendingIterator<Item<'_> = (FrameId, PictureRef<'_>)>`.
    fn pictures(self) -> Filtered<'d, PictureRef<'d>, Self>
    where
        Self: Sized,
    {
        fn func(data: DataCow<'_>) -> Option<Boo<'_, PictureRef<'_>>> {
            match data {
                DataCow::Owned(Data::Picture(owned)) => Some(Boo::Owned(owned)),
                DataCow::Ref(DataRef::Picture(borrow)) => Some(Boo::Borrowed(borrow)),
                _ => None,
            }
        }
        Filtered::new(self, func)
    }

    /// Creates an iterator over references to yielded
    /// [involved people](DataLike::as_involved_people) objects.
    ///
    /// The return type of this can be thought of as
    /// `impl LendingIterator<Item<'_> = (FrameId, InvolvedPeopleRef<'_>)>`.
    fn involved_people_lists(self) -> Filtered<'d, InvolvedPeopleRef<'d>, Self>
    where
        Self: Sized,
    {
        fn func(data: DataCow<'_>) -> Option<Boo<'_, InvolvedPeopleRef<'_>>> {
            match data {
                DataCow::Owned(Data::InvolvedPeople(owned)) => Some(Boo::Owned(owned)),
                DataCow::Ref(DataRef::InvolvedPeople(borrow)) => Some(Boo::Borrowed(borrow)),
                _ => None,
            }
        }
        Filtered::new(self, func)
    }

    /// Creates an iterator over references to yielded [binary objects](DataLike::as_object).
    ///
    /// The return type of this can be thought of as
    /// `impl LendingIterator<Item<'_> = (FrameId, ObjectRef<'_>)>`.
    fn objects(self) -> Filtered<'d, ObjectRef<'d>, Self>
    where
        Self: Sized,
    {
        fn func(data: DataCow<'_>) -> Option<Boo<'_, ObjectRef<'_>>> {
            match data {
                DataCow::Owned(Data::Object(owned)) => Some(Boo::Owned(owned)),
                DataCow::Ref(DataRef::Object(borrow)) => Some(Boo::Borrowed(borrow)),
                _ => None,
            }
        }
        Filtered::new(self, func)
    }
}

impl<'d, I: Iterator<Item = FrameCow<'d>>> FrameIterExt<'d> for I {}
