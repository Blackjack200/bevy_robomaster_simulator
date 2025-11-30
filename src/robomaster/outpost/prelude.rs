use crate::robomaster::outpost::construct::OutpostConstructorPlugin;
use bevy::app::plugin_group;

pub use crate::robomaster::outpost::construct::*;
use crate::robomaster::outpost::update::OutpostUpdatePlugin;

plugin_group! {
    #[derive(Default)]
    pub struct OutpostPlugins {
        :OutpostConstructorPlugin,
        :OutpostUpdatePlugin,
    }
}
