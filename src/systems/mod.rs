mod camera;
mod debug;
mod input;
mod projectile;

pub use camera::*;
pub use debug::*;
pub use input::*;
pub use projectile::*;

use bevy::prelude::*;

#[derive(SystemSet, Clone, PartialEq, Eq, Hash, Debug)]
pub enum GameplaySystems {
    Input,
    GameLogic,
    Camera,
    Cleanup,
}
