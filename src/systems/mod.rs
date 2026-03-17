mod camera;
mod chassis_observation;
mod debug;
mod input;
mod projectile;
mod uav;
pub use camera::*;
pub use chassis_observation::*;
pub use debug::*;
pub use input::*;
pub use projectile::*;
pub use uav::*;

use bevy::prelude::*;

#[derive(SystemSet, Clone, PartialEq, Eq, Hash, Debug)]
pub enum GameplaySystems {
    Input,
    GameLogic,
    Camera,
    Cleanup,
}
