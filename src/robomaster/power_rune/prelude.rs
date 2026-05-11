use crate::robomaster::power_rune::collision::PowerRuneCollisionPlugin;
use crate::robomaster::power_rune::construct::PowerRuneConstructorPlugin;
use crate::robomaster::power_rune::rune::PowerRuneUpdatePlugin;
use bevy::app::plugin_group;

pub use crate::robomaster::power_rune::collision::*;
pub use crate::robomaster::power_rune::common::*;
pub use crate::robomaster::power_rune::construct::*;
pub use crate::robomaster::power_rune::rotation::*;
pub use crate::robomaster::power_rune::rune::*;
pub use crate::robomaster::power_rune::state::*;
pub use crate::robomaster::visibility::Activation;

plugin_group! {
    #[derive(Default)]
    pub struct PowerRunePlugins {
        :PowerRuneConstructorPlugin,
        :PowerRuneCollisionPlugin,
        :PowerRuneUpdatePlugin,
    }
}
