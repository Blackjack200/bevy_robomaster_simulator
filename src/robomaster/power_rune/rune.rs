use crate::robomaster::power_rune::common::RuneMode;
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
    pub fn new(mode: RuneMode) -> Self {
        Self {
            state: MechanismState::inactive(mode),
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
        Self::new(RuneMode::Small)
    }
}

fn rune_activation_tick(
    time: Res<Time>,
    mut runes: Query<(&mut PowerRuneMechanism, &mut PowerRuneRotation)>,
) {
    let delta_secs = time.delta_secs();
    let mut rng = rand::rng();

    for (mut mechanism, mut rotation) in &mut runes {
        mechanism.state.tick(delta_secs, &mut rng);
        rotation.sync_activation(
            mechanism.state.mode(),
            mechanism.state.is_activating(),
            &mut rng,
        );
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
