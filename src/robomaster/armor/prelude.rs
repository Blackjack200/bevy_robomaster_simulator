use super::collision::ArmorCollisionPlugin;
pub use crate::robomaster::armor::common::*;
pub use crate::robomaster::armor::construct::*;
pub use crate::robomaster::armor::marker::*;
use bevy::app::plugin_group;

plugin_group! {
    pub struct ArmorPlugins {
        :ArmorConstructorPlugin,
        :ArmorCollisionPlugin
    }
}
