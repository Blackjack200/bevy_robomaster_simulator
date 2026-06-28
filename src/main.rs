#![allow(dead_code)]
mod auto_gen;
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
use bevy::render::settings::{InstanceFlags, RenderCreation, WgpuSettings, WgpuSettingsPriority};
use bevy::render::{RenderPlugin, RenderSystems};
use bevy::window::PresentMode;
use bevy_inspector_egui::bevy_egui::EguiPlugin;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use clap::Parser;
use std::sync::atomic::AtomicBool;

use crate::auto_gen::AutoGenPlugin;
use crate::components::{CameraMode, FollowingType, ProjectileCooldown, SubscribeAutoAim};
use crate::config::{ConfigPlugin, SimulationConfig};
use crate::dataset::prelude::DatasetPlugin;
use crate::handler::{on_activate, on_hit};
use crate::robomaster::prelude::RoboMasterPlugins;
use crate::setup::{setup, setup_collision, setup_dart_launch, setup_ground, setup_vehicle};
use crate::statistic::ProjectileStatistics;
use crate::systems::{
    ChassisObservationFrame, GameplaySystems, PreviousKinematicState, auto_aim_switch,
    change_appearance, cleanup_projectiles, dart_launch, following_controls, freecam_controls,
    gimbal_controls, projectile_aerodynamics, projectile_launch, remote_gimbal_controls,
    remote_vehicle_controls, screenshot_on_f2, screenshot_saving, setup_projectile,
    switch_slapper_control, uav_launch, update_chassis_observation, update_help_text,
    vehicle_controls,
};

/// Command-line arguments for the application
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Enable auto dataset generation mode
    #[arg(long)]
    auto_gen: bool,
}

#[cfg(feature = "ros2")]
use crate::ros2::plugin::ROS2Plugin;
#[cfg(feature = "talos")]
use talos::TalosPlugin;

fn present_mode_from_config(value: &str) -> Option<PresentMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto_vsync" | "vsync" => Some(PresentMode::AutoVsync),
        "auto_no_vsync" | "no_vsync" | "novsync" => Some(PresentMode::AutoNoVsync),
        "fifo" => Some(PresentMode::Fifo),
        "fifo_relaxed" | "fifo-relaxed" => Some(PresentMode::FifoRelaxed),
        "mailbox" => Some(PresentMode::Mailbox),
        "immediate" => Some(PresentMode::Immediate),
        _ => None,
    }
}

fn is_wsl() -> bool {
    std::env::var_os("WSL_DISTRO_NAME").is_some()
        || std::env::var_os("WSL_INTEROP").is_some()
        || std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .map(|release| release.to_ascii_lowercase().contains("microsoft"))
            .unwrap_or(false)
}

fn render_plugin_for_platform() -> RenderPlugin {
    if cfg!(target_os = "linux") && is_wsl() {
        return RenderPlugin {
            render_creation: RenderCreation::Automatic(Box::new(WgpuSettings {
                instance_flags: InstanceFlags::default()
                    | InstanceFlags::ALLOW_UNDERLYING_NONCOMPLIANT_ADAPTER,
                priority: WgpuSettingsPriority::Functionality,
                ..default()
            })),
            ..default()
        };
    }

    RenderPlugin::default()
}

#[cfg(feature = "talos")]
fn should_enable_talos_plugin(app: &App) -> bool {
    #[cfg(feature = "ros2")]
    let ros_capture_active = app
        .world()
        .contains_resource::<crate::ros2::capture::RosCaptureContext>();
    #[cfg(not(feature = "ros2"))]
    let ros_capture_active = false;

    let force_talos_capture = std::env::var("DAEDALUS_FORCE_TALOS_CAPTURE")
        .map(|v| v == "1")
        .unwrap_or(false);

    !ros_capture_active || force_talos_capture
}

fn main() {
    let args = Args::parse();

    // Auto-gen mode: minimal setup
    if args.auto_gen {
        let config = SimulationConfig::default();
        let present_mode =
            present_mode_from_config(&config.window.present_mode).unwrap_or_else(|| {
                warn!(
                    "Unknown window.present_mode {:?}, falling back to auto_no_vsync",
                    config.window.present_mode
                );
                PresentMode::AutoNoVsync
            });

        App::new()
            .add_plugins((
                DefaultPlugins
                    .set(WindowPlugin {
                        primary_window: Some(Window {
                            present_mode,
                            fit_canvas_to_parent: true,
                            ..default()
                        }),
                        ..default()
                    })
                    .set(render_plugin_for_platform()),
                PhysicsPlugins::default(),
            ))
            .add_plugins(RoboMasterPlugins)
            .add_plugins(ConfigPlugin)
            .add_observer(setup_vehicle)
            .insert_resource(Gravity(Vec3::ZERO))
            .insert_resource(SubstepCount(config.physics.substep_count))
            .add_plugins(AutoGenPlugin)
            .run();
        return;
    }

    // Full simulation mode: existing functionality
    let config = SimulationConfig::default();
    let present_mode = present_mode_from_config(&config.window.present_mode).unwrap_or_else(|| {
        warn!(
            "Unknown window.present_mode {:?}, falling back to auto_no_vsync",
            config.window.present_mode
        );
        PresentMode::AutoNoVsync
    });
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    present_mode,
                    fit_canvas_to_parent: true,
                    ..default()
                }),
                ..default()
            })
            .set(render_plugin_for_platform()),
        PhysicsPlugins::default(),
    ));

    if config.debug.egui {
        app.add_plugins(EguiPlugin::default());
        if config.debug.inspector {
            app.add_plugins(WorldInspectorPlugin::new());
        }
    }

    app.add_plugins(RoboMasterPlugins)
        .add_plugins(DatasetPlugin)
        .add_plugins(ConfigPlugin)
        .init_resource::<CameraMode>()
        .init_resource::<ProjectileStatistics>()
        .init_resource::<ChassisObservationFrame>()
        .init_resource::<PreviousKinematicState>()
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
        .add_observer(setup_dart_launch)
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
            update_chassis_observation.after(TransformSystems::Propagate),
        )
        .add_systems(
            PostUpdate,
            projectile_launch
                .after(TransformSystems::Propagate)
                .run_if(|keyboard: Res<ButtonInput<KeyCode>>| keyboard.pressed(KeyCode::Space)),
        )
        .add_systems(
            PostUpdate,
            dart_launch
                .after(TransformSystems::Propagate)
                .run_if(|keyboard: Res<ButtonInput<KeyCode>>| keyboard.just_pressed(KeyCode::KeyG)),
        )
        .add_systems(PostUpdate, uav_launch.after(TransformSystems::Propagate))
        .add_systems(FixedUpdate, projectile_aerodynamics);

    if config.debug.diagnostics {
        app.add_plugins((
            FrameTimeDiagnosticsPlugin::default(),
            LogDiagnosticsPlugin::default(),
        ));
    }

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
        if should_enable_talos_plugin(&app) {
            app.add_plugins(TalosPlugin::default());
            info!("talos integration enabled");
        } else {
            info!(
                "talos integration skipped: ROS2 capture already active \
                 (set DAEDALUS_FORCE_TALOS_CAPTURE=1 to override)"
            );
        }
    }

    app.run();
}
