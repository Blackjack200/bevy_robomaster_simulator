#![allow(dead_code)]
mod dataset;
mod handler;
mod robomaster;
mod ros2;
mod statistic;
mod util;

use crate::dataset::prelude::DatasetPlugin;
use crate::robomaster::prelude::{
    INFANTRY_THREE_CONFIG, PowerRuneRoot, Projectile, RoboMasterPlugins, RobotConfig, Team,
};
use crate::robomaster::vehicle::movement::VehicleDynamic;
use crate::ros2::plugin::ROS2Plugin;
use crate::util::bevy::insert_all_child;
use crate::{
    handler::{on_activate, on_hit},
    statistic::{accurate_count, accurate_pct, increase_launch, launch_count},
};
use avian3d::prelude::*;
use bevy::camera::Exposure;
use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::light::light_consts::lux;
use bevy::render::RenderSystems;
use bevy::render::view::screenshot::{Capturing, Screenshot, save_to_disk};
use bevy::window::{CursorIcon, PresentMode, SystemCursorIcon};
use bevy::{
    anti_alias::fxaa::Fxaa,
    input::mouse::MouseMotion,
    prelude::*,
    scene::{SceneInstance, SceneInstanceReady},
};
use bevy_inspector_egui::bevy_egui::{EguiGlobalSettings, PrimaryEguiContext};
use bevy_inspector_egui::{bevy_egui::EguiPlugin, quick::WorldInspectorPlugin};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;

#[derive(Component)]
struct MainCamera {
    follow_offset: Vec3,
}

#[derive(Component)]
struct Controlled;

#[derive(Component)]
struct Infantry(Team, RobotConfig);

#[derive(Resource, PartialEq, Deref, DerefMut)]
struct CameraMode(pub FollowingType);

#[derive(PartialEq)]
enum FollowingType {
    Free,
    Robot,
    ThirdPerson,
}

#[derive(Component, Default)]
struct InfantryChassis {
    yaw: f32,
}

#[derive(Component, Default)]
struct InfantryGimbal {
    local_yaw: f32,
    pitch: f32,
}

#[derive(Component)]
struct InfantryViewOffset;

#[derive(Component)]
struct InfantryLaunchOffset;

#[derive(PhysicsLayer, Default, Clone, Copy, Debug)]
enum GameLayer {
    #[default]
    Default,
    Vehicle,
    ProjectileSelf,
    ProjectileOther,
    Environment,
}

#[derive(Resource, Deref, DerefMut)]
struct Cooldown(Mutex<Timer>);

/// Creates help text at the bottom of the screen.
fn create_help_text() -> Text {
    format!(
        "total={} accurate={} pct={}\nControls: F2-Screenshot F3-Change Camera | WASD-Move Mouse-Look Space-Shoot",
        launch_count(),
        accurate_count(),
        accurate_pct()
    )
        .into()
}

/// Spawns the help text at the bottom of the screen.
fn spawn_text(commands: &mut Commands) {
    commands.spawn((
        create_help_text(),
        Node {
            position_type: PositionType::Absolute,
            bottom: px(12),
            left: px(12),
            ..default()
        },
    ));
}

fn update_help_text(mut text: Query<&mut Text>) {
    for mut text in text.iter_mut() {
        *text = create_help_text();
    }
}

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    present_mode: PresentMode::AutoVsync,
                    fit_canvas_to_parent: true,
                    ..default()
                }),
                ..default()
            }),
            PhysicsPlugins::default(),
        ))
        .add_plugins(ROS2Plugin::default())
        .add_plugins((EguiPlugin::default(), WorldInspectorPlugin::new()))
        //.add_plugins(PhysicsDebugPlugin::default())
        .add_plugins(RoboMasterPlugins)
        .add_plugins((
            FrameTimeDiagnosticsPlugin::default(),
            LogDiagnosticsPlugin::default(),
        ))
        .add_plugins(DatasetPlugin)
        .insert_resource(CameraMode(FollowingType::Robot))
        .insert_resource(Gravity(Vec3::NEG_Y * 9.81))
        .insert_resource(SubstepCount(10))
        .insert_resource(Cooldown(Mutex::new(Timer::from_seconds(
            0.1,
            TimerMode::Once,
        ))))
        .add_systems(Startup, (setup, setup_projectile))
        .add_observer(setup_vehicle)
        .add_observer(setup_collision)
        .add_observer(on_hit)
        .add_observer(on_activate)
        .add_systems(
            Update,
            (
                update_help_text,
                following_controls,
                vehicle_controls.run_if(|mode: Res<CameraMode>| mode.0 != FollowingType::Free),
                freecam_controls.run_if(|mode: Res<CameraMode>| mode.0 == FollowingType::Free),
                update_camera_follow
                    .run_if(|mode: Res<CameraMode>| mode.0 != FollowingType::Free)
                    .before(RenderSystems::Render),
                remote_vehicle_controls,
                gimbal_controls,
                remote_gimbal_controls,
                screenshot_on_f2
                    .run_if(|input: Res<ButtonInput<KeyCode>>| input.just_pressed(KeyCode::F2)),
                screenshot_saving,
            ),
        )
        .add_systems(
            PostUpdate,
            projectile_launch.after(TransformSystems::Propagate).run_if(
                |time: Res<Time>, cooldown: Res<Cooldown>, keyboard: Res<ButtonInput<KeyCode>>| {
                    let mut cooldown = cooldown.lock().unwrap();
                    cooldown.tick(time.delta());
                    if !cooldown.is_finished() {
                        return false;
                    }
                    cooldown.reset();
                    return keyboard.pressed(KeyCode::Space);
                },
            ),
        )
        .run();
}

#[derive(Component, Deref, DerefMut)]
struct PreciousCollision(
    HashMap<String, (ColliderConstructorHierarchy, CollisionLayers, Visibility)>,
);

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut egui_global_settings: ResMut<EguiGlobalSettings>,
) {
    egui_global_settings.auto_create_primary_context = false;
    spawn_text(&mut commands);
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(0.9, 0.95, 1.0),
            shadows_enabled: true,
            illuminance: lux::DIRECT_SUNLIGHT,
            ..default()
        },
        Transform::from_xyz(0.0, 4.0, 0.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    let layer_env = CollisionLayers::new(
        [GameLayer::Environment],
        [
            GameLayer::Default,
            GameLayer::Vehicle,
            GameLayer::ProjectileSelf,
            GameLayer::ProjectileOther,
        ],
    );

    let trimesh = || {
        ColliderConstructorHierarchy::new(ColliderConstructor::TrimeshFromMeshWithConfig(
            TrimeshFlags::all(),
        ))
    };
    let voxel = |size| {
        ColliderConstructorHierarchy::new(ColliderConstructor::VoxelizedTrimeshFromMesh {
            voxel_size: size,
            fill_mode: FillMode::FloodFill {
                detect_cavities: true,
            },
        })
    };

    commands.spawn((
        RigidBody::Static,
        SceneRoot(asset_server.load("GROUND.glb#Scene0")),
        Transform::IDENTITY,
        Friction::new(0.5),
        PreciousCollision(HashMap::from([(
            "GROUND_DENSE".to_string(),
            (trimesh(), layer_env, Visibility::Visible),
        )])),
    ));

    let mut power_rune_col = HashMap::from([(
        "BASE".to_string(),
        (trimesh(), layer_env, Visibility::Visible),
    )]);
    for i in 1..=2 {
        /*power_rune_col.insert(
            format!("FACE_{}", i).to_string(),
            (trimesh.clone(), layer_env, Visibility::Visible),
        );*/
        for j in 1..=5 {
            for k in ["ACTIVATED", "ACTIVE", "COMPLETED", "DISABLED"] {
                power_rune_col.insert(
                    format!("FACE_{}_TARGET_{}_{}", i, j, k).to_string(),
                    (voxel(0.015), layer_env, Visibility::Visible),
                );
            }
        }
    }
    commands.spawn((
        RigidBody::Static,
        CollisionMargin(0.001),
        Restitution::ZERO,
        SceneRoot(asset_server.load("POWER.glb#Scene0")),
        Transform::IDENTITY,
        PowerRuneRoot,
        PreciousCollision(power_rune_col),
    ));

    commands.spawn((
        SceneRoot(asset_server.load("vehicle.glb#Scene0")),
        Transform::from_xyz(0.0, 1.0, 0.0),
        Infantry(Team::Red, INFANTRY_THREE_CONFIG),
        Controlled,
    ));

    commands.spawn((
        SceneRoot(asset_server.load("vehicle.glb#Scene0")),
        Transform::from_xyz(1.0, 1.0, 1.0),
        Infantry(Team::Blue, INFANTRY_THREE_CONFIG),
    ));

    commands.spawn((
        Camera3d::default(),
        Camera::default(),
        PrimaryEguiContext,
        Projection::Perspective(PerspectiveProjection {
            fov: std::f32::consts::PI / 180.0 * 45.0,
            near: 0.1,
            far: 500000000.0,
            ..default()
        }),
        Exposure::SUNLIGHT,
        Msaa::Off,
        Fxaa::default(),
        Transform::from_xyz(0.0, 10.0, 15.0).looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y),
        MainCamera {
            follow_offset: Vec3::new(0.0, 3.0, 2.0),
        },
        ros2::plugin::MainCamera,
    ));
}

#[derive(Component, Clone)]
pub struct Armor(Team, RobotConfig);

fn setup_vehicle(
    events: On<SceneInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    root_query: Query<(Entity, &Infantry, Option<&Controlled>)>,
    secondary_query: Query<&ChildOf, (Without<Infantry>, Without<SceneInstance>)>,
    node_query: Query<(Entity, &Name, &ChildOf), (Without<Infantry>, Without<SceneInstance>)>,
) {
    let root = events.entity;
    if root_query.get(root).is_err() {
        return;
    }
    let (root, &Infantry(team, config), is_local) = root_query.get(root).unwrap();
    let is_local = is_local.is_some();
    if is_local {
        children.iter_descendants(root).for_each(|e| {
            commands.entity(e).insert(Controlled);
        });
    }
    commands.entity(root).insert((
        RigidBody::Dynamic,
        VehicleDynamic::default(),
        Collider::compound(vec![
            (
                Vec3::new(0.0, 0.115649, 0.0),
                Quat::IDENTITY,
                Collider::cylinder(0.1040215, 0.364237),
            ),
            (
                Vec3::new(0.0, -0.115649, 0.0),
                Quat::IDENTITY,
                Collider::cylinder(0.2593615, 0.231298),
            ),
        ]),
        CollisionMargin(0.005),
        CollisionLayers::new(
            GameLayer::Vehicle,
            [
                GameLayer::Default,
                GameLayer::Vehicle,
                GameLayer::ProjectileOther,
                GameLayer::Environment,
            ],
        ),
        Mass(25.0),
        Restitution::new(0.01),
        AngularDamping(50.0),
    ));

    let mut despawn = HashSet::new();

    for (node, name, &ChildOf(secondary)) in node_query {
        let Ok(&ChildOf(root2)) = secondary_query.get(secondary) else {
            continue;
        };
        if root != root2 {
            continue;
        }
        despawn.insert(secondary);
        commands.entity(secondary).remove_child(node);
        commands.entity(root).add_child(node);
        let mut ent = commands.entity(node);
        match name.as_str() {
            "BASE" => {
                ent.insert(InfantryChassis::default());
                let mut stack = VecDeque::from([(node, name)]);
                let mut set = HashSet::new();
                while let Some((e, name)) = stack.pop_front() {
                    if !set.insert(e) {
                        continue;
                    }
                    if name.starts_with("ARMOR_") && name.ends_with("_P") {
                        insert_all_child(&mut commands, e, &children, || Armor(team, config));
                        commands.entity(e).insert(ColliderConstructorHierarchy::new(
                            ColliderConstructor::TrimeshFromMeshWithConfig(
                                TrimeshFlags::MERGE_DUPLICATE_VERTICES,
                            ),
                        ));
                    }
                    if name.starts_with("ARMOR_") && name.ends_with("_L") {
                        insert_all_child(&mut commands, e, &children, || Armor(team, config));
                        commands.entity(e).insert(ColliderConstructorHierarchy::new(
                            ColliderConstructor::TrimeshFromMeshWithConfig(
                                TrimeshFlags::MERGE_DUPLICATE_VERTICES,
                            ),
                        ));
                    }
                    for (ee, n, &ChildOf(r)) in node_query {
                        if r == e {
                            stack.push_back((ee, n));
                        }
                    }
                }
            }
            "GIMBAL" => {
                ent.insert(InfantryGimbal::default());
                if is_local {
                    children.iter_descendants(node).for_each(|e| {
                        if let Ok((_, name, _)) = node_query.get(e) {
                            match name.as_str() {
                                "SHOT_DIRECTION" => {
                                    commands.entity(e).insert(InfantryLaunchOffset);
                                }
                                "CAM_DIRECTION" => {
                                    commands.entity(e).insert(InfantryViewOffset);
                                }
                                _ => {}
                            }
                        }
                    });
                }
            }
            _ => {}
        }
    }

    for ent in despawn {
        commands.entity(ent).despawn();
    }
}

fn setup_projectile(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(ProjectileSetting(
        meshes.add(Sphere::new(44.5 * 0.001 / 2.0)),
        materials.add(StandardMaterial {
            base_color: Color::srgba(0.132866, 1.0, 0.132869, 0.55),
            emissive: LinearRgba::new(0.132866, 1.0, 0.132869, 0.55),
            emissive_exposure_weight: -1.0,
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
    ));
}

#[derive(Resource)]
struct ProjectileSetting(Handle<Mesh>, Handle<StandardMaterial>);

fn projectile_launch(
    _asset_server: Res<AssetServer>,
    mut commands: Commands,
    setting: Res<ProjectileSetting>,
    infantry: Single<
        (&Transform, &LinearVelocity, &AngularVelocity),
        (With<Infantry>, With<Controlled>),
    >,
    gimbal: Single<
        (&GlobalTransform, &InfantryGimbal),
        (With<Controlled>, Without<InfantryChassis>),
    >,
    launch_offset: Single<&Transform, (With<Controlled>, With<InfantryLaunchOffset>)>,
) {
    increase_launch();
    let direction = (gimbal.0.rotation() * launch_offset.rotation)
        .mul_vec3(Vec3::Y)
        .normalize_or_zero();
    if direction == Vec3::ZERO {
        return;
    }
    let vel = infantry.1.0 + direction * 25.0;
    commands.spawn((
        RigidBody::Dynamic,
        Collider::sphere(44.5 * 0.001 / 2.0),
        Mass(44.5 * 0.001),
        Friction::new(1.1),
        Restitution::ZERO,
        LinearDamping(0.05),
        CollisionLayers::new(
            GameLayer::ProjectileSelf,
            [
                GameLayer::Default,
                GameLayer::Vehicle,
                GameLayer::ProjectileSelf,
                GameLayer::ProjectileOther,
                GameLayer::Environment,
            ],
        ),
        Mesh3d(setting.0.clone()),
        MeshMaterial3d(setting.1.clone()),
        LinearVelocity(vel),
        AngularVelocity(infantry.2.0),
        Transform::IDENTITY.with_translation(
            infantry.0.translation + (gimbal.0.rotation() * launch_offset.translation),
        ),
        //AudioPlayer::new(asset_server.load("projectile_launch.ogg")),
        Projectile,
    ));
}

fn setup_collision(
    events: On<SceneInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    name: Query<&Name, With<Children>>,
    root_query: Query<(Entity, &PreciousCollision)>,
) {
    let Ok((_, PreciousCollision(map))) = root_query.get(events.entity) else {
        return;
    };
    for e in children.iter_descendants(events.entity) {
        let Ok(name) = name.get(e) else {
            continue;
        };
        if let Some((constructor, layer, visibility)) = map.get(&name.to_string()) {
            println!("{}", name);
            commands.entity(e).insert((
                RigidBody::Static,
                Restitution::ZERO,
                constructor.clone(),
                CollisionMargin(0.001),
                *layer,
            ));
            if visibility == Visibility::Hidden {
                commands.entity(e).insert(*visibility);
            }
        }
    }
    commands.entity(events.entity).remove::<PreciousCollision>();
}

// 单位 rad/s
const VEHICLE_ROTATION_SPEED: f32 = 3.0;
const GIMBAL_ROTATION_SPEED: f32 = 3.0;

macro_rules! input {
    ($keyboard:ident, $forward:ident,$left:ident,$backward:ident,$right:ident) => {{
        let mut input = Vec2::ZERO;
        if $keyboard.pressed(KeyCode::$forward) {
            input.y += 1.0;
        }
        if $keyboard.pressed(KeyCode::$backward) {
            input.y -= 1.0;
        }
        if $keyboard.pressed(KeyCode::$right) {
            input.x += 1.0;
        }
        if $keyboard.pressed(KeyCode::$left) {
            input.x -= 1.0;
        }
        input
    }};
    ($keyboard:ident, $left:ident,$right:ident) => {{
        let mut input: f32 = 0.0;
        if $keyboard.pressed(KeyCode::$left) {
            input += 1.0;
        }
        if $keyboard.pressed(KeyCode::$right) {
            input += -1.0;
        }
        input
    }};
}

fn vehicle_controls(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    infantry: Single<(Forces, &Mass, &mut VehicleDynamic), (With<Infantry>, With<Controlled>)>,
    gimbal: Single<
        (&GlobalTransform, &InfantryGimbal),
        (With<Controlled>, Without<InfantryChassis>),
    >,
    chassis: Single<
        (&mut Transform, &mut InfantryChassis),
        (
            With<Controlled>,
            Without<InfantryGimbal>,
            With<InfantryChassis>,
            Without<Infantry>,
        ),
    >,
) {
    let input = input!(keyboard, KeyW, KeyA, KeyS, KeyD);

    let (mut forces, &Mass(mass), mut dynamic) = infantry.into_inner();

    let dt = time.delta_secs();
    dynamic.linear(
        &mut forces,
        mass,
        gimbal.into_inner().0,
        input,
        time.delta_secs(),
    );

    let input = input!(keyboard, KeyQ, KeyE);
    let (mut chassis_transform, mut chassis_data) = chassis.into_inner();
    chassis_data.yaw += input * VEHICLE_ROTATION_SPEED * dt;
    chassis_transform.rotation = Quat::from_euler(EulerRot::YXZ, chassis_data.yaw, 0.0, 0.0);
}

fn remote_vehicle_controls(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    infantry: Single<(Forces, &Mass, &mut VehicleDynamic), (With<Infantry>, Without<Controlled>)>,
    gimbal: Single<
        (&GlobalTransform, &InfantryGimbal),
        (Without<Controlled>, Without<InfantryChassis>),
    >,
    chassis: Single<
        (&mut Transform, &mut InfantryChassis),
        (Without<Controlled>, Without<InfantryGimbal>),
    >,
) {
    let input = input!(keyboard, KeyI, KeyJ, KeyK, KeyL);

    let (mut forces, &Mass(mass), mut dynamic) = infantry.into_inner();

    let dt = time.delta_secs();
    dynamic.linear(
        &mut forces,
        mass,
        gimbal.into_inner().0,
        input,
        time.delta_secs(),
    );

    let input = input!(keyboard, KeyU, KeyO);
    let (mut chassis_transform, mut chassis_data) = chassis.into_inner();
    chassis_data.yaw += input * VEHICLE_ROTATION_SPEED * dt;
    chassis_transform.rotation = Quat::from_euler(EulerRot::YXZ, chassis_data.yaw, 0.0, 0.0);
}

fn gimbal_controls(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    gimbal: Single<
        (&mut Transform, &mut InfantryGimbal),
        (With<Controlled>, Without<InfantryChassis>),
    >,
) {
    let dt = time.delta_secs();
    let (mut gimbal_transform, mut gimbal_data) = gimbal.into_inner();

    (gimbal_data.local_yaw, gimbal_data.pitch, _) =
        gimbal_transform.rotation.to_euler(EulerRot::YXZ);

    gimbal_data.local_yaw += input!(keyboard, ArrowLeft, ArrowRight) * GIMBAL_ROTATION_SPEED * dt;
    gimbal_data.pitch += input!(keyboard, ArrowUp, ArrowDown) * GIMBAL_ROTATION_SPEED * dt;

    gimbal_data.pitch = gimbal_data.pitch.clamp(-0.785, 0.785);

    let gimbal_rotation =
        Quat::from_euler(EulerRot::YXZ, gimbal_data.local_yaw, gimbal_data.pitch, 0.0);

    gimbal_transform.rotation = gimbal_rotation;
}

fn remote_gimbal_controls(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    gimbal: Single<
        (&mut Transform, &mut InfantryGimbal),
        (Without<Controlled>, Without<InfantryChassis>),
    >,
) {
    let dt = time.delta_secs();
    let (mut gimbal_transform, mut gimbal_data) = gimbal.into_inner();

    (gimbal_data.local_yaw, gimbal_data.pitch, _) =
        gimbal_transform.rotation.to_euler(EulerRot::YXZ);

    gimbal_data.local_yaw += input!(keyboard, KeyC, KeyB) * GIMBAL_ROTATION_SPEED * dt;
    gimbal_data.pitch += input!(keyboard, KeyF, KeyV) * GIMBAL_ROTATION_SPEED * dt;
    gimbal_data.pitch = gimbal_data.pitch.clamp(-0.785, 0.785);

    let gimbal_rotation =
        Quat::from_euler(EulerRot::YXZ, gimbal_data.local_yaw, gimbal_data.pitch, 0.0);

    gimbal_transform.rotation = gimbal_rotation;
}

fn following_controls(mut mode: ResMut<CameraMode>, keyboard: Res<ButtonInput<KeyCode>>) {
    if keyboard.just_pressed(KeyCode::F3) {
        mode.0 = match mode.0 {
            FollowingType::Free => FollowingType::Robot,
            FollowingType::Robot => FollowingType::ThirdPerson,
            FollowingType::ThirdPerson => FollowingType::Free,
        };
    }
}

fn update_camera_follow(
    camera_query: Single<(&mut Transform, &MainCamera), Without<Controlled>>,
    infantry: Single<&Transform, (With<Infantry>, With<Controlled>)>,
    gimbal: Single<&Transform, (With<Controlled>, With<InfantryGimbal>)>,
    view_offset: Single<&Transform, (With<Controlled>, With<InfantryViewOffset>)>,
    mode: Res<CameraMode>,
) {
    let gimbal_transform = gimbal.into_inner();
    let (mut camera_transform, camera_offset) = camera_query.into_inner();

    match mode.0 {
        FollowingType::Robot => {
            // 严格跟随机器人 → 直接赋值
            let view_offset_transform = view_offset.into_inner();
            let gimbal_world_rotation = infantry.rotation * gimbal_transform.rotation;
            let view_offset_world = gimbal_world_rotation * view_offset_transform.translation;

            camera_transform.translation = infantry.translation + view_offset_world;
            camera_transform.rotation = gimbal_world_rotation;
        }
        FollowingType::ThirdPerson => {
            let base_transform = infantry.into_inner();
            let offset = base_transform.rotation * camera_offset.follow_offset;
            camera_transform.translation = base_transform.translation + offset;
            camera_transform.look_at(base_transform.translation, Vec3::Y);
        }
        FollowingType::Free => {
            // 自由模式不修改
        }
    }
}

fn freecam_controls(
    time: Res<Time>,
    mode: Res<CameraMode>,
    mut mouse_motion_events: MessageReader<MouseMotion>,
    keyboard: Res<ButtonInput<KeyCode>>,
    camera_query: Single<&mut Transform, (With<MainCamera>, Without<Infantry>)>,
) {
    if mode.0 != FollowingType::Free {
        return;
    }

    let delta = time.delta_secs();
    let mut camera_transform = camera_query.into_inner();

    let mut mouse_delta = Vec2::ZERO;
    for event in mouse_motion_events.read() {
        mouse_delta += event.delta;
    }

    if mouse_delta != Vec2::ZERO {
        let (yaw, pitch, roll) = camera_transform.rotation.to_euler(EulerRot::YXZ);

        let new_yaw = yaw - mouse_delta.x * 0.003;
        let new_pitch = (pitch - mouse_delta.y * 0.003).clamp(-1.4, 1.4);

        camera_transform.rotation = Quat::from_euler(EulerRot::YXZ, new_yaw, new_pitch, roll);
    }

    const CAMERA_SPEED: f32 = 8.0;
    let speed = CAMERA_SPEED * delta;
    let forward = camera_transform.forward();
    let right = camera_transform.right();
    let up = camera_transform.up();

    if keyboard.pressed(KeyCode::KeyW) {
        camera_transform.translation += forward * speed;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        camera_transform.translation -= forward * speed;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        camera_transform.translation -= right * speed;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        camera_transform.translation += right * speed;
    }
    if keyboard.pressed(KeyCode::KeyN) {
        camera_transform.translation += up * speed;
    }
    if keyboard.pressed(KeyCode::KeyJ) {
        camera_transform.translation -= up * speed;
    }
}

fn screenshot_on_f2(mut commands: Commands, mut counter: Local<u32>) {
    let path = format!("./screenshot-{}.png", *counter);
    *counter += 1;
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
}

fn screenshot_saving(
    mut commands: Commands,
    screenshot_saving: Query<Entity, With<Capturing>>,
    window: Single<Entity, With<Window>>,
) {
    match screenshot_saving.iter().count() {
        0 => {
            commands.entity(*window).remove::<CursorIcon>();
        }
        x if x > 0 => {
            commands
                .entity(*window)
                .insert(CursorIcon::from(SystemCursorIcon::Progress));
        }
        _ => {}
    }
}
