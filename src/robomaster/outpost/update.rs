use crate::robomaster::outpost::rotation::{RotationController, RotationDirection, RotationMode};
use crate::robomaster::prelude::Team;
use bevy::app::Update;
use bevy::log::info;
use bevy::prelude::{
    ButtonInput, Component, IntoScheduleConfigs, KeyCode, Query, Res, ResMut, Resource, Time,
    Transform,
};
use std::hash::{Hash, Hasher};

#[derive(Component)]
pub struct Outpost {
    team: Team,
}

impl Hash for Outpost {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.team.hash(state);
    }
}

impl PartialEq for Outpost {
    fn eq(&self, other: &Self) -> bool {
        self.team == other.team
    }
}

impl Eq for Outpost {}

impl Outpost {
    pub fn team(&self) -> Team {
        self.team
    }

    pub(super) fn new(team: Team) -> Self {
        Self { team }
    }
}

#[derive(Component)]
pub struct OutpostRotator {
    rotation: RotationController,
}

impl OutpostRotator {
    pub(crate) fn new(direction: RotationDirection) -> Self {
        Self {
            rotation: RotationController::new(direction),
        }
    }
}

#[derive(Resource, Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
struct OutpostRotationMode(RotationMode);

fn debug_cycle_outpost_rotation(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<OutpostRotationMode>,
) {
    if !(keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight))
        || !keyboard.just_pressed(KeyCode::KeyC)
    {
        return;
    }

    mode.0 = mode.0.next();
    info!("Outpost rotation mode: {:?}", mode.0);
}

fn outpost_rotation_system(
    time: Res<Time>,
    mode: Res<OutpostRotationMode>,
    mut outposts: Query<(&mut Transform, &OutpostRotator)>,
) {
    let dt = time.delta_secs();
    for (mut transform, outpost) in &mut outposts {
        outpost.rotation.step(&mut transform, dt, mode.0);
    }
}

#[derive(Default)]
pub(super) struct OutpostUpdatePlugin;

impl bevy::app::Plugin for OutpostUpdatePlugin {
    fn build(&self, app: &mut bevy::app::App) {
        app.init_resource::<OutpostRotationMode>().add_systems(
            Update,
            (debug_cycle_outpost_rotation, outpost_rotation_system).chain(),
        );
    }
}
