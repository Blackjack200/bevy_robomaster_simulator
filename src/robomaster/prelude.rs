use crate::robomaster::power_rune::PowerRunePlugin;
use crate::robomaster::visibility::StatefulAppearancePlugin;
use bevy::app::plugin_group;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Team {
    Red,
    Blue,
}

plugin_group! {
    pub struct RoboMasterPlugins {
        :PowerRunePlugin,
        :StatefulAppearancePlugin,
    }
}
