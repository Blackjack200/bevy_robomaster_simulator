use bevy::prelude::*;

use crate::robomaster::prelude::{RobotConfig, Team};

#[derive(Component)]
pub struct Controlled;

#[derive(Component)]
pub struct Infantry {
    pub team: Team,
    pub config: RobotConfig,
}

impl Infantry {
    pub const fn new(team: Team, config: RobotConfig) -> Self {
        Self { team, config }
    }
}

#[derive(Component, Default)]
pub struct InfantryChassis {
    pub yaw: f32,
    pub yaw_velocity: f32,
}

#[derive(Component, Default)]
pub struct InfantryGimbal {
    pub local_yaw: f32,
    pub pitch: f32,
}

#[derive(Component)]
pub struct InfantryViewOffset;

#[derive(Component)]
pub struct InfantryLaunchOffset;

#[derive(Component)]
pub struct SlapperInfantry;

/// Marker for the currently active (controlled) SlapperInfantry
#[derive(Component)]
pub struct ActiveSlapper;
