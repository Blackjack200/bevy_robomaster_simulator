use crate::util::either::Either;
use bevy::ecs::system::SystemParam;
use bevy::ecs::system::lifetimeless::Read;
use bevy::prelude::*;
use exact::{ExactExt, ExactOneExt};
use std::iter::{Empty, empty};

#[macro_export]
macro_rules! query {
    ($query: expr, $($tt:tt)*) => {
        $crate::query!(@internal $query.clone(), $($tt)*)
    };
    (nocopy $query: expr, $($tt:tt)*) => {
        $crate::query!(@internal $query, $($tt)*)
    };
    (@internal $query: expr, ..$ident:literal $($tt:tt)*) => {
        $crate::query!(@internal $query.suffix($ident) $($tt)*)
    };
    (@internal $query: expr, $ident:literal.. $($tt:tt)*) => {
        $crate::query!(@internal $query.prefix($ident) $($tt)*)
    };
    (@internal $query: expr, $ident:literal $($tt:tt)*) => {
        $crate::query!(@internal $query.exact($ident) $($tt)*)
    };
    (@internal $query: expr, ... $($tt:tt)*) => {
        $crate::query!(@internal $query.any() $($tt)*)
    };
    (@internal $query: expr) => { $query.one() };
    (@internal $query: expr, ) => { $query.one() };
    (@internal $query: expr, ref) => { $query };
}

#[derive(SystemParam)]
pub struct HierarchyQuery<'w, 's> {
    child_of: Query<'w, 's, Read<ChildOf>>,
    children: Query<'w, 's, Read<Children>>,
    name: Query<'w, 's, Read<Name>, With<ChildOf>>,
}

impl<'w, 's> HierarchyQuery<'w, 's> {
    pub fn new(
        child_of: Query<'w, 's, Read<ChildOf>>,
        children: Query<'w, 's, Read<Children>>,
        name: Query<'w, 's, Read<Name>, With<ChildOf>>,
    ) -> Self {
        Self {
            child_of,
            children,
            name,
        }
    }
}

pub trait HierarchyIter: Iterator<Item = Entity> + Clone {}

impl<I: Iterator<Item = Entity> + Clone> HierarchyIter for I {}

impl<'w, 's> HierarchyQuery<'w, 's> {
    pub fn of<'q>(&'q self, root: Entity) -> Hierarchy<'q, 'w, 's, impl HierarchyIter> {
        Hierarchy::Prologue::<'q, 'w, 's> {
            lazy: vec![root].into_iter(),
            param: self,
        }
    }
}

#[derive(Copy, Clone)]
pub enum Hierarchy<'q, 'w, 's, IterType: HierarchyIter>
where
    's: 'q,
    'w: 's,
{
    Prologue {
        lazy: IterType,
        param: &'q HierarchyQuery<'w, 's>,
    },
    Epilogue,
}

macro_rules! impl_hierarchy {
    ($v:vis $method_name:ident,$method:ident $($prefix:tt)*) => {
        #[must_use]
        #[inline]
        $v fn $method_name<T: Into<&'q str>>(
            self,
            suffix: T,
        ) -> Hierarchy<'q, 'w, 's, impl HierarchyIter> {
            match self {
                Hierarchy::Prologue { lazy, param } => {
                    let _suffix = suffix.into();
                    let flatten = lazy
                        .filter_map(|current| {
                            param
                                .children
                                .get(current)
                                .ok()
                                .map(|children| children.into_iter())
                        })
                        .flatten()
                        .copied()
                        .filter(move |&child| {
                            if let Ok(_name) = param.name.get(child) {
                                $($prefix)* _name.as_ref().$method(_suffix)
                            } else {
                                false
                            }
                        });
                    Hierarchy::Prologue {
                        lazy: flatten,
                        param,
                    }
                }
                Hierarchy::Epilogue => Hierarchy::Epilogue,
            }
        }
    };
}

impl<'q, 'w, 's, IterType: HierarchyIter> Hierarchy<'q, 'w, 's, IterType> {
    impl_hierarchy!(pub suffix, ends_with);
    impl_hierarchy!(pub prefix, starts_with);
    impl_hierarchy!(pub exact, eq);
    impl_hierarchy!(pub with, contains);
    impl_hierarchy!(pub without, contains !);
    // literally a hack lol
    impl_hierarchy!(_any, eq return true;#[allow(unreachable_code)]);

    pub fn any(self) -> Hierarchy<'q, 'w, 's, impl HierarchyIter> {
        self._any("")
    }

    pub fn one(self) -> Option<Entity> {
        match self {
            Hierarchy::Prologue { lazy, .. } => lazy.exact::<1>().ok().into_single(),
            Hierarchy::Epilogue => None,
        }
    }
}

impl<'q, 'w, 's, IterType: HierarchyIter> IntoIterator for Hierarchy<'q, 'w, 's, IterType>
where
    IterType: Iterator<Item = Entity>,
{
    type Item = Entity;
    type IntoIter = Either<Empty<Entity>, IterType>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            Hierarchy::Prologue { lazy, .. } => Either::Right(lazy),
            Hierarchy::Epilogue => Either::Left(empty()),
        }
    }
}
