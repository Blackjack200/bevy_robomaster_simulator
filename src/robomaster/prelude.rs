use crate::robomaster::{armor, outpost, power_rune};
use bevy::app::App;
use bevy::prelude::Plugin;

pub use crate::robomaster::common::*;
use crate::robomaster::outpost::prelude::OutpostPlugins;
use crate::robomaster::visibility::StatefulAppearancePlugin;
pub use armor::prelude::*;
pub use outpost::prelude::*;
pub use power_rune::prelude::*;

#[derive(Default)]
pub struct RoboMasterPlugins;
impl Plugin for RoboMasterPlugins {
    fn build(&self, app: &mut App) {
        app.add_plugins(StatefulAppearancePlugin)
            .add_plugins(PowerRunePlugins)
            .add_plugins(OutpostPlugins);
    }
}
