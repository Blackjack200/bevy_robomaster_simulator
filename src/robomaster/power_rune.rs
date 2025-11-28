use std::collections::HashMap;

use crate::robomaster::prelude::Team;
use crate::robomaster::visibility::{
    Activation, Control, Controller, StatefulAppearance, StatefulAppearanceCreator,
};
use crate::util::bevy::{drain_entities_by, insert_all_child};
use crate::{material, visibility};
use avian3d::prelude::{CollisionEventsEnabled, CollisionStart};
use bevy::prelude::{GizmoAsset, Visibility};
use bevy::{
    app::Update,
    asset::Assets,
    ecs::{
        component::Component,
        entity::Entity,
        event::EntityEvent,
        hierarchy::Children,
        name::Name,
        observer::On,
        query::With,
        system::{Commands, Query, Res, ResMut, SystemParam},
    },
    math::Dir3,
    scene::{SceneInstanceReady, SceneSpawner},
    time::{Time, Timer, TimerMode},
    transform::components::Transform,
};
use rand::{seq::SliceRandom, Rng};

#[derive(Component)]
#[require(CollisionEventsEnabled)]
pub struct Projectile;

#[derive(Component)]
pub struct PowerRuneRoot;

#[derive(Debug, PartialEq, Eq)]
enum RuneAction {
    StartActivating,
    NewRound,
    Failure,
    ResetToInactive,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RuneMode {
    Small,
    Large,
}

#[derive(Debug, Clone)]
enum MechanismState {
    Inactive { wait: Timer },
    Activating(ActivatingState),
    Activated { wait: Timer },
    Failed { wait: Timer },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationWindow {
    Primary,
    Secondary,
}

#[derive(Component, Clone)]
pub struct RuneIndex(pub usize, pub Entity);

pub struct RuneData {
    visual: RuneVisual,
    pub state: Activation,
    pub applied_state: Activation,
}

struct RuneVisual {
    target: Controller,
    legging_segments: [Controller; 3],
    padding_segments: Controller,
    progress_segments: Controller,
}

impl RuneVisual {
    pub fn apply(
        &mut self,
        mode: RuneMode,
        activation: Activation,
        appearance: &mut StatefulAppearance,
    ) {
        match mode {
            RuneMode::Small => {
                self.target.set(activation, appearance);
                for swap in &mut self.legging_segments {
                    swap.set(activation, appearance);
                }
            }
            RuneMode::Large => {
                self.target.set(
                    match activation {
                        Activation::Activated => Activation::Deactivated,
                        _ => activation,
                    },
                    appearance,
                );
                match activation {
                    Activation::Activated => self.legging_segments[0].set(activation, appearance),
                    _ => {
                        for legging in &mut self.legging_segments {
                            legging.set(activation, appearance);
                        }
                    }
                }
            }
        }

        self.padding_segments.set(activation, appearance);
        self.progress_segments.set(activation, appearance);
    }
}

#[derive(Debug, Clone)]
struct ActivatingState {
    highlighted: Vec<usize>,
    hit_flags: Vec<bool>,
    window: ActivationWindow,
    timeout: Timer,
    global_timeout: Timer, // 20秒全局激活超时
}

struct VariableRotation {
    a: f32,
    omega: f32,
    t: f32,
}

impl VariableRotation {
    fn random(rng: &mut impl Rng) -> Self {
        let a = rng.random_range(0.780..=1.045);
        let omega = rng.random_range(1.884..=2.0);
        Self { a, omega, t: 0.0 }
    }

    fn advance(&mut self, dt: f32) -> f32 {
        self.t += dt;
        self.speed()
    }

    fn speed(&self) -> f32 {
        let b = 2.090 - self.a;
        self.a * (self.omega * self.t).sin() + b
    }
}

const ACTIVATION_PRIMARY_TIMEOUT: f32 = 2.5;
const LARGE_SECONDARY_TIMEOUT: f32 = 1.0;
const INACTIVE_WAIT: f32 = 1.0;
const FAILURE_RECOVER: f32 = 1.5;
const ACTIVATED_HOLD: f32 = 6.0;
const ACTIVATION_GLOBAL_TIMEOUT: f32 = 20.0; // 20秒全局激活超时
const ROTATION_BASELINE_SMALL: f32 = std::f32::consts::PI / 3.0; // 小机关固定角速度

struct RotationController {
    baseline: f32,
    direction: Dir3,
    variable: Option<VariableRotation>,
    clockwise: bool,
}

impl RotationController {
    fn new(clockwise: bool) -> Self {
        Self {
            baseline: ROTATION_BASELINE_SMALL,
            direction: Dir3::from_xyz(-1.0, 0.0, -1.0).unwrap(),
            variable: None,
            clockwise,
        }
    }

    fn reset_variable(&mut self, rng: &mut impl Rng) {
        self.variable = Some(VariableRotation::random(rng));
        // 确保重置时间参数
        if let Some(ref mut variable) = self.variable {
            variable.t = 0.0;
        }
    }

    fn clear_variable(&mut self) {
        self.variable = None;
    }

    fn current_speed(&mut self, mode: RuneMode, dt: f32) -> f32 {
        let sgn = if self.clockwise { 1.0 } else { -1.0 };
        if mode == RuneMode::Small {
            return sgn * self.baseline;
        }
        // 大机关只有在激活状态下使用变量旋转
        if let Some(variable) = &mut self.variable {
            variable.advance(dt);
            return sgn * variable.speed();
        }
        sgn * self.baseline
    }
}

const FUNNY: bool = true;

#[derive(Component)]
pub struct PowerRune {
    pub _team: Team,
    pub mode: RuneMode,
    r: Controller,
    state: MechanismState,
    pub targets: Vec<RuneData>,
    rotation: RotationController,
}

pub struct HitResult {
    pub accurate: bool,
    pub change_state: bool,
}

impl PowerRune {
    fn new(
        team: Team,
        mode: RuneMode,
        r: Controller,
        targets: Vec<RuneData>,
        clockwise: bool,
    ) -> Self {
        Self {
            _team: team,
            mode,
            r,
            state: MechanismState::Inactive {
                wait: Timer::from_seconds(INACTIVE_WAIT, TimerMode::Once),
            },
            targets,
            rotation: RotationController::new(clockwise),
        }
    }

    fn available_targets(&self) -> Vec<usize> {
        self.targets
            .iter()
            .enumerate()
            .filter_map(|(idx, target)| {
                if matches!(target.state, Activation::Activated) {
                    None
                } else {
                    Some(idx)
                }
            })
            .collect()
    }

    fn build_new_round(&mut self, rng: &mut impl Rng) -> Option<ActivatingState> {
        let mut available = self.available_targets();
        if available.is_empty() {
            return None;
        }
        available.shuffle(rng);
        let required = match self.mode {
            RuneMode::Small => 1,
            RuneMode::Large => 2,
        };
        let count = required.min(available.len());
        let selection: Vec<usize> = available.into_iter().take(count).collect();

        for target in &mut self.targets {
            if !matches!(target.state, Activation::Activated) {
                target.state = Activation::Deactivated;
            }
        }
        for &idx in &selection {
            self.targets[idx].state = Activation::Activating;
        }

        Some(ActivatingState {
            highlighted: selection,
            hit_flags: vec![false; count],
            window: ActivationWindow::Primary,
            timeout: Timer::from_seconds(ACTIVATION_PRIMARY_TIMEOUT, TimerMode::Once),
            global_timeout: Timer::from_seconds(ACTIVATION_GLOBAL_TIMEOUT, TimerMode::Once), // 20秒全局激活超时
        })
    }

    fn enter_activating(&mut self, rng: &mut impl Rng) {
        // 重新创建旋转控制器确保参数完全重置
        self.rotation.clear_variable();

        // 大机关激活时使用变量旋转，小机关使用固定速度
        if self.mode == RuneMode::Large {
            self.rotation.reset_variable(rng);
        }
        if let Some(state) = self.build_new_round(rng) {
            self.state = MechanismState::Activating(state);
        } else {
            self.enter_completed();
        }
    }

    fn enter_inactive(&mut self) {
        self.reset_all_targets(Activation::Deactivated);
        self.rotation.clear_variable();
        self.state = MechanismState::Inactive {
            wait: Timer::from_seconds(INACTIVE_WAIT, TimerMode::Once),
        };
    }

    fn enter_failed(&mut self) {
        self.reset_all_targets(Activation::Deactivated);
        self.state = MechanismState::Failed {
            wait: Timer::from_seconds(FAILURE_RECOVER, TimerMode::Once),
        };
    }

    fn enter_completed(&mut self) {
        self.reset_all_targets(Activation::Completed);
        // 激活状态下停止旋转
        self.rotation.clear_variable();
        self.state = MechanismState::Activated {
            wait: Timer::from_seconds(ACTIVATED_HOLD, TimerMode::Once),
        };
    }

    fn reset_all_targets(&mut self, state: Activation) {
        for target in &mut self.targets {
            target.state = state;
        }
    }

    fn on_target_hit(&mut self, target_index: usize, rng: &mut impl Rng) -> HitResult {
        match &mut self.state {
            MechanismState::Activating(state) => {
                let Some(pos) = state
                    .highlighted
                    .iter()
                    .position(|&idx| idx == target_index)
                else {
                    if !FUNNY {
                        // 击中非点亮模块，触发激活失败
                        self.enter_failed();
                    }
                    return HitResult {
                        accurate: true,
                        change_state: false,
                    };
                };

                if state.hit_flags[pos] {
                    return HitResult {
                        accurate: true,
                        change_state: false,
                    };
                }
                state.hit_flags[pos] = true;
                self.targets[target_index].state = Activation::Activated;

                if self
                    .targets
                    .iter()
                    .all(|target| matches!(target.state, Activation::Activated))
                {
                    self.enter_completed();
                    return HitResult {
                        accurate: true,
                        change_state: true,
                    };
                }

                match self.mode {
                    RuneMode::Small => {
                        if let Some(next) = self.build_new_round(rng) {
                            self.state = MechanismState::Activating(next);
                            return HitResult {
                                accurate: true,
                                change_state: false,
                            };
                        }
                        self.enter_completed();
                        HitResult {
                            accurate: true,
                            change_state: true,
                        }
                    }
                    RuneMode::Large => {
                        let hits = state.hit_flags.iter().filter(|&&flag| flag).count();

                        // 大机关逻辑：规则要求命中任意一个靶后启动1秒二次窗口
                        if hits == 1 && state.window == ActivationWindow::Primary {
                            // 命中第一个靶后启动1秒二次命中窗口
                            state.window = ActivationWindow::Secondary;
                            state.timeout =
                                Timer::from_seconds(LARGE_SECONDARY_TIMEOUT, TimerMode::Once);
                            return HitResult {
                                accurate: true,
                                change_state: false,
                            };
                        }

                        // 处理边界情况：如果二次窗口超时后仍未命中第二个靶，进入下一轮
                        if let Some(next) = self.build_new_round(rng) {
                            self.state = MechanismState::Activating(next);
                            return HitResult {
                                accurate: true,
                                change_state: false,
                            };
                        }
                        self.enter_completed();
                        HitResult {
                            accurate: true,
                            change_state: true,
                        }
                    }
                }
            }
            _ => HitResult {
                accurate: false,
                change_state: false,
            },
        }
    }

    fn apply_shared_visual(&mut self, appearance: &mut StatefulAppearance) {
        match &self.state {
            MechanismState::Inactive { .. } => {
                self.r.set(Activation::Deactivated, appearance);
            }
            MechanismState::Activating { .. } => {
                self.r.set(Activation::Activating, appearance);
            }
            MechanismState::Activated { .. } => {
                self.r.set(Activation::Completed, appearance);
            }
            _ => {
                self.r.set(Activation::Deactivated, appearance);
            }
        };
    }
}

fn build_targets(
    face_index: usize,
    face_entity: Entity,
    name_map: &mut HashMap<&str, Entity>,
    param: &mut PowerRuneParam,
    creator: &mut StatefulAppearanceCreator,
) -> Vec<RuneData> {
    let mut targets = Vec::new();
    for target_idx in 1..=5 {
        let prefix = format!("FACE_{}_TARGET_{}", face_index, target_idx);

        let padding_segments = creator.create_controller(
            drain_entities_by(name_map, |name| {
                name.starts_with(&format!("{}_PADDING", prefix))
            }),
            material!(on = { completed }),
        );
        let progress_segments = creator.create_controller(
            drain_entities_by(name_map, |name| {
                name.starts_with(&format!("{}_LEGGING_PROGRESSING", prefix))
            }),
            visibility!(activating),
        );

        let ad = format!("{}_ACTIVATED", prefix);
        let at = format!("{}_ACTIVE", prefix);
        let d = format!("{}_DISABLED", prefix);
        let c = format!("{}_COMPLETED", prefix);
        let activated = ad.as_str();
        let active = at.as_str();
        let deactivated = d.as_str();
        let completed = c.as_str();

        let activated = name_map.remove(activated);
        let activating = name_map.remove(active);
        let deactivated = name_map.remove(deactivated);
        let completed = name_map.remove(completed);

        let logical_index = targets.len();
        /*
        let mut gizmo = GizmoAsset::new();
        gizmo
            .sphere(Isometry3d::IDENTITY, 0.15, CRIMSON)
            .resolution(30_000 / 3);
        let handle = param.gizmo_assets.add(gizmo);
        */
        for entity in [deactivated, activating, activated] {
            if let Some(entity) = entity {
                insert_all_child(&mut param.commands, entity, &mut param.children, || {
                    (
                        RuneIndex(logical_index, face_entity),
                        CollisionEventsEnabled,
                        /*Gizmo {
                            handle: handle.clone(),
                            line_config: GizmoLineConfig {
                                width: 2.,
                                ..default()
                            },
                            ..default()
                        },*/
                    )
                });
            }
        }

        let mut legging_segments: [Controller; 3] = [
            Controller::new_combined(vec![]),
            Controller::new_combined(vec![]),
            Controller::new_combined(vec![]),
        ];
        for legging_idx in 1..=3 {
            legging_segments[legging_idx - 1] = creator.create_controller(
                drain_entities_by(name_map, |name| {
                    name.starts_with(&format!("{}_LEGGING_{}", prefix, legging_idx))
                        && !name.contains("PROGRESSING")
                }),
                material!(on = {activated, completed}),
            )
        }

        targets.push(RuneData {
            visual: RuneVisual {
                target: Controller::new_visibility(deactivated, activating, activated, completed),
                legging_segments,
                padding_segments,
                progress_segments,
            },
            state: Activation::Deactivated,
            applied_state: Activation::Deactivated,
        });
    }
    targets
}

#[derive(SystemParam)]
struct PowerRuneParam<'w, 's> {
    commands: Commands<'w, 's>,
    scene_spawner: Res<'w, SceneSpawner>,

    gizmo_assets: ResMut<'w, Assets<GizmoAsset>>,

    power_query: Query<'w, 's, (), With<PowerRuneRoot>>,
    names: Query<'w, 's, &'static Name>,
    children: Query<'w, 's, &'static Children>,
}

fn setup_power_rune(
    events: On<SceneInstanceReady>,
    mut param: PowerRuneParam,
    mut creator: StatefulAppearanceCreator,
) {
    if !param.power_query.contains(events.entity) {
        return;
    }

    let names = param.names;
    let mut name_map = param
        .scene_spawner
        .iter_instance_entities(events.instance_id)
        .filter_map(|entity| names.get(entity).map(|n| (n.as_str(), entity)).ok())
        .fold(HashMap::new(), |mut m, (name, entity)| {
            m.insert(name, entity);
            m
        });

    if name_map.is_empty() {
        return;
    }

    let mut faces: Vec<(usize, Entity)> = name_map
        .iter()
        .filter_map(|(name, &entity)| {
            let rest = name.strip_prefix("FACE_")?;
            if rest.contains('_') {
                return None;
            }
            let index = rest.parse::<usize>().ok()?;
            Some((index, entity))
        })
        .collect();

    faces.sort_by_key(|(idx, _)| *idx);
    if faces.is_empty() {
        return;
    }

    for (index, face_entity) in faces {
        let mode = if index & 2 > 0 {
            RuneMode::Large
        } else {
            RuneMode::Small
        };

        let deactivated = name_map.remove(format!("FACE_{}_R_UNPOWERED", index).as_str());
        let activated = name_map.remove(format!("FACE_{}_R_POWERED", index).as_str());

        let mut targets =
            build_targets(index, face_entity, &mut name_map, &mut param, &mut creator);
        for target in &mut targets {
            target
                .visual
                .apply(mode, Activation::Deactivated, &mut creator.appearance);
        }

        if targets.is_empty() {
            continue;
        }

        param.commands.entity(face_entity).insert(PowerRune::new(
            if (index & 1) > 0 {
                Team::Red
            } else {
                Team::Blue
            },
            mode,
            Controller::new_visibility(
                deactivated,
                activated.clone(),
                activated.clone(),
                activated,
            ),
            targets,
            (index & 1) > 0,
        ));
    }
}

#[derive(EntityEvent)]
pub struct RuneActivated {
    #[event_target]
    pub rune: Entity,
}

#[derive(EntityEvent)]
pub struct RuneHit {
    #[event_target]
    pub rune: Entity,
    pub result: HitResult,
}

fn handle_rune_collision(
    event: On<CollisionStart>,
    mut commands: Commands,
    mut runes: Query<&mut PowerRune>,
    targets: Query<(&Visibility, &RuneIndex)>,
    projectiles: Query<(), With<Projectile>>,
    mut param: PowerRuneParam,
) {
    if event.body1.is_none() || event.body2.is_none() {
        return;
    }
    let projectile = event.body1.unwrap();
    if !projectiles.contains(projectile) {
        return;
    }
    let target = event.body2.unwrap();
    let Ok((visibility, &RuneIndex(index, rune_ent))) = targets.get(target) else {
        return;
    };
    if *visibility == Visibility::Hidden {
        return;
    }
    let Ok(mut rune) = runes.get_mut(rune_ent) else {
        return;
    };

    let mut rng = rand::rng();
    let result = rune.on_target_hit(index, &mut rng);
    match rune.state {
        MechanismState::Inactive { .. } => {
            commands.trigger(RuneHit {
                rune: rune_ent,
                result,
            });
        }
        MechanismState::Activating(_) => {
            commands.trigger(RuneHit {
                rune: rune_ent,
                result,
            });
        }
        MechanismState::Activated { .. } => {
            let mode = rune.mode;
            for target in &mut rune.targets {
                target.state = Activation::Completed;
                //target.visual.apply(mode, target.state, &mut param)
            }
            if result.change_state {
                commands.trigger(RuneActivated { rune: rune_ent });
            } else {
                commands.trigger(RuneHit {
                    rune: rune_ent,
                    result,
                });
            }
        }
        MechanismState::Failed { .. } => {
            commands.trigger(RuneHit {
                rune: rune_ent,
                result,
            });
        }
    }
}

fn rune_activation_tick(time: Res<Time>, mut runes: Query<&mut PowerRune>) {
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
                let mut action = None;

                // 检查20秒全局激活超时（最高优先级）
                if state.global_timeout.tick(delta).just_finished() {
                    action = Some(RuneAction::Failure); // 20秒全局超时激活失败
                }
                // 否则检查激活窗口超时
                else if state.timeout.tick(delta).just_finished() {
                    action = match state.window {
                        ActivationWindow::Primary => Some(RuneAction::Failure), // 2.5秒超时激活失败
                        ActivationWindow::Secondary => Some(RuneAction::NewRound), // 1秒窗口过期进入下一轮
                    };
                }

                action
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
            match action {
                RuneAction::StartActivating => rune.enter_activating(&mut rng),
                RuneAction::NewRound => {
                    if let Some(state) = rune.build_new_round(&mut rng) {
                        rune.state = MechanismState::Activating(state);
                    } else {
                        rune.enter_completed();
                    }
                }
                RuneAction::Failure => rune.enter_failed(),
                RuneAction::ResetToInactive => rune.enter_inactive(),
            }
        }
    }
}

fn rune_apply_visuals(mut runes: Query<&mut PowerRune>, mut appearance: StatefulAppearance) {
    for mut rune in &mut runes {
        rune.apply_shared_visual(&mut appearance);
        let mode = rune.mode;
        for target in &mut rune.targets {
            if target.state != target.applied_state {
                target.visual.apply(mode, target.state, &mut appearance);
                target.applied_state = target.state;
            }
        }
    }
}

fn rune_rotation_system(time: Res<Time>, mut runes: Query<(&mut Transform, &mut PowerRune)>) {
    let dt = time.delta_secs();
    for (mut transform, mut rune) in &mut runes {
        let mode = rune.mode.clone();
        // 只有在激活状态下大机关才使用变量旋转
        let speed = rune.rotation.current_speed(mode, dt);
        let angle = speed * dt;

        // 确保旋转方向正确：红方顺时针(正角)，蓝方逆时针(负角)
        transform.rotate_local_axis(rune.rotation.direction, angle);
    }
}

#[derive(Default)]
pub(super) struct PowerRunePlugin;

impl bevy::app::Plugin for PowerRunePlugin {
    fn build(&self, app: &mut bevy::app::App) {
        app.add_observer(handle_rune_collision)
            .add_observer(setup_power_rune)
            .add_systems(
                Update,
                (
                    rune_activation_tick,
                    rune_apply_visuals,
                    rune_rotation_system,
                ),
            );
    }
}
