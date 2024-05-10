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

pub mod __impl {
    pub use serde::ser::SerializeMap;
    pub use serde::de::{DeserializeOwned, MapAccess, Visitor};
    pub use serde::{Serialize, Deserialize};
    pub use std::fmt::{Debug, Display};
    pub use std::error::Error;
    pub use super::{Requirement, Supports};
}

#[macro_export]
macro_rules! requirements {
    ($name:ident = $($ty:tt),+ $(,)*) => {
        concat_idents::concat_idents!(
            mod_name = __req, $name {
                mod mod_name {
                    use $crate::requirements::__impl::*;

                    #[derive(Clone, Debug, PartialEq)]
                    pub enum $name {
                        $($ty { val: super::$ty }),+
                    }
        
                    impl Display for $name {
                        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                            match self {
                                $(Self::$ty { val } => Display::fmt(val, f)),+
                            }
                        }
                    }
        
                    #[derive(Debug)]
                    pub enum CreateErrorImpl<S: $crate::system::System> {
                        $($ty(<super::$ty as Requirement>::CreateError<S>)),+
                    }
        
                    impl<S: $crate::system::System> std::fmt::Display for CreateErrorImpl<S> {
                        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                            match self {
                                $(Self::$ty(val) => Display::fmt(val, f)),*
                            }
                        }            
                    }
        
                    impl<S: $crate::system::System> std::error::Error for CreateErrorImpl<S> {
                        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                            match self {
                                $(Self::$ty(val) => std::error::Error::source(val)),*
                            }
                        }
                    }
        
                    #[derive(Debug)]
                    pub enum ModifyErrorImpl<S: $crate::system::System> {
                        $($ty(<super::$ty as Requirement>::ModifyError<S>)),+
                    }
        
                    impl<S: $crate::system::System> std::fmt::Display for ModifyErrorImpl<S> {
                        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                            match self {
                                $(Self::$ty(val) => Display::fmt(val, f)),*
                            }
                        }            
                    }
        
                    impl<S: $crate::system::System> std::error::Error for ModifyErrorImpl<S> {
                        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                            match self {
                                $(Self::$ty(val) => std::error::Error::source(val)),*
                            }
                        }
                    }
        
                    #[derive(Debug)]
                    pub enum DeleteErrorImpl<S: $crate::system::System> {
                        $($ty(<super::$ty as Requirement>::DeleteError<S>)),+
                    }
        
                    impl<S: $crate::system::System> std::fmt::Display for DeleteErrorImpl<S> {
                        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                            match self {
                                $(Self::$ty(val) => Display::fmt(val, f)),*
                            }
                        }            
                    }
        
                    impl<S: $crate::system::System> std::error::Error for DeleteErrorImpl<S> {
                        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                            match self {
                                $(Self::$ty(val) => std::error::Error::source(val)),*
                            }
                        }
                    }
        
                    #[derive(Debug)]
                    pub enum HasBeenCreatedErrorImpl<S: $crate::system::System> {
                        $($ty(<super::$ty as Requirement>::HasBeenCreatedError<S>)),+
                    }
        
                    impl<S: $crate::system::System> std::fmt::Display for HasBeenCreatedErrorImpl<S> {
                        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                            match self {
                                $(Self::$ty(val) => Display::fmt(val, f)),*
                            }
                        }            
                    }
        
                    impl<S: $crate::system::System> std::error::Error for HasBeenCreatedErrorImpl<S> {
                        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                            match self {
                                $(Self::$ty(val) => std::error::Error::source(val)),*
                            }
                        }
                    }
        
                    impl Requirement for $name {
                        type CreateError<S: $crate::system::System> = CreateErrorImpl<S>;
                        type ModifyError<S: $crate::system::System> = ModifyErrorImpl<S>;
                        type DeleteError<S: $crate::system::System> = DeleteErrorImpl<S>;
                        type HasBeenCreatedError<S: $crate::system::System> = HasBeenCreatedErrorImpl<S>;
                    
                        fn create<S: $crate::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
                            match self {
                                $(Self::$ty { val } => Requirement::create(val, system).map_err(CreateErrorImpl::<S>::$ty)),*
                            }
                        }
                    
                        fn modify<S: $crate::system::System>(&self, system: &mut S) -> Result<(), Self::ModifyError<S>> {
                            match self {
                                $(Self::$ty { val } => Requirement::modify(val, system).map_err(ModifyErrorImpl::<S>::$ty)),*
                            }
                        }
                    
                        fn delete<S: $crate::system::System>(&self, system: &mut S) -> Result<(), Self::DeleteError<S>> {
                            match self {
                                $(Self::$ty { val } => Requirement::delete(val, system).map_err(DeleteErrorImpl::<S>::$ty)),*
                            }
                        }
                    
                        fn has_been_created<S: $crate::system::System>(
                            &self,
                            system: &mut S,
                        ) -> Result<bool, Self::HasBeenCreatedError<S>> {
                            match self {
                                $(Self::$ty { val } => Requirement::has_been_created(val, system).map_err(HasBeenCreatedErrorImpl::<S>::$ty)),*
                            }
                        }
                    
                        fn affects(&self, other: &Self) -> bool {
                            match (self, other) {
                                $((Self::$ty { val: a }, Self::$ty { val: b }) => Requirement::affects(a, b)),+,
                                _ => false,
                            }
                        }
                    
                        fn supports_modifications(&self) -> bool {
                            match self {
                                $(Self::$ty { val } => Requirement::supports_modifications(val)),*
                            }
                        }
                    
                        fn can_undo(&self) -> bool {
                            match self {
                                $(Self::$ty { val } => Requirement::can_undo(val)),*
                            }
                        }
                    
                        fn may_pre_exist(&self) -> bool {
                            match self {
                                $(Self::$ty { val } => Requirement::may_pre_exist(val)),*
                            }
                        }
                    
                        fn verify<S: $crate::system::System>(&self, system: &mut S) -> Result<bool, ()> {
                            match self {
                                $(Self::$ty { val } => Requirement::verify(val, system)),*
                            }
                        }
                    
                        const NAME: &'static str = "<unused>";
                    }
        
                    impl Serialize for $name {
                        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                        where
                            S: serde::Serializer,
                        {
                            match self {
                                $(Self::$ty { val } => {
                                    let mut map = serializer.serialize_map(Some(1))?;
                                    map.serialize_entry(super::$ty::NAME, val)?;
                                    map.end()
                                }),*
                            }
                        }
                    }
        
                    #[derive(Copy, Clone)]
                    pub struct ReqVisitor;
        
                    impl Default for ReqVisitor {
                        fn default() -> Self {
                            Self
                        }
                    }
        
                    pub trait DecodeKey {
                        type Output;
        
                        fn decode<'de, M: MapAccess<'de>>(key: &str, access: M) -> Result<Self::Output, M::Error>;
                    }
        
                    impl DecodeKey for ReqVisitor {
                        type Output = $name;
        
                        fn decode<'de, M: MapAccess<'de>>(key: &str, mut access: M) -> Result<Self::Output, M::Error> {
                            Ok(match key {
                                $(<super::$ty as $crate::requirements::Requirement>::NAME => $name::$ty { val: access.next_value()? }),+,
                                _ => panic!("unknown key {}", key),
                            })
                        }
                    }
        
                    impl<'de> Visitor<'de> for ReqVisitor {
                        type Value = $name;
        
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
        
                    impl<'de> Deserialize<'de> for $name {
                        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                        where
                            D: serde::Deserializer<'de>,
                        {
                            deserializer.deserialize_map(ReqVisitor::default())
                        }
                    }
        
                    $(
                        impl Supports<super::$ty> for $name {
                            fn create_from(item: super::$ty) -> Self {
                                Self::$ty { val: item }
                            }
                        }
                    )*
                }
        
                pub use mod_name::$name as $name;
            }
        );
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

pub trait Supports<R> {
    fn create_from(item: R) -> Self;
}

impl<T> Supports<T> for T {
    fn create_from(item: T) -> Self {
        item
    }
}

#[cfg(test)]
mod tests {
    use super::Supports;
    use crate::requirements::Requirement;
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
        requirements!(R = Foo, Bar, Baz);
        let v: R = R::create_from(Baz { k: (5, 10, 15) });
        println!("{:?}", v);

        let s = serde_json::to_string(&v).unwrap();
        println!("{:?}", s);

        requirements!(R1 = Baz, Foo, Bar);
        let u: R1 = serde_json::from_str(&s).unwrap();
        println!("{:?}", u);

        requirements!(R2 = Bar, Baz, Foo);
        let u: R2 = serde_json::from_str(&s).unwrap();
        println!("{:?}", u);

        requirements!(R3 = Foo, Bar, Baz);
        let u: R3 = serde_json::from_str(&s).unwrap();
        println!("{:?}", u);
    }

    #[test]
    pub fn from() {
        requirements!(R = Foo, Bar, Baz);
        let v: R = Supports::create_from(Baz { k: (5, 10, 15) });
        assert_eq!(
            v,
            R::create_from(Baz { k: (5, 10, 15) })
        );

        let v: R = Supports::create_from(Baz { k: (5, 10, 15) });
        assert_eq!(v, R::create_from(Baz { k: (5, 10, 15) }));

        let v: R = Supports::create_from(Baz { k: (5, 10, 15) });
        assert_eq!(v, R::create_from(Baz { k: (5, 10, 15) }));
    }
}
