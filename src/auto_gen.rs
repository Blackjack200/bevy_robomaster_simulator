//! Automatic dataset generation mode
//! Usage: cargo run -- --auto-gen

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::{Extract, RenderApp};

use crate::capture::driver::{CameraCapturePlugin, CaptureConfig, CapturedFrameKind};
use crate::capture::{
    CameraFov, CaptureSource, IMAGE_HEIGHT, IMAGE_WIDTH, ImageHandle, setup_capture_camera,
    sync_capture_camera,
};
use crate::components::Infantry;
use crate::dataset::prelude::{DatasetPlugin, capture};
use crate::robomaster::prelude::*;

// ==================== Configuration Parameters ====================
const DIST_MIN: f32 = 2.0;
const DIST_MAX: f32 = 8.0;
const DIST_STEP: f32 = 0.5;

const YAW_MIN: f32 = -std::f32::consts::PI; // -180°
const YAW_MAX: f32 = std::f32::consts::PI; // 180°
const YAW_STEP: f32 = 0.5236; // 30° in radians

const PITCH_MIN: f32 = -0.7854; // -45° in radians
const PITCH_MAX: f32 = 0.7854; // 45° in radians
const PITCH_STEP: f32 = 0.2618; // 15° in radians

const HEIGHT_OFFSET: f32 = 0.5;
const SETTLE_FRAMES: u32 = 5;
const FOV: f32 = 45.0;
// =================================================

#[derive(Component)]
struct AutoGenTarget;

#[derive(Resource, Clone)]
struct AutoGenState {
    distances: Vec<f32>,
    yaws: Vec<f32>,
    pitches: Vec<f32>,
    d_idx: usize,
    y_idx: usize,
    p_idx: usize,
    settle_counter: u32,
    frame_count: usize,
    capturing: bool,
}

pub struct AutoGenPlugin;

impl Plugin for AutoGenPlugin {
    fn build(&self, app: &mut App) {
        let capture_config = CaptureConfig {
            width: IMAGE_WIDTH,
            height: IMAGE_HEIGHT,
            texture_format: TextureFormat::Bgra8UnormSrgb,
            frame_kind: CapturedFrameKind::Rgb8,
        };

        use crate::dataset::prelude::DatasetSnapshotCreator;
        let (camera_capture_plugin, image_handle) = CameraCapturePlugin::new(
            app,
            capture_config.clone(),
            vec![Box::new(DatasetSnapshotCreator::default())],
        );

        app.add_plugins(camera_capture_plugin)
            .add_plugins(DatasetPlugin)
            .insert_resource(ImageHandle(image_handle))
            .insert_resource(CameraFov(FOV))
            .insert_resource(capture_config)
            .add_systems(Startup, (setup_auto_gen, setup_capture_camera))
            .add_systems(Update, (auto_gen_loop, sync_capture_camera));

        // Add capture system to RenderApp's ExtractSchedule
        app.sub_app_mut(RenderApp)
            .add_systems(ExtractSchedule, write_flag)
            .insert_resource(ShouldCapture(true, 0))
            .add_systems(ExtractSchedule, capture_condition);
    }
}
fn capture_condition(world: &mut World) {
    let mut res = world.resource_mut::<ShouldCapture>();
    if res.0 {
        res.0 = false;
        world.run_system_once(capture).unwrap();
    }
}

fn setup_auto_gen(mut commands: Commands, asset_server: Res<AssetServer>) {
    info!(
        "Config: dist {:.1}-{:.1} step {:.1}, yaw {:.1}-{:.1} step {:.1}, pitch {:.1}-{:.1} step {:.1}",
        DIST_MIN,
        DIST_MAX,
        DIST_STEP,
        YAW_MIN.to_degrees(),
        YAW_MAX.to_degrees(),
        YAW_STEP.to_degrees(),
        PITCH_MIN.to_degrees(),
        PITCH_MAX.to_degrees(),
        PITCH_STEP.to_degrees()
    );

    // Create ground
    commands.spawn((
        SceneRoot(asset_server.load("GROUND.glb#Scene0")),
        Transform::IDENTITY,
    ));

    // Create target robot
    commands.spawn((
        SceneRoot(asset_server.load("HERO.glb#Scene0")),
        Transform::from_xyz(0.0, 1.0, 0.0),
        Infantry(Team::Blue, HERO_ROBOT_CONFIG),
        ScanArmor(Team::Blue, HERO_ROBOT_CONFIG.0, HERO_ROBOT_CONFIG.1),
        AutoGenTarget,
    ));

    // Create capture source camera (this is what we move around)
    commands.spawn((
        Camera3d::default(),
        Camera {
            is_active: true,
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: FOV.to_radians(),
            near: 0.1,
            far: 100.0,
            ..default()
        }),
        Transform::from_xyz(3.0, 2.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
        CaptureSource,
        Name::new("AutoGenCamera"),
    ));

    // Initialize state
    let distances = gen_range(DIST_MIN, DIST_MAX, DIST_STEP);
    let yaws = gen_range(YAW_MIN, YAW_MAX, YAW_STEP);
    let pitches = gen_range(PITCH_MIN, PITCH_MAX, PITCH_STEP);
    let total = distances.len() * yaws.len() * pitches.len();

    info!("Total poses to capture: {}", total);

    commands.insert_resource(AutoGenState {
        distances,
        yaws,
        pitches,
        d_idx: 0,
        y_idx: 0,
        p_idx: 0,
        settle_counter: 0,
        frame_count: 0,
        capturing: false,
    });
}

fn auto_gen_loop(
    _commands: Commands,
    mut state: ResMut<AutoGenState>,
    mut camera: Single<&mut Transform, With<CaptureSource>>,
    target: Single<&GlobalTransform, With<AutoGenTarget>>,
) {
    // Wait for screenshot to complete
    if state.capturing {
        state.capturing = false;
        next_pose(&mut state);

        return;
    }

    // Wait for camera to settle
    if state.settle_counter > 0 {
        state.settle_counter -= 1;
        if state.settle_counter == 0 {
            // Capture frame - trigger the capture system
            state.capturing = true;
        }
        return;
    }

    // Check if complete
    if state.d_idx >= state.distances.len() {
        info!("=== Dataset Generation Complete! ===");
        info!("Total frames captured: {}", state.frame_count);
        std::process::exit(0);
    }

    // Move camera to next pose
    let dist = state.distances[state.d_idx];
    let yaw = state.yaws[state.y_idx];
    let pitch = state.pitches[state.p_idx];

    let target_pos = target.translation();
    let x = dist * yaw.cos() * pitch.cos();
    let y = dist * pitch.sin() + HEIGHT_OFFSET;
    let z = dist * yaw.sin() * pitch.cos();

    camera.translation = target_pos + Vec3::new(x, y, z);
    camera.look_at(target_pos, Vec3::Y);

    let done = state.d_idx * state.yaws.len() * state.pitches.len()
        + state.y_idx * state.pitches.len()
        + state.p_idx;
    let total = state.distances.len() * state.yaws.len() * state.pitches.len();

    if state.frame_count % 10 == 0
        || (state.d_idx == 0 && state.y_idx == 0 && state.p_idx == 0)
        || done == total - 1
    {
        info!(
            "Progress: {}/{} (dist={:.1}, yaw={:.1}°, pitch={:.1}°)",
            done + 1,
            total,
            dist,
            yaw.to_degrees(),
            pitch.to_degrees()
        );
    }

    state.settle_counter = SETTLE_FRAMES;
}

fn write_flag(q: Extract<Res<AutoGenState>>, mut r: ResMut<ShouldCapture>) {
    let old_frame_id = r.1;
    if q.p_idx != 0 && q.p_idx != old_frame_id {
        r.0 = true;
        r.1 = q.p_idx;
    } else {
        r.0 = false;
    }
}

#[derive(Resource)]
struct ShouldCapture(bool, usize);

fn next_pose(state: &mut AutoGenState) {
    state.frame_count += 1;
    state.p_idx += 1;
    if state.p_idx >= state.pitches.len() {
        state.p_idx = 0;
        state.y_idx += 1;
        if state.y_idx >= state.yaws.len() {
            state.y_idx = 0;
            state.d_idx += 1;
        }
    }
}

fn gen_range(min: f32, max: f32, step: f32) -> Vec<f32> {
    let mut v = Vec::new();
    let mut x = min;
    while x <= max + 0.0001 {
        v.push(x);
        x += step;
    }
    v
}
