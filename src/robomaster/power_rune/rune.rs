use crate::robomaster::power_rune::common::{RuneAction, RuneMode};
use crate::robomaster::power_rune::consts::*;
use crate::robomaster::power_rune::rotation::RotationController;
use crate::robomaster::power_rune::state::{ActivatingState, MechanismState};
use crate::robomaster::power_rune::visual::RuneVisual;
use crate::robomaster::prelude::Team;
use crate::robomaster::visibility::{Activation, Control, Controller, StatefulAppearance};
use bevy::app::Update;
use bevy::prelude::{Component, Query, Res, Time, Timer, TimerMode, Transform};
use rand::Rng;
use std::hash::{Hash, Hasher};

#[derive(Component)]
pub struct PowerRune {
    team: Team,
    mode: RuneMode,
    r: Controller,
    pub(super) state: MechanismState,
    targets: Vec<RuneVisual>,
    rotation: RotationController,
}

impl Hash for PowerRune {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.team.hash(state);
        self.mode.hash(state);
    }
}

impl PartialEq for PowerRune {
    fn eq(&self, other: &Self) -> bool {
        self.team == other.team && self.mode == other.mode
    }
}

impl Eq for PowerRune {}

impl PowerRune {
    pub fn team(&self) -> Team {
        self.team
    }
    pub fn mode(&self) -> RuneMode {
        self.mode
    }

    pub fn state(&self) -> &MechanismState {
        &self.state
    }

    pub fn rotation_controller(&self) -> &RotationController {
        &self.rotation
    }

    pub fn activating_targets(&self) -> Option<&[Activation]> {
        match &self.state {
            MechanismState::Activating(s) => Some(s.targets()),
            _ => None,
        }
    }

    pub(super) fn new(
        team: Team,
        mode: RuneMode,
        r: Controller,
        targets: Vec<RuneVisual>,
        clockwise: bool,
    ) -> Self {
        Self {
            team,
            mode,
            r,
            state: MechanismState::Inactive {
                wait: Timer::from_seconds(INACTIVE_WAIT, TimerMode::Once),
            },
            targets,
            rotation: RotationController::new(clockwise),
        }
    }

    pub(super) fn handle_action(
        &mut self,
        rng: &mut impl Rng,
        action: RuneAction,
        appearance: &mut StatefulAppearance,
    ) {
        match action {
            RuneAction::StartActivating => {
                // 重新创建旋转控制器确保参数完全重置
                self.rotation.clear_variable();
                self.r.set(Activation::Activated, appearance);
                // 大机关激活时使用变量旋转，小机关使用固定速度
                if self.mode == RuneMode::Large {
                    self.rotation.set_variable(rng);
                }
                let mut state = ActivatingState::new(
                    self.mode,
                    vec![Activation::Deactivated; self.targets.len()],
                );
                for act in state.start() {
                    self.handle_action(rng, act, appearance);
                }
                self.state = MechanismState::Activating(state);
            }
            RuneAction::Failure => {
                self.reset_all_targets(Activation::Deactivated, appearance);
                self.state = MechanismState::Failed {
                    wait: Timer::from_seconds(FAILURE_RECOVER, TimerMode::Once),
                };
            }
            RuneAction::ResetToInactive => {
                self.reset_all_targets(Activation::Deactivated, appearance);
                self.rotation.clear_variable();
                self.state = MechanismState::Inactive {
                    wait: Timer::from_seconds(INACTIVE_WAIT, TimerMode::Once),
                };
            }
            RuneAction::SetAppearance(idx, activation) => {
                self.targets[idx].apply(self.mode, activation, appearance);
            }
            RuneAction::PartialActivate(idx) => {
                let MechanismState::Activating(_) = self.state else {
                    panic!("cannot partially activate a rune in {:?}", self.state);
                };
                self.handle_action(
                    rng,
                    RuneAction::SetAppearance(idx, Activation::Activated),
                    appearance,
                );
            }
            RuneAction::FullActivate(last) => {
                let MechanismState::Activating(_) = self.state else {
                    panic!("cannot activate a rune in {:?}", self.state);
                };
                self.handle_action(rng, RuneAction::PartialActivate(last), appearance);
                self.reset_all_targets(Activation::Completed, appearance);
                // 激活状态下停止旋转
                self.rotation.clear_variable();
                self.state = MechanismState::Activated {
                    wait: Timer::from_seconds(ACTIVATED_HOLD, TimerMode::Once),
                };
            }
        }
    }

    fn reset_all_targets(&mut self, activation: Activation, appearance: &mut StatefulAppearance) {
        self.r.set(activation, appearance);
        for target in &mut self.targets {
            target.apply(self.mode, activation, appearance);
        }
    }
}

fn rune_activation_tick(
    time: Res<Time>,
    mut runes: Query<&mut PowerRune>,
    mut appearance: StatefulAppearance,
) {
    let delta = time.delta();
    let mut rng = rand::rng();
    for mut rune in &mut runes {
        let action = match &mut rune.state {
            MechanismState::Inactive { wait } => {
                if wait.tick(delta).just_finished() {
                    Some(RuneAction::StartActivating)
                } else {
                    None
                }
            }
            MechanismState::Activating(state) => {
                if let Some(action) = state.tick(delta) {
                    for action in action {
                        rune.handle_action(&mut rng, action, &mut appearance);
                    }
                }
                None
            }
            MechanismState::Activated { wait } => {
                if wait.tick(delta).just_finished() {
                    Some(RuneAction::ResetToInactive)
                } else {
                    None
                }
            }
            MechanismState::Failed { wait } => {
                if wait.tick(delta).just_finished() {
                    Some(RuneAction::ResetToInactive)
                } else {
                    None
                }
            }
        };

        if let Some(action) = action {
            rune.handle_action(&mut rng, action, &mut appearance);
        }
    }
}

fn rune_rotation_system(time: Res<Time>, mut runes: Query<(&mut Transform, &mut PowerRune)>) {
    let dt = time.delta_secs();
    for (mut transform, mut rune) in &mut runes {
        let mode = rune.mode;
        // 只有在激活状态下大机关才使用变量旋转
        let speed = rune.rotation.current_speed(mode, dt);
        let angle = speed * dt;

        // 确保旋转方向正确：红方顺时针(正角)，蓝方逆时针(负角)
        rune.rotation.rotate(&mut transform, angle);
    }
}

#[derive(Default)]
pub(super) struct PowerRuneUpdatePlugin;

impl bevy::app::Plugin for PowerRuneUpdatePlugin {
    fn build(&self, app: &mut bevy::app::App) {
        app.add_systems(Update, (rune_activation_tick, rune_rotation_system));
    }
}
