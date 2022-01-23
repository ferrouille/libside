use crate::system::System;
use serde::{
    de::{DeserializeOwned, MapAccess, Visitor},
    ser::SerializeMap,
    Deserialize, Serialize,
};
use std::{
    error::Error,
    fmt::{Debug, Display},
    marker::PhantomData,
};

#[macro_export]
macro_rules! requirements {
    ($($rest:ty),+ $(,)*) => {
        $crate::requirements::Wrapper::<$crate::requirements!(@ $($rest,)*)>
    };
    (@ $ty:ty $(, $rest:ty)+, ) => {
        $crate::requirements::Join::<$ty, $crate::requirements!(@ $($rest,)*)>
    };
    (@ $ty:ty,) => {
        $crate::requirements::Unit::<$ty>
    };
}

pub trait Requirement:
    Clone + std::fmt::Debug + std::fmt::Display + Serialize + DeserializeOwned
{
    const NAME: &'static str;

    type CreateError<S: System>: std::error::Error;
    type ModifyError<S: System>: std::error::Error;
    type DeleteError<S: System>: std::error::Error;
    type HasBeenCreatedError<S: System>: std::error::Error;

    fn create<S: System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>>;
    fn modify<S: System>(&self, system: &mut S) -> Result<(), Self::ModifyError<S>>;
    fn delete<S: System>(&self, system: &mut S) -> Result<(), Self::DeleteError<S>>;

    fn pre_existing_delete<S: System>(&self, _system: &mut S) -> Result<(), Self::DeleteError<S>> {
        Ok(())
    }

    fn has_been_created<S: System>(
        &self,
        system: &mut S,
    ) -> Result<bool, Self::HasBeenCreatedError<S>>;

    /// Determines whether the other requirement is satisfied if the current requirement already holds.
    fn affects(&self, other: &Self) -> bool;
    fn supports_modifications(&self) -> bool;
    fn can_undo(&self) -> bool;
    fn may_pre_exist(&self) -> bool;

    fn verify<S: System>(&self, system: &mut S) -> Result<bool, ()>;
}

pub trait RequirementList: Clone + Debug + Display + Serialize + DeserializeOwned {
    type CreateError<S: super::system::System>: ErrorList;
    type ModifyError<S: super::system::System>: ErrorList;
    type DeleteError<S: super::system::System>: ErrorList;
    type HasBeenCreatedError<S: super::system::System>: ErrorList;
    type Visitor<'de>: Visitor<'de> + Default + DecodeKey<Output = Self>;

    fn create<S: super::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>>;

    fn modify<S: super::system::System>(&self, system: &mut S) -> Result<(), Self::ModifyError<S>>;

    fn delete<S: super::system::System>(&self, system: &mut S) -> Result<(), Self::DeleteError<S>>;

    fn has_been_created<S: super::system::System>(
        &self,
        system: &mut S,
    ) -> Result<bool, Self::HasBeenCreatedError<S>>;

    fn affects(&self, other: &Self) -> bool;

    fn supports_modifications(&self) -> bool;

    fn can_undo(&self) -> bool;

    fn may_pre_exist(&self) -> bool;

    fn verify<S: super::system::System>(&self, system: &mut S) -> Result<bool, ()>;
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Join<A: Requirement, B: RequirementList> {
    Item(A),
    Next(B),
}

impl<A: Requirement, B: RequirementList> Serialize for Join<A, B> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Join::Item(item) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry(A::NAME, item)?;
                map.end()
            }
            Join::Next(next) => next.serialize(serializer),
        }
    }
}

#[derive(Copy, Clone)]
pub struct JoinVisitor<A: Requirement, B: RequirementList> {
    _phantom: PhantomData<(A, B)>,
}

impl<A: Requirement, B: RequirementList> Default for JoinVisitor<A, B> {
    fn default() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

pub trait DecodeKey {
    type Output;

    fn decode<'de, M: MapAccess<'de>>(key: &str, access: M) -> Result<Self::Output, M::Error>;
}

impl<A: Requirement, B: RequirementList> DecodeKey for JoinVisitor<A, B> {
    type Output = Join<A, B>;

    fn decode<'de, M: MapAccess<'de>>(key: &str, mut access: M) -> Result<Self::Output, M::Error> {
        Ok(if key == A::NAME {
            Join::Item(access.next_value()?)
        } else {
            Join::Next(B::Visitor::<'de>::decode(key, access)?)
        })
    }
}

impl<'de, A: Requirement, B: RequirementList> Visitor<'de> for JoinVisitor<A, B> {
    type Value = Join<A, B>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a requirement")
    }

    fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let key: String = access.next_key()?.unwrap();
        Self::decode(&key, access)
    }
}

impl<'de, A: Requirement, B: RequirementList> Deserialize<'de> for Join<A, B> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(<Self as RequirementList>::Visitor::<'de>::default())
    }
}

impl<A: Requirement, B: RequirementList> Display for Join<A, B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Item(item) => Display::fmt(item, f),
            Self::Next(next) => Display::fmt(next, f),
        }
    }
}

impl<A: Requirement, B: RequirementList> Debug for Join<A, B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Item(item) => Debug::fmt(item, f),
            Self::Next(next) => Debug::fmt(next, f),
        }
    }
}

pub trait ErrorList: Error {}

pub enum JoinedError<A: Error, B: ErrorList> {
    Item(A),
    Next(B),
}

impl<A: Error, B: ErrorList> Display for JoinedError<A, B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Item(item) => Display::fmt(item, f),
            Self::Next(next) => Display::fmt(next, f),
        }
    }
}

impl<A: Error, B: ErrorList> Debug for JoinedError<A, B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Item(item) => Debug::fmt(item, f),
            Self::Next(next) => Debug::fmt(next, f),
        }
    }
}

impl<A: Error, B: ErrorList> Error for JoinedError<A, B> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Item(item) => Error::source(item),
            Self::Next(next) => Error::source(next),
        }
    }
}

impl<A: Error, B: ErrorList> ErrorList for JoinedError<A, B> {}

impl<A: Requirement, B: RequirementList> RequirementList for Join<A, B> {
    type CreateError<S: super::system::System> =
        JoinedError<A::CreateError<S>, <B as RequirementList>::CreateError<S>>;
    type ModifyError<S: super::system::System> =
        JoinedError<A::ModifyError<S>, <B as RequirementList>::ModifyError<S>>;
    type DeleteError<S: super::system::System> =
        JoinedError<A::DeleteError<S>, <B as RequirementList>::DeleteError<S>>;
    type HasBeenCreatedError<S: super::system::System> =
        JoinedError<A::HasBeenCreatedError<S>, <B as RequirementList>::HasBeenCreatedError<S>>;
    type Visitor<'de> = JoinVisitor<A, B>;

    fn create<S: super::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        match self {
            Self::Item(item) => Requirement::create(item, system).map_err(JoinedError::Item),
            Self::Next(next) => RequirementList::create(next, system).map_err(JoinedError::Next),
        }
    }

    fn modify<S: super::system::System>(&self, system: &mut S) -> Result<(), Self::ModifyError<S>> {
        match self {
            Self::Item(item) => Requirement::modify(item, system).map_err(JoinedError::Item),
            Self::Next(next) => RequirementList::modify(next, system).map_err(JoinedError::Next),
        }
    }

    fn delete<S: super::system::System>(&self, system: &mut S) -> Result<(), Self::DeleteError<S>> {
        match self {
            Self::Item(item) => Requirement::delete(item, system).map_err(JoinedError::Item),
            Self::Next(next) => RequirementList::delete(next, system).map_err(JoinedError::Next),
        }
    }

    fn has_been_created<S: super::system::System>(
        &self,
        system: &mut S,
    ) -> Result<bool, Self::HasBeenCreatedError<S>> {
        match self {
            Self::Item(item) => {
                Requirement::has_been_created(item, system).map_err(JoinedError::Item)
            }
            Self::Next(next) => {
                RequirementList::has_been_created(next, system).map_err(JoinedError::Next)
            }
        }
    }

    fn affects(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Item(item), Self::Item(other)) => Requirement::affects(item, other),
            (Self::Next(next), Self::Next(other)) => RequirementList::affects(next, other),
            _ => false,
        }
    }

    fn supports_modifications(&self) -> bool {
        match self {
            Self::Item(item) => Requirement::supports_modifications(item),
            Self::Next(next) => RequirementList::supports_modifications(next),
        }
    }

    fn can_undo(&self) -> bool {
        match self {
            Self::Item(item) => Requirement::can_undo(item),
            Self::Next(next) => RequirementList::can_undo(next),
        }
    }

    fn may_pre_exist(&self) -> bool {
        match self {
            Self::Item(item) => Requirement::may_pre_exist(item),
            Self::Next(next) => RequirementList::may_pre_exist(next),
        }
    }

    fn verify<S: super::system::System>(&self, system: &mut S) -> Result<bool, ()> {
        match self {
            Self::Item(item) => Requirement::verify(item, system),
            Self::Next(next) => RequirementList::verify(next, system),
        }
    }
}

pub trait Supports<R> {
    fn create_from(item: R) -> Self;
}

impl<A: Requirement, B: RequirementList> Supports<A> for Join<A, B> {
    fn create_from(item: A) -> Self {
        Join::Item(item)
    }
}

impl<T, A: Requirement, B: RequirementList + Supports<T>> Supports<T> for Join<A, B>
where
    Cmp<T, A>: Neq,
    Cmp<T, Join<A, B>>: Neq,
{
    fn create_from(next: T) -> Self {
        Join::Next(B::create_from(next))
    }
}

pub struct Cmp<A, B>(A, B);

pub trait Eq {}

impl<T> Eq for Cmp<T, T> {}

pub auto trait Neq {}
impl<T> !Neq for Cmp<T, T> {}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Unit<A: Requirement>(A);

impl<A: Requirement> Serialize for Unit<A> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(A::NAME, &self.0)?;
        map.end()
    }
}

#[derive(Copy, Clone)]
pub struct UnitVisitor<A: Requirement> {
    _phantom: PhantomData<A>,
}

impl<A: Requirement> Default for UnitVisitor<A> {
    fn default() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<A: Requirement> DecodeKey for UnitVisitor<A> {
    type Output = Unit<A>;

    fn decode<'de, M: MapAccess<'de>>(key: &str, mut access: M) -> Result<Self::Output, M::Error> {
        Ok(if key == A::NAME {
            Unit(access.next_value()?)
        } else {
            panic!("Unknown key: {}", key)
        })
    }
}

impl<'de, A: Requirement> Visitor<'de> for UnitVisitor<A> {
    type Value = Unit<A>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a requirement")
    }

    fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let key: String = access.next_key()?.unwrap();
        Self::decode(&key, access)
    }
}

impl<'de, A: Requirement> Deserialize<'de> for Unit<A> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(<Self as RequirementList>::Visitor::<'de>::default())
    }
}

impl<A: Requirement> Display for Unit<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl<A: Requirement> Debug for Unit<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

pub struct UnitError<A: Error>(A);

impl<A: Error> Display for UnitError<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl<A: Error> Debug for UnitError<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl<A: Error> Error for UnitError<A> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Error::source(&self.0)
    }
}

impl<A: Error> ErrorList for UnitError<A> {}

impl<A: Requirement> RequirementList for Unit<A> {
    type CreateError<S: super::system::System> = UnitError<A::CreateError<S>>;
    type ModifyError<S: super::system::System> = UnitError<A::ModifyError<S>>;
    type DeleteError<S: super::system::System> = UnitError<A::DeleteError<S>>;
    type HasBeenCreatedError<S: super::system::System> = UnitError<A::HasBeenCreatedError<S>>;
    type Visitor<'de> = UnitVisitor<A>;

    fn create<S: super::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        Requirement::create(&self.0, system).map_err(UnitError)
    }

    fn modify<S: super::system::System>(&self, system: &mut S) -> Result<(), Self::ModifyError<S>> {
        Requirement::modify(&self.0, system).map_err(UnitError)
    }

    fn delete<S: super::system::System>(&self, system: &mut S) -> Result<(), Self::DeleteError<S>> {
        Requirement::delete(&self.0, system).map_err(UnitError)
    }

    fn has_been_created<S: super::system::System>(
        &self,
        system: &mut S,
    ) -> Result<bool, Self::HasBeenCreatedError<S>> {
        Requirement::has_been_created(&self.0, system).map_err(UnitError)
    }

    fn affects(&self, other: &Self) -> bool {
        Requirement::affects(&self.0, &other.0)
    }

    fn supports_modifications(&self) -> bool {
        Requirement::supports_modifications(&self.0)
    }

    fn can_undo(&self) -> bool {
        Requirement::can_undo(&self.0)
    }

    fn may_pre_exist(&self) -> bool {
        Requirement::may_pre_exist(&self.0)
    }

    fn verify<S: super::system::System>(&self, system: &mut S) -> Result<bool, ()> {
        Requirement::verify(&self.0, system)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Wrapper<T: RequirementList>(T);

impl<T: RequirementList + Debug> Debug for Wrapper<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl<T: RequirementList + Display> Display for Wrapper<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl<'de, T: RequirementList + DeserializeOwned> Deserialize<'de> for Wrapper<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        T::deserialize(deserializer).map(Wrapper)
    }
}

impl<T: RequirementList + Serialize> Serialize for Wrapper<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        Serialize::serialize(&self.0, serializer)
    }
}

impl<T: RequirementList> Requirement for Wrapper<T> {
    type CreateError<S: super::system::System> = <T as RequirementList>::CreateError<S>;
    type ModifyError<S: super::system::System> = <T as RequirementList>::ModifyError<S>;
    type DeleteError<S: super::system::System> = <T as RequirementList>::DeleteError<S>;
    type HasBeenCreatedError<S: super::system::System> =
        <T as RequirementList>::HasBeenCreatedError<S>;

    fn create<S: super::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        <T as RequirementList>::create(&self.0, system)
    }

    fn modify<S: super::system::System>(&self, system: &mut S) -> Result<(), Self::ModifyError<S>> {
        <T as RequirementList>::modify(&self.0, system)
    }

    fn delete<S: super::system::System>(&self, system: &mut S) -> Result<(), Self::DeleteError<S>> {
        <T as RequirementList>::delete(&self.0, system)
    }

    fn has_been_created<S: super::system::System>(
        &self,
        system: &mut S,
    ) -> Result<bool, Self::HasBeenCreatedError<S>> {
        <T as RequirementList>::has_been_created(&self.0, system)
    }

    fn affects(&self, other: &Self) -> bool {
        <T as RequirementList>::affects(&self.0, &other.0)
    }

    fn supports_modifications(&self) -> bool {
        <T as RequirementList>::supports_modifications(&self.0)
    }

    fn can_undo(&self) -> bool {
        <T as RequirementList>::can_undo(&self.0)
    }

    fn may_pre_exist(&self) -> bool {
        <T as RequirementList>::may_pre_exist(&self.0)
    }

    fn verify<S: super::system::System>(&self, system: &mut S) -> Result<bool, ()> {
        <T as RequirementList>::verify(&self.0, system)
    }

    const NAME: &'static str = "<unused>";
}

impl<A: Requirement> Supports<A> for Unit<A> {
    fn create_from(item: A) -> Self {
        Unit(item)
    }
}

impl<T> Supports<T> for T {
    fn create_from(item: T) -> Self {
        item
    }
}

impl<F, T: RequirementList + Supports<F>> Supports<F> for Wrapper<T>
where
    Cmp<F, Self>: Neq,
{
    fn create_from(item: F) -> Self {
        Wrapper(T::create_from(item))
    }
}

#[cfg(test)]
mod tests {
    use super::{Join, Supports, Unit};
    use crate::requirements::{Requirement, Wrapper};
    use serde::{Deserialize, Serialize};
    use std::fmt::{Debug, Display};

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct Foo {
        x: i32,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct Bar {
        s: String,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct Baz {
        k: (u8, u8, u8),
    }

    impl Display for Foo {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            Debug::fmt(self, f)
        }
    }

    impl Display for Bar {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            Debug::fmt(self, f)
        }
    }

    impl Display for Baz {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            Debug::fmt(self, f)
        }
    }

    impl Requirement for Foo {
        const NAME: &'static str = "foo";

        type CreateError<S: crate::system::System> = std::io::Error;
        type ModifyError<S: crate::system::System> = std::io::Error;
        type DeleteError<S: crate::system::System> = std::io::Error;
        type HasBeenCreatedError<S: crate::system::System> = std::io::Error;

        fn create<S: crate::system::System>(
            &self,
            _system: &mut S,
        ) -> Result<(), Self::CreateError<S>> {
            todo!()
        }
        fn modify<S: crate::system::System>(
            &self,
            _system: &mut S,
        ) -> Result<(), Self::ModifyError<S>> {
            todo!()
        }
        fn delete<S: crate::system::System>(
            &self,
            _system: &mut S,
        ) -> Result<(), Self::DeleteError<S>> {
            todo!()
        }
        fn has_been_created<S: crate::system::System>(
            &self,
            _system: &mut S,
        ) -> Result<bool, Self::HasBeenCreatedError<S>> {
            todo!()
        }
        fn affects(&self, _other: &Self) -> bool {
            todo!()
        }
        fn supports_modifications(&self) -> bool {
            todo!()
        }
        fn can_undo(&self) -> bool {
            todo!()
        }
        fn may_pre_exist(&self) -> bool {
            todo!()
        }
        fn verify<S: crate::system::System>(&self, _system: &mut S) -> Result<bool, ()> {
            todo!()
        }
    }

    impl Requirement for Bar {
        const NAME: &'static str = "bar";

        type CreateError<S: crate::system::System> = std::io::Error;
        type ModifyError<S: crate::system::System> = std::io::Error;
        type DeleteError<S: crate::system::System> = std::io::Error;
        type HasBeenCreatedError<S: crate::system::System> = std::io::Error;

        fn create<S: crate::system::System>(
            &self,
            _system: &mut S,
        ) -> Result<(), Self::CreateError<S>> {
            todo!()
        }
        fn modify<S: crate::system::System>(
            &self,
            _system: &mut S,
        ) -> Result<(), Self::ModifyError<S>> {
            todo!()
        }
        fn delete<S: crate::system::System>(
            &self,
            _system: &mut S,
        ) -> Result<(), Self::DeleteError<S>> {
            todo!()
        }
        fn has_been_created<S: crate::system::System>(
            &self,
            _system: &mut S,
        ) -> Result<bool, Self::HasBeenCreatedError<S>> {
            todo!()
        }
        fn affects(&self, _other: &Self) -> bool {
            todo!()
        }
        fn supports_modifications(&self) -> bool {
            todo!()
        }
        fn can_undo(&self) -> bool {
            todo!()
        }
        fn may_pre_exist(&self) -> bool {
            todo!()
        }
        fn verify<S: crate::system::System>(&self, _system: &mut S) -> Result<bool, ()> {
            todo!()
        }
    }

    impl Requirement for Baz {
        const NAME: &'static str = "baz";

        type CreateError<S: crate::system::System> = std::io::Error;
        type ModifyError<S: crate::system::System> = std::io::Error;
        type DeleteError<S: crate::system::System> = std::io::Error;
        type HasBeenCreatedError<S: crate::system::System> = std::io::Error;

        fn create<S: crate::system::System>(
            &self,
            _system: &mut S,
        ) -> Result<(), Self::CreateError<S>> {
            todo!()
        }
        fn modify<S: crate::system::System>(
            &self,
            _system: &mut S,
        ) -> Result<(), Self::ModifyError<S>> {
            todo!()
        }
        fn delete<S: crate::system::System>(
            &self,
            _system: &mut S,
        ) -> Result<(), Self::DeleteError<S>> {
            todo!()
        }
        fn has_been_created<S: crate::system::System>(
            &self,
            _system: &mut S,
        ) -> Result<bool, Self::HasBeenCreatedError<S>> {
            todo!()
        }
        fn affects(&self, _other: &Self) -> bool {
            todo!()
        }
        fn supports_modifications(&self) -> bool {
            todo!()
        }
        fn can_undo(&self) -> bool {
            todo!()
        }
        fn may_pre_exist(&self) -> bool {
            todo!()
        }
        fn verify<S: crate::system::System>(&self, _system: &mut S) -> Result<bool, ()> {
            todo!()
        }
    }

    #[test]
    pub fn serialize_deserialize() {
        let v: requirements!(Foo, Bar, Baz) =
            Wrapper(Join::Next(Join::Next(Unit(Baz { k: (5, 10, 15) }))));
        println!("{:?}", v);

        let s = serde_json::to_string(&v).unwrap();
        println!("{:?}", s);

        let u: Join<Baz, Join<Foo, Unit<Bar>>> = serde_json::from_str(&s).unwrap();
        println!("{:?}", u);

        let u: Join<Bar, Join<Baz, Unit<Foo>>> = serde_json::from_str(&s).unwrap();
        println!("{:?}", u);

        let u: Join<Foo, Join<Bar, Unit<Baz>>> = serde_json::from_str(&s).unwrap();
        println!("{:?}", u);
    }

    #[test]
    pub fn from() {
        let v: requirements!(Foo, Bar, Baz) = Supports::create_from(Baz { k: (5, 10, 15) });
        assert_eq!(
            v,
            Wrapper(Join::Next(Join::Next(Unit(Baz { k: (5, 10, 15) }))))
        );

        let v: requirements!(Foo, Baz, Bar) = Supports::create_from(Baz { k: (5, 10, 15) });
        assert_eq!(v, Wrapper(Join::Next(Join::Item(Baz { k: (5, 10, 15) }))));

        let v: requirements!(Baz, Bar, Foo) = Supports::create_from(Baz { k: (5, 10, 15) });
        assert_eq!(v, Wrapper(Join::Item(Baz { k: (5, 10, 15) })));
    }
}
