use bevy::prelude::*;

use crate::robomaster::prelude::{RobotConfig, Team};

#[derive(Component)]
pub struct Controlled;

#[derive(Component)]
pub struct Infantry(pub Team, pub RobotConfig);

#[derive(Component, Default)]
pub struct InfantryChassis {
    pub yaw: f32,
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
