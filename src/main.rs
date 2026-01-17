#![allow(dead_code)]
mod capture;
mod components;
mod config;
mod dataset;
mod handler;
mod robomaster;
mod setup;
mod statistic;
mod systems;
mod telemetry;
mod util;

#[cfg(feature = "ros2")]
mod ros2;
#[cfg(feature = "talos")]
mod talos;

use avian3d::prelude::*;
use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::render::RenderSystems;
use bevy::window::PresentMode;
use bevy_inspector_egui::bevy_egui::EguiPlugin;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use std::sync::atomic::AtomicBool;

use crate::components::{CameraMode, FollowingType, ProjectileCooldown, SubscribeAutoAim};
use crate::config::{ConfigPlugin, SimulationConfig};
use crate::dataset::prelude::DatasetPlugin;
use crate::handler::{on_activate, on_hit};
use crate::robomaster::prelude::RoboMasterPlugins;
use crate::setup::{setup, setup_collision, setup_ground, setup_vehicle};
use crate::statistic::ProjectileStatistics;
use crate::systems::{
    GameplaySystems, auto_aim_switch, change_appearance, cleanup_projectiles, following_controls,
    freecam_controls, gimbal_controls, projectile_aerodynamics, projectile_launch,
    remote_gimbal_controls, remote_vehicle_controls, screenshot_on_f2, screenshot_saving,
    setup_projectile, switch_slapper_control, update_help_text, vehicle_controls,
};

#[cfg(feature = "ros2")]
use crate::ros2::plugin::ROS2Plugin;
#[cfg(feature = "talos")]
use talos::TalosPlugin;

fn main() {
    let config = SimulationConfig::default();
    let mut app = App::new();
    app.add_plugins((
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
    .add_plugins((EguiPlugin::default(), WorldInspectorPlugin::new()))
    .add_plugins(RoboMasterPlugins)
    .add_plugins((
        FrameTimeDiagnosticsPlugin::default(),
        LogDiagnosticsPlugin::default(),
    ))
    .add_plugins(DatasetPlugin)
    .add_plugins(ConfigPlugin)
    .init_resource::<CameraMode>()
    .init_resource::<ProjectileStatistics>()
    .register_type::<ProjectileStatistics>()
    .insert_resource(Gravity(Vec3::NEG_Y * 9.81))
    .insert_resource(SubstepCount(config.physics.substep_count))
    .insert_resource(SubscribeAutoAim(AtomicBool::new(false)))
    .insert_resource(ProjectileCooldown(Timer::from_seconds(
        config.projectile.cooldown,
        TimerMode::Once,
    )))
    .add_systems(Startup, (setup, setup_projectile))
    .add_observer(setup_ground)
    .add_observer(setup_vehicle)
    .add_observer(setup_collision)
    .add_observer(on_hit)
    .add_observer(on_activate)
    .configure_sets(
        Update,
        (
            GameplaySystems::Input,
            GameplaySystems::GameLogic,
            GameplaySystems::Camera,
            GameplaySystems::Cleanup,
        )
            .chain(),
    )
    .add_systems(
        Update,
        (
            // Input phase
            (
                auto_aim_switch,
                following_controls,
                switch_slapper_control,
                vehicle_controls.run_if(|mode: Res<CameraMode>| mode.0 != FollowingType::Free),
                remote_vehicle_controls,
                gimbal_controls,
                remote_gimbal_controls,
            )
                .in_set(GameplaySystems::Input),
            // GameLogic phase
            (change_appearance, update_help_text).in_set(GameplaySystems::GameLogic),
            // Camera phase
            (
                freecam_controls.run_if(|mode: Res<CameraMode>| mode.0 == FollowingType::Free),
                systems::update_camera_follow
                    .run_if(|mode: Res<CameraMode>| mode.0 != FollowingType::Free),
            )
                .in_set(GameplaySystems::Camera)
                .before(RenderSystems::Render),
            // Cleanup phase
            (
                cleanup_projectiles,
                screenshot_on_f2
                    .run_if(|input: Res<ButtonInput<KeyCode>>| input.just_pressed(KeyCode::F2)),
                screenshot_saving,
            )
                .in_set(GameplaySystems::Cleanup),
        ),
    )
    .add_systems(
        PostUpdate,
        projectile_launch
            .after(TransformSystems::Propagate)
            .run_if(|keyboard: Res<ButtonInput<KeyCode>>| keyboard.pressed(KeyCode::Space)),
    )
    .add_systems(FixedUpdate, projectile_aerodynamics);

    #[cfg(feature = "ros2")]
    {
        app.add_plugins(ROS2Plugin::default());
        info!("ROS2 integration enabled");
    }
    #[cfg(not(feature = "ros2"))]
    {
        info!("ROS2 integration disabled");
    }

    #[cfg(feature = "talos")]
    {
        app.add_plugins(TalosPlugin::default());
        info!("talos integration enabled");
    }

    app.run();
}
