use bevy::ecs::system::SystemParam;
use bevy::ecs::system::lifetimeless::Read;
use bevy::prelude::*;

pub struct HierarchyQuery<'w, 's> {
    child_of: Query<'w, 's, Read<ChildOf>>,
    children: Query<'w, 's, Read<Children>>,
    name: Query<'w, 's, Read<Name>, (With<ChildOf>, With<Children>)>,
}

impl<'w, 's> HierarchyQuery<'w, 's> {
    pub fn new(
        child_of: Query<'w, 's, Read<ChildOf>>,
        children: Query<'w, 's, Read<Children>>,
        name: Query<'w, 's, Read<Name>, (With<ChildOf>, With<Children>)>,
    ) -> Self {
        Self {
            child_of,
            children,
            name,
        }
    }
}

impl<'w, 's> HierarchyQuery<'w, 's> {
    pub fn query<'q>(&'q self, root: Entity) -> Hierarchy<'q, 'w, 's> {
        Hierarchy::Prologue::<'q, 'w, 's> {
            current: root,
            param: self,
        }
    }
}

pub enum Hierarchy<'q, 'w, 's> {
    Prologue {
        current: Entity,
        param: &'q HierarchyQuery<'w, 's>,
    },
    Epilogue,
}

macro_rules! impl_hierarchy {
    ($method_name:ident,$method:ident $($prefix:tt)?) => {
        #[must_use]
        #[inline]
        pub fn $method_name<T: AsRef<str>>(self, $method_name: T) -> Hierarchy<'q, 'w, 's> {
            match self {
                Hierarchy::Prologue { current, param } => {
                    let Ok(children) = param.children.get(current) else {
                        return Hierarchy::Epilogue;
                    };
                    let $method_name = $method_name.as_ref();
                    for &child in children {
                        let Ok(name) = param.name.get(child) else {
                            continue;
                        };
                        if $($prefix)* name.as_ref().$method($method_name) {
                            return Hierarchy::Prologue {
                                current: child,
                                param,
                            };
                        }
                    }
                    Hierarchy::Epilogue
                }
                Hierarchy::Epilogue => self,
            }
        }
    };
}

impl<'q, 'w, 's> Hierarchy<'q, 'w, 's> {
    impl_hierarchy!(suffix, ends_with);
    impl_hierarchy!(prefix, starts_with);
    impl_hierarchy!(exact, eq);
    impl_hierarchy!(with, contains);
    impl_hierarchy!(without, contains !);

    pub fn try_match<F>(self, f: F) -> Self
    where
        F: Fn(&str) -> bool,
    {
        match self {
            Hierarchy::Prologue { current, param } => {
                let Ok(children) = param.children.get(current) else {
                    return Hierarchy::Epilogue;
                };
                for &child in children {
                    let Ok(name) = param.name.get(child) else {
                        return Hierarchy::Epilogue;
                    };
                    if f(name.as_ref()) {
                        return Hierarchy::Prologue {
                            current: child,
                            param,
                        };
                    }
                }
                Hierarchy::Epilogue
            }
            Hierarchy::Epilogue => self,
        }
    }

    pub fn decay(self) -> Option<Entity> {
        self.into()
    }
}

impl<'q, 'w, 's> Iterator for Hierarchy<'q, 'w, 's> {
    type Item = Entity;

    fn next(&mut self) -> Option<Self::Item> {
        match std::mem::replace(self, Hierarchy::Epilogue) {
            Hierarchy::Prologue { current, .. } => Some(current),
            Hierarchy::Epilogue => None,
        }
    }
}

impl<'q, 'w, 's> From<Hierarchy<'q, 'w, 's>> for Option<Entity> {
    fn from(value: Hierarchy<'q, 'w, 's>) -> Self {
        match value {
            Hierarchy::Prologue { current, .. } => Some(current),
            Hierarchy::Epilogue => None,
        }
    }
}
