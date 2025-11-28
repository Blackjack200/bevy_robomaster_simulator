use crate::robomaster::{common, power_rune};
use bevy::app::App;
use bevy::prelude::Plugin;

use crate::robomaster::visibility::StatefulAppearancePlugin;
pub use common::*;
pub use power_rune::prelude::*;

#[derive(Default)]
pub struct RoboMasterPlugins;
impl Plugin for RoboMasterPlugins {
    fn build(&self, app: &mut App) {
        app.add_plugins(StatefulAppearancePlugin)
            .add_plugins(PowerRunePlugins);
    }
}
