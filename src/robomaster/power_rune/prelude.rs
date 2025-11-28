use crate::robomaster::power_rune::collision::PowerRuneCollisionPlugin;
use crate::robomaster::power_rune::construct::PowerRuneConstructorPlugin;
use crate::robomaster::power_rune::rune::PowerRuneUpdatePlugin;
use bevy::app::plugin_group;

pub use crate::robomaster::power_rune::collision::*;
pub use crate::robomaster::power_rune::construct::*;
pub use crate::robomaster::power_rune::rune::*;

plugin_group! {
    #[derive(Default)]
    pub struct PowerRunePlugins {
        :PowerRuneConstructorPlugin,
        :PowerRuneCollisionPlugin,
        :PowerRuneUpdatePlugin,
    }
}
