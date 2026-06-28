use avian3d::prelude::*;
use bevy::anti_alias::fxaa::Fxaa;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::world_serialization::{WorldInstance, WorldInstanceReady};
use bevy_inspector_egui::bevy_egui::{EguiGlobalSettings, PrimaryEguiContext};
use std::collections::HashMap;

use crate::components::{
    ActiveSlapper, Controlled, DartLaunch, GameLayer, GroundRoot, Infantry, InfantryChassis,
    InfantryGimbal, InfantryLaunchOffset, InfantryViewOffset, MainCamera, PreciousCollision,
    SlapperInfantry,
};
use crate::config::SimulationConfig;
use crate::robomaster::prelude::{
    HERO_ROBOT_CONFIG, INFANTRY_THREE_CONFIG, OutpostRoot, PowerRuneRoot, ScanArmor, Team,
    TechCoreRoot,
};
use crate::robomaster::vehicle::movement::VehicleDynamic;
use crate::systems::spawn_text;
use crate::util::entity_query::HierarchyQuery;

#[derive(Component)]
pub struct ScanOutpost;

pub fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    config: Res<SimulationConfig>,
    egui_global_settings: Option<ResMut<EguiGlobalSettings>>,
) {
    if let Some(mut egui_global_settings) = egui_global_settings {
        egui_global_settings.auto_create_primary_context = false;
    }
    spawn_text(&mut commands);
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(0.9, 0.95, 1.0),
            illuminance: config.render.illuminance,
            shadow_maps_enabled: config.render.shadows,
            contact_shadows_enabled: config.render.shadows,
            ..default()
        },
        Transform::from_xyz(0.0, 4.0, 0.0).looking_at(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)),
    ));

    let layer_env = GameLayer::environment_collision_layers();

    let trimesh = || {
        ColliderConstructorHierarchy::new(ColliderConstructor::TrimeshFromMeshWithConfig(
            TrimeshFlags::all(),
        ))
        .with_default_layers(layer_env)
    };
    let voxel = |size| {
        ColliderConstructorHierarchy::new(ColliderConstructor::VoxelizedTrimeshFromMesh {
            voxel_size: size,
            fill_mode: FillMode::FloodFill {
                detect_cavities: true,
            },
        })
        .with_default_layers(layer_env)
    };

    commands.spawn((
        WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("GROUND.glb"))),
        Transform::IDENTITY,
        GroundRoot,
        Friction::new(0.5),
        PreciousCollision(HashMap::from([(
            "GROUND_DENSE".to_string(),
            (
                trimesh(),
                layer_env,
                Visibility::Visible,
                Some(RigidBody::Static),
            ),
        )])),
    ));

    commands.spawn((
        WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("CALIB.glb"))),
        Transform::IDENTITY
            .with_scale(Vec3::splat(1.0))
            .with_translation(Vec3::new(1.0, 2.5, 1.0)),
    ));

    commands.spawn((
        WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("CALIB.glb"))),
        Transform::IDENTITY
            .with_scale(Vec3::splat(1.0))
            .with_translation(Vec3::new(2.0, 0.5, 2.0)),
    ));

    commands.spawn((
        RigidBody::Static,
        WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("OUTPOST.glb"))),
        Transform::IDENTITY,
        ScanOutpost,
    ));

    commands.spawn((
        WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("TECH_CORE.glb"))),
        Transform::IDENTITY,
        TechCoreRoot,
        PreciousCollision(HashMap::from([(
            "GROUND".to_string(),
            (
                trimesh(),
                layer_env,
                Visibility::Visible,
                Some(RigidBody::Static),
            ),
        )])),
    ));

    let mut power_rune_col = HashMap::from([(
        "BASE".to_string(),
        (
            trimesh(),
            layer_env,
            Visibility::Visible,
            Some(RigidBody::Static),
        ),
    )]);
    for i in 1..=2 {
        for j in 1..=5 {
            for k in ["ACTIVATED", "ACTIVE", "COMPLETED", "DISABLED"] {
                power_rune_col.insert(
                    format!("FACE_{}_TARGET_{}_{}", i, j, k).to_string(),
                    (voxel(0.015), layer_env, Visibility::Visible, None),
                );
            }
        }
    }
    commands.spawn((
        RigidBody::Static,
        CollisionMargin(0.001),
        Restitution::ZERO,
        WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("POWER.glb"))),
        Transform::IDENTITY,
        PowerRuneRoot,
        PreciousCollision(power_rune_col),
    ));

    commands.spawn((
        WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("vehicle.glb"))),
        Transform::from_xyz(0.0, 1.0, 0.0),
        Infantry::new(Team::Red, INFANTRY_THREE_CONFIG),
        Controlled,
    ));

    commands.spawn((
        WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("vehicle.glb"))),
        Transform::from_xyz(1.0, 1.0, 1.0),
        Infantry::new(Team::Blue, INFANTRY_THREE_CONFIG),
        SlapperInfantry,
    ));

    commands.spawn((
        WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("HERO.glb"))),
        Transform::from_xyz(2.0, 1.0, 1.0),
        Infantry::new(Team::Blue, HERO_ROBOT_CONFIG),
        SlapperInfantry,
        ActiveSlapper,
    ));

    let mut main_camera = commands.spawn((
        Camera3d::default(),
        Camera {
            // When Talos/ROS2 capture is enabled, the actual on-screen preview is a UI blit of the
            // off-screen capture texture. Keep this camera inactive to avoid rendering twice.
            #[cfg(any(feature = "ros2", feature = "talos"))]
            is_active: false,
            #[cfg(not(any(feature = "ros2", feature = "talos")))]
            is_active: config.preview.enabled,
            // clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: config.camera.fov.to_radians(),
            near: 0.1,
            far: 500000000.0,
            ..default()
        }),
        Tonemapping::None,
        Msaa::Off,
        Transform::from_xyz(0.0, 10.0, 15.0).looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y),
        MainCamera {
            follow_offset: Vec3::from_array(config.camera.follow_offset),
        },
    ));
    if config.render.main_camera_fxaa {
        main_camera.insert(Fxaa::default());
    }
    if config.debug.egui {
        main_camera.insert(PrimaryEguiContext);
    }
    #[cfg(any(feature = "ros2", feature = "talos"))]
    main_camera.insert(crate::capture::CaptureSource);
}

pub fn setup_ground(
    events: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    name: Query<&Name>,
    ground: Single<Entity, With<ScanOutpost>>,
) {
    let root = events.entity;
    if ground.into_inner() != root {
        return;
    }
    children.iter_descendants(root).for_each(|e| {
        let Ok(name) = name.get(e) else {
            return;
        };
        if name.as_str() == "OUTPOST_1" {
            commands.entity(e).insert(OutpostRoot::new(Team::Red));
        }
        if name.as_str() == "OUTPOST_2" {
            commands.entity(e).insert(OutpostRoot::new(Team::Blue));
        }
    })
}

pub fn setup_dart_launch(
    events: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    name: Query<&Name>,
    ground: Query<(), With<GroundRoot>>,
) {
    if ground.get(events.entity).is_err() {
        return;
    }

    for entity in children.iter_descendants(events.entity) {
        let Ok(name) = name.get(entity) else {
            continue;
        };
        if name.as_str() == "DART_LAUNCH_DIRECTION" {
            commands.entity(entity).insert(DartLaunch);
            return;
        }
    }

    warn!("GROUND.glb is missing DART_LAUNCH_DIRECTION");
}

pub fn setup_vehicle(
    events: On<WorldInstanceReady>,
    mut commands: Commands,
    query: HierarchyQuery,
    root_query: Query<(
        Entity,
        &Infantry,
        Option<&Controlled>,
        Option<&ActiveSlapper>,
    )>,
    _secondary_query: Query<&ChildOf, (Without<Infantry>, Without<WorldInstance>)>,
    _node_query: Query<(&Name, &ChildOf), (Without<Infantry>, Without<WorldInstance>)>,
    sim_config: Res<SimulationConfig>,
) {
    let root = events.entity;
    if root_query.get(root).is_err() {
        return;
    }
    let (root, infantry, is_local, is_active) = root_query.get(root).unwrap();
    let team = infantry.team;
    let config = infantry.config;
    let is_local = is_local.is_some();
    let is_active = is_active.is_some();
    if is_local {
        query.children.iter_descendants(root).for_each(|e| {
            commands.entity(e).insert(Controlled);
        });
    } else {
        query.children.iter_descendants(root).for_each(|e| {
            commands.entity(e).insert(SlapperInfantry);
            if is_active {
                commands.entity(e).insert(ActiveSlapper);
            }
        });
    }
    let vehicle_body_collision_layers = GameLayer::vehicle_body_collision_layers(is_local);
    let vehicle_armor_collision_layers = GameLayer::vehicle_armor_collision_layers(is_local);

    commands.entity(root).insert((
        RigidBody::Dynamic,
        VehicleDynamic::new(
            sim_config.vehicle.max_speed,
            sim_config.vehicle.linear_acceleration,
            sim_config.vehicle.acceleration_exponent,
        ),
        Collider::compound(vec![(
            Vec3::new(0.0, -0.115649, 0.0),
            Quat::IDENTITY,
            Collider::cylinder(0.2593615, 0.231298),
        )]),
        CollisionMargin(0.005),
        vehicle_body_collision_layers,
        Mass(15.0),
        Restitution::new(0.01),
        AngularDamping(50.0),
    ));

    query.children.iter_descendants(root).for_each(|e| {
        commands.entity(e).insert(vehicle_armor_collision_layers);
    });

    let iter = query.of(root).any().exact("VEHICLE").flatten();
    let base = iter.clone().exact("BASE").one().unwrap();
    commands.entity(base).insert((
        InfantryChassis::default(),
        ScanArmor::new(team, config.armor),
    ));
    let gimbal = iter.exact("GIMBAL").one().unwrap();
    commands.entity(gimbal).insert(InfantryGimbal::default());
    if is_local {
        let q = query.of(gimbal).flatten();
        commands
            .entity(q.clone().exact("SHOT_DIRECTION").one().unwrap())
            .insert(InfantryLaunchOffset);
        commands
            .entity(q.exact("CAM_DIRECTION").one().unwrap())
            .insert(InfantryViewOffset);
    }
}

pub fn setup_collision(
    events: On<WorldInstanceReady>,
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
        if let Some((constructor, layer, visibility, rigid)) = map.get(&name.to_string()) {
            if let Some(rigid) = rigid {
                commands
                    .entity(e)
                    .insert((*rigid, constructor.clone(), *layer));
            } else {
                commands.entity(e).insert((constructor.clone(), *layer));
            }
            if visibility == &Visibility::Hidden {
                commands.entity(e).insert(*visibility);
            }
        }
    }
    commands.entity(events.entity).remove::<PreciousCollision>();
}
