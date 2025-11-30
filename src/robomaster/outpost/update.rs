use crate::robomaster::outpost::rotation::RotationController;
use crate::robomaster::prelude::Team;
use bevy::app::Update;
use bevy::prelude::{Component, Query, Res, Time, Transform};
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
    pub(crate) fn new(clockwise: bool) -> Self {
        Self {
            rotation: RotationController::new(clockwise),
        }
    }
}

fn outpost_rotation_system(
    time: Res<Time>,
    mut outposts: Query<(&mut Transform, &OutpostRotator)>,
) {
    let dt = time.delta_secs();
    for (mut transform, outpost) in &mut outposts {
        outpost.rotation.step(&mut transform, dt);
    }
}

#[derive(Default)]
pub(super) struct OutpostUpdatePlugin;

impl bevy::app::Plugin for OutpostUpdatePlugin {
    fn build(&self, app: &mut bevy::app::App) {
        app.add_systems(Update, outpost_rotation_system);
    }
}
