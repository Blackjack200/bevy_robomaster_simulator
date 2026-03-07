use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender, unbounded};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;
use std::path::Path;

#[derive(Resource, Deserialize, Reflect, Clone)]
#[reflect(Resource)]
pub struct SimulationConfig {
    #[serde(default)]
    pub window: WindowConfig,
    #[serde(default)]
    pub debug: DebugConfig,
    #[serde(default)]
    pub preview: PreviewConfig,
    #[serde(default)]
    pub render: RenderConfig,
    #[serde(default)]
    pub capture: CapturePipelineConfig,
    #[serde(default)]
    pub livox_ros: LivoxRosConfig,
    pub physics: PhysicsConfig,
    pub vehicle: VehicleConfig,
    #[serde(default)]
    pub mecanum: MecanumConfig,
    pub projectile: ProjectileConfig,
    pub camera: CameraConfig,
}

#[derive(Deserialize, Reflect, Clone)]
pub struct WindowConfig {
    pub present_mode: String,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            // Uncap rendering by default so off-screen capture (Talos/ROS2) can exceed 60Hz.
            present_mode: "auto_no_vsync".to_string(),
        }
    }
}

#[derive(Deserialize, Reflect, Clone)]
#[serde(default)]
pub struct DebugConfig {
    pub egui: bool,
    pub inspector: bool,
    pub diagnostics: bool,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            egui: false,
            inspector: false,
            diagnostics: false,
        }
    }
}

#[derive(Deserialize, Reflect, Clone)]
pub struct PreviewConfig {
    pub enabled: bool,
}

impl Default for PreviewConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Deserialize, Reflect, Clone)]
#[serde(default)]
pub struct RenderConfig {
    pub shadows: bool,
    pub main_camera_fxaa: bool,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            shadows: false,
            main_camera_fxaa: false,
        }
    }
}

#[derive(Deserialize, Reflect, Clone)]
pub struct PhysicsConfig {
    pub substep_count: u32,
}

#[derive(Deserialize, Reflect, Clone)]
pub struct VehicleConfig {
    pub rotation_speed: f32,
    pub gimbal_rotation_speed: f32,
    pub gimbal_pitch_limit: f32,
    pub max_speed: f32,
    pub linear_acceleration: f32,
    pub acceleration_exponent: f32,
}

#[derive(Deserialize, Reflect, Clone)]
#[serde(default)]
pub struct MecanumConfig {
    pub wheel_radius_m: f32,
    pub half_wheelbase_m: f32,
    pub half_trackwidth_m: f32,
}

impl Default for MecanumConfig {
    fn default() -> Self {
        Self {
            wheel_radius_m: 0.076,
            half_wheelbase_m: 0.18,
            half_trackwidth_m: 0.15,
        }
    }
}

#[derive(Deserialize, Reflect, Clone)]
pub struct ProjectileConfig {
    pub lifetime: f32,
    pub speed: f32,
    pub cooldown: f32,
    pub diameter: f32,
    pub mass: f32,
    pub friction: f32,
    pub linear_damping: f32,
    #[serde(default)]
    pub aerodynamics: ProjectileAerodynamicsConfig,
}

#[derive(Deserialize, Reflect, Clone)]
pub struct ProjectileAerodynamicsConfig {
    pub enabled: bool,
    pub air_density: f32,
    pub drag_coefficient: f32,
    pub wind: [f32; 3],
}

impl Default for ProjectileAerodynamicsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            // kg/m^3 - air density at sea level (15°C)
            air_density: 1.225,
            // Drag coefficient for a smooth sphere, typical Re for 17mm @ ~25m/s.
            drag_coefficient: 0.47,
            // m/s - wind velocity in world coordinates.
            wind: [0.0, 0.0, 0.0],
        }
    }
}

#[derive(Deserialize, Reflect, Clone)]
pub struct CameraConfig {
    pub fov: f32,
    pub free_move_speed: f32,
    pub follow_offset: [f32; 3],
    pub mouse_sensitivity: f32,
}

#[derive(Deserialize, Reflect, Clone)]
#[serde(default)]
pub struct CapturePipelineConfig {
    pub color: CaptureStreamConfig,
    pub depth: DepthCaptureConfig,
}

impl Default for CapturePipelineConfig {
    fn default() -> Self {
        Self {
            color: CaptureStreamConfig::default(),
            depth: DepthCaptureConfig::default(),
        }
    }
}

#[derive(Deserialize, Reflect, Clone)]
#[serde(default)]
pub struct CaptureStreamConfig {
    pub width: u32,
    pub height: u32,
}

impl Default for CaptureStreamConfig {
    fn default() -> Self {
        Self {
            width: 1440,
            height: 1080,
        }
    }
}

#[derive(Deserialize, Reflect, Clone)]
#[serde(default)]
pub struct DepthCaptureConfig {
    pub width: u32,
    pub height: u32,
    pub near: f32,
    pub far: f32,
}

impl Default for DepthCaptureConfig {
    fn default() -> Self {
        Self {
            width: 640,
            height: 480,
            near: 0.1,
            far: 80.0,
        }
    }
}

#[derive(Deserialize, Reflect, Clone)]
#[serde(default)]
pub struct LivoxRosConfig {
    pub enabled: bool,
    pub frame_id: String,
    pub publish_freq: f32,
    pub points_per_second: u32,
    pub line_num: u8,
    pub tag_default: u8,
    pub intensity_default: f32,
}

impl Default for LivoxRosConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            frame_id: "livox_frame".to_string(),
            publish_freq: 10.0,
            points_per_second: 100_000,
            line_num: 6,
            tag_default: 0,
            intensity_default: 100.0,
        }
    }
}

impl SimulationConfig {
    pub fn load() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let content = std::fs::read_to_string("config.toml")?;
        Ok(toml::from_str(&content)?)
    }
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self::load().unwrap_or_else(|e| {
            warn!("Failed to load config.toml: {}, using defaults", e);
            Self {
                window: WindowConfig::default(),
                debug: DebugConfig::default(),
                preview: PreviewConfig::default(),
                render: RenderConfig::default(),
                capture: CapturePipelineConfig::default(),
                livox_ros: LivoxRosConfig::default(),
                physics: PhysicsConfig { substep_count: 10 },
                vehicle: VehicleConfig {
                    rotation_speed: 3.0,
                    gimbal_rotation_speed: 3.0,
                    gimbal_pitch_limit: 0.785,
                    max_speed: 4.0,
                    linear_acceleration: 8.0,
                    acceleration_exponent: 10.0,
                },
                mecanum: MecanumConfig::default(),
                projectile: ProjectileConfig {
                    lifetime: 5.0,
                    speed: 25.0,
                    cooldown: 0.1,
                    diameter: 0.017,
                    mass: 0.017,
                    friction: 1.1,
                    linear_damping: 0.0,
                    aerodynamics: ProjectileAerodynamicsConfig::default(),
                },
                camera: CameraConfig {
                    fov: 45.0,
                    free_move_speed: 8.0,
                    follow_offset: [0.0, 3.0, 2.0],
                    mouse_sensitivity: 0.003,
                },
            }
        })
    }
}

#[derive(Resource)]
pub struct ConfigWatcher {
    _watcher: RecommendedWatcher,
    receiver: Receiver<Result<Event, notify::Error>>,
}

pub struct ConfigPlugin;

impl Plugin for ConfigPlugin {
    fn build(&self, app: &mut App) {
        let config = SimulationConfig::default();

        // Set up file watcher using crossbeam-channel for thread safety
        let (tx, rx): (
            Sender<Result<Event, notify::Error>>,
            Receiver<Result<Event, notify::Error>>,
        ) = unbounded();
        let watcher_result = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            notify::Config::default(),
        );

        match watcher_result {
            Ok(mut watcher) => {
                if let Err(e) = watcher.watch(Path::new("config.toml"), RecursiveMode::NonRecursive)
                {
                    warn!("Failed to watch config.toml: {}", e);
                } else {
                    info!("Config hot-reload enabled for config.toml");
                    app.insert_resource(ConfigWatcher {
                        _watcher: watcher,
                        receiver: rx,
                    });
                    app.add_systems(Update, config_hot_reload);
                }
            }
            Err(e) => {
                warn!("Failed to create config watcher: {}", e);
            }
        }

        app.insert_resource(config)
            .register_type::<SimulationConfig>();
    }
}

fn config_hot_reload(mut config: ResMut<SimulationConfig>, watcher: Option<Res<ConfigWatcher>>) {
    let Some(watcher) = watcher else {
        return;
    };

    // Non-blocking check for file changes
    while let Ok(Ok(event)) = watcher.receiver.try_recv() {
        if event.kind.is_modify() {
            match SimulationConfig::load() {
                Ok(new_config) => {
                    info!("Config reloaded successfully");
                    *config = new_config;
                }
                Err(e) => {
                    warn!("Failed to reload config: {}", e);
                }
            }
        }
    }
}
