use crate::robomaster::{armor, outpost, power_rune, tech_core};
use bevy::app::App;
use bevy::prelude::Plugin;

pub use crate::robomaster::common::*;
use crate::robomaster::outpost::prelude::OutpostPlugins;
use crate::robomaster::tech_core::prelude::TechCorePlugins;
use crate::robomaster::visibility::StatefulAppearancePlugin;
pub use armor::prelude::*;
pub use outpost::prelude::*;
pub use power_rune::prelude::*;
pub use tech_core::prelude::*;

#[derive(Default)]
pub struct RoboMasterPlugins;
impl Plugin for RoboMasterPlugins {
    fn build(&self, app: &mut App) {
        app.add_plugins(StatefulAppearancePlugin)
            .add_plugins(ArmorPlugins)
            .add_plugins(PowerRunePlugins)
            .add_plugins(OutpostPlugins)
            .add_plugins(TechCorePlugins);
    }
}

#[macro_export]
macro_rules! entity_root {
    (
        super $child_of:expr => $children:expr;
        name $name:expr;
        $root:ident {
            $($expr:tt)*
        }
    ) => {{
        let _child_of = &$child_of;
        let _children = &$children;
        let _name = &$name;
        let _root = $root;
        $crate::entity_root!(@internal _root, _name, _child_of, _children, { $($expr)* });
    }};

    (@match $root:expr, $name:ident, $child_of:ident, $children:ident,
            $name_str:ident,
            $label:expr => $ident:ident {$($tt:tt)*}; $($rest:tt)*
    )=>{
        if $name_str == $label {
            let $ident = $root;
            $crate::entity_root!(@internal $ident, $name, $child_of, $children, {$($tt)*});
            continue;
        }
        $crate::entity_root!(@match $root, $name, $child_of, $children, $name_str, $($rest)*);
    };

    (@match $root:expr, $name:ident, $child_of:ident, $children:ident,
            $name_str:ident,
            :$label:literal => $ident:ident {$($tt:tt)*}; $($rest:tt)*
    )=>{
        if $name_str.ends_with(&$label) {
            let $ident = $root;
            $crate::entity_root!(@internal $ident, $name, $child_of, $children, {$($tt)*});
            continue;
        }
        $crate::entity_root!(@match $root, $name, $child_of, $children, $name_str, $($rest)*);
    };

    (@match $root:expr, $name:ident, $child_of:ident, $children:ident,
            $name_str:ident,
            $label:literal: => $ident:ident {$($tt:tt)*}; $($rest:tt)*
    )=>{
        if $name_str.starts_with(&$label) {
            let $ident = $root;
            $crate::entity_root!(@internal $ident, $name, $child_of, $children, {$($tt)*});
            continue;
        }
        $crate::entity_root!(@match $root, $name, $child_of, $children, $name_str, $($rest)*);
    };

    (@match $root:expr, $name:ident, $child_of:ident, $children:ident,
            $name_str:ident,
            :$label:literal: => $ident:ident {$($tt:tt)*}; $($rest:tt)*
    )=>{
        if $name_str.contains(&$label) {
            let $ident = $root;
            $crate::entity_root!(@internal $ident, $name, $child_of, $children, {$($tt)*});
            continue;
        }
        $crate::entity_root!(@match $root, $name, $child_of, $children, $name_str, $($rest)*);
    };

    (@internal $root:expr, $name:ident, $child_of:ident, $children:ident, {
        match {
            $($rest:tt)*
        }
    }) => {{
        if let Ok(children) = $children.get($root) {
            for &child in children.iter() {
                let Ok(name) = $name.get(child) else { continue; };
                let name_str = name.as_str();
                $crate::entity_root!(@match child, $name, $child_of, $children, name_str, $($rest)*);
            }
        }
    }};

    (@internal $root:expr, $name:ident, $child_of:ident, $children:ident, {
        $($stmt:stmt);* $(;)?
    }) => {{
        let _ = $root;
        $($stmt)*
    }};

    (@internal $root:expr, $name:ident, $child_of:ident, $children:ident, $(;)?) => {};


    (@match $root:expr, $name:ident, $child_of:ident, $children:ident,
            $name_str:ident,
            $(;)?
    )=>{};
}
