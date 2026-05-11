use crate::robomaster::power_rune::common::{RuneMode, RuneTransition};
use crate::robomaster::power_rune::rotation::PowerRuneRotation;
use crate::robomaster::power_rune::state::MechanismState;
use crate::robomaster::power_rune::visual::PowerRuneVisuals;
use crate::robomaster::prelude::Team;
use crate::robomaster::visibility::StatefulAppearance;
use bevy::app::Update;
use bevy::prelude::{Component, IntoScheduleConfigs, Query, Res, Time, Transform};

#[derive(Component, Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct PowerRune {
    team: Team,
    mode: RuneMode,
}

#[derive(Component, Debug, Clone, PartialEq)]
pub struct PowerRuneMechanism {
    state: MechanismState,
}

impl PowerRune {
    pub fn new(team: Team, mode: RuneMode) -> Self {
        Self { team, mode }
    }

    pub fn team(&self) -> Team {
        self.team
    }

    pub fn mode(&self) -> RuneMode {
        self.mode
    }
}

impl PowerRuneMechanism {
    pub fn new() -> Self {
        Self {
            state: MechanismState::inactive(),
        }
    }

    pub fn state(&self) -> &MechanismState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut MechanismState {
        &mut self.state
    }
}

impl Default for PowerRuneMechanism {
    fn default() -> Self {
        Self::new()
    }
}

fn sync_rotation_after_transition(
    transition: RuneTransition,
    mode: RuneMode,
    rotation: &mut PowerRuneRotation,
    rng: &mut impl rand::Rng,
) {
    match transition {
        RuneTransition::Started => rotation.begin_activation(mode, rng),
        RuneTransition::Failed | RuneTransition::Activated | RuneTransition::ResetToInactive => {
            rotation.end_activation()
        }
        RuneTransition::None | RuneTransition::Advanced => {}
    }
}

fn rune_activation_tick(
    time: Res<Time>,
    mut runes: Query<(&PowerRune, &mut PowerRuneMechanism, &mut PowerRuneRotation)>,
) {
    let delta_secs = time.delta_secs();
    let mut rng = rand::rng();

    for (rune, mut mechanism, mut rotation) in &mut runes {
        let transition = mechanism.state.tick(rune.mode, delta_secs, &mut rng);
        sync_rotation_after_transition(transition, rune.mode, &mut rotation, &mut rng);
    }
}

fn apply_power_rune_visuals(
    mut runes: Query<(&PowerRune, &PowerRuneMechanism, &mut PowerRuneVisuals)>,
    mut appearance: StatefulAppearance,
) {
    for (rune, mechanism, mut visuals) in &mut runes {
        visuals.apply(rune.mode, mechanism.state(), &mut appearance);
    }
}

fn rune_rotation_system(
    time: Res<Time>,
    mut runes: Query<(&PowerRune, &mut PowerRuneRotation, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (rune, mut rotation, mut transform) in &mut runes {
        rotation.rotate(rune.mode, &mut transform, dt);
    }
}

#[derive(Default)]
pub(super) struct PowerRuneUpdatePlugin;

impl bevy::app::Plugin for PowerRuneUpdatePlugin {
    fn build(&self, app: &mut bevy::app::App) {
        app.add_systems(
            Update,
            (
                rune_activation_tick,
                apply_power_rune_visuals,
                rune_rotation_system,
            )
                .chain(),
        );
    }
}
