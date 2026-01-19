use crate::capture::driver::CaptureConfig;
use crate::capture::{CaptureSource, IMAGE_HEIGHT, IMAGE_WIDTH};
use crate::components::{
    Controlled, InfantryChassis, InfantryGimbal, InfantryLaunchOffset, SubscribeAutoAim,
};
use crate::config::SimulationConfig;
use crate::systems::projectile_launch;
use crate::talos::capture::{TalosCaptureContext, TalosCapturePlugin};
use crate::talos::layout::*;
use crate::talos::publisher::ShmPublisher;
use crate::talos::subscriber::ShmSubscriber;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use std::f32::consts::PI;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Resource)]
pub struct ShmSubscriberRes(pub Arc<Mutex<ShmSubscriber>>);

#[derive(Resource, Deref, DerefMut)]
pub struct TalosEnabled(pub AtomicBool);

#[derive(Resource, Clone, Default)]
struct CachedPoseData {
    odom_translation: [f32; 3],
    odom_quaternion: [f32; 4],
    gimbal_quaternion: [f32; 4],
    muzzle_translation: [f32; 3],
    camera_translation: [f32; 3],
    valid: bool,
}

#[derive(Resource)]
struct HighFrequencyTimer {
    accumulator: f32,
}

impl Default for HighFrequencyTimer {
    fn default() -> Self {
        Self { accumulator: 0.0 }
    }
}

pub struct TalosPluginConfig {
    pub width: u32,
    pub height: u32,
    pub fov_y: f32,
    pub texture_format: TextureFormat,
}

impl Default for TalosPluginConfig {
    fn default() -> Self {
        let config = SimulationConfig::default();
        Self {
            width: IMAGE_WIDTH,
            height: IMAGE_HEIGHT,
            fov_y: config.camera.fov.to_radians(),
            texture_format: TextureFormat::bevy_default(),
        }
    }
}

#[derive(Default)]
pub struct TalosPlugin {
    pub config: TalosPluginConfig,
}

impl Plugin for TalosPlugin {
    fn build(&self, app: &mut App) {
        let publisher = match ShmPublisher::create() {
            Ok(p) => {
                info!("talos shm created");
                p
            }
            Err(e) => {
                error!("cannot create talos shm: {}", e);
                return;
            }
        };

        let publisher = Arc::new(Mutex::new(publisher));

        let capture_config = CaptureConfig {
            width: self.config.width,
            height: self.config.height,
            texture_format: self.config.texture_format,
        };

        let capture_context = TalosCaptureContext {
            publisher: publisher.clone(),
            fov_y: self.config.fov_y,
        };

        app.add_plugins(TalosCapturePlugin {
            config: capture_config,
            context: capture_context,
        });

        match ShmSubscriber::connect() {
            Ok(subscriber) => {
                info!("connected to talos-cpp");
                app.insert_resource(ShmSubscriberRes(Arc::new(Mutex::new(subscriber))));
            }
            Err(_) => {
                info!("could not connect to talos-cpp");
            }
        }

        app.insert_resource(TalosEnabled(AtomicBool::new(true)));
        app.insert_resource(CachedPoseData::default());
        app.insert_resource(HighFrequencyTimer::default());
        app.add_systems(Last, heartbeat_system);
        app.add_systems(
            Last,
            (
                process_subscription
                    .run_if(|enabled: Res<SubscribeAutoAim>| enabled.load(Ordering::Acquire)),
                publish_gimbal_pose_system,
            ),
        );
        app.add_systems(Update, high_frequency_publish_system);
    }
}

fn process_subscription(
    context: Option<Res<ShmSubscriberRes>>,
    mut commands: Commands,
    gimbal: Single<
        (&mut Transform, &mut InfantryGimbal),
        (
            With<Controlled>,
            Without<InfantryChassis>,
            Without<InfantryLaunchOffset>,
        ),
    >,
    muzzle_offset: Single<
        (&GlobalTransform, &Transform),
        (With<InfantryLaunchOffset>, With<Controlled>),
    >,
) {
    let Some(ctx) = context else {
        return;
    };
    let (mut gimbal_transform, mut gimbal_data) = gimbal.into_inner();

    let Some(cmd) = recv_gimbal_cmd(&ctx) else {
        return;
    };
    if cmd.distance_m == -1.0 {
        return;
    }
    if cmd.fire_advice == 1 {
        commands.queue(|w: &mut World| {
            w.run_system_once(projectile_launch).unwrap();
        });
    }
    let yaw_f32 = (cmd.yaw_deg).to_radians();
    let pitch_f32 = (-cmd.pitch_deg - 90.0).to_radians();
    gimbal_data.local_yaw = yaw_f32;
    gimbal_data.pitch = pitch_f32;
    let expected_rotation = Quat::from_euler(EulerRot::YXZ, yaw_f32, pitch_f32, 0.0);
    let current_rotation = muzzle_offset.0.rotation();
    let delta = expected_rotation * current_rotation.inverse();
    gimbal_transform.rotation = delta * gimbal_transform.rotation;
    //info!("yaw={} pitch={}", cmd.yaw_deg, cmd.pitch_deg);
}

fn heartbeat_system(context: Option<Res<TalosCaptureContext>>) {
    if let Some(ctx) = context {
        if let Ok(mut publisher) = ctx.publisher.lock() {
            publisher.update_heartbeat();
        }
    }
}

pub fn publish_pose(
    context: &TalosCaptureContext,
    index: PoseIndex,
    position: [f32; 3],
    quaternion: [f32; 4],
    timestamp_ns: u64,
) {
    if let Ok(mut publisher) = context.publisher.lock() {
        publisher.publish_pose(index, position, quaternion, timestamp_ns);
    }
}

pub fn recv_gimbal_cmd(subscriber: &ShmSubscriberRes) -> Option<GimbalCmd> {
    subscriber.0.lock().ok()?.recv_gimbal_cmd()
}

pub const M_ALIGN_MAT3: Mat3 = Mat3::from_cols(
    Vec3::new(0.0, -1.0, 0.0), // M[0,0], M[1,0], M[2,0]
    Vec3::new(0.0, 0.0, 1.0),  // M[0,1], M[1,1], M[2,1]
    Vec3::new(-1.0, 0.0, 0.0), // M[0,2], M[1,2], M[2,2]
);

#[inline]
pub fn to_ros(bevy_transform: Transform) -> Transform {
    let new_rotation = to_ros_quat(bevy_transform.rotation);
    let new_translation = to_ros_translation(bevy_transform.translation);
    Transform::from_translation(new_translation).with_rotation(new_rotation)
}

pub fn to_ros_translation(vec3: Vec3) -> Vec3 {
    let align_rot_mat = M_ALIGN_MAT3;
    let new_translation = align_rot_mat * vec3;
    new_translation
}

pub fn to_ros_quat(quat: Quat) -> Quat {
    let align_rot_mat = M_ALIGN_MAT3;
    let align_quat = Quat::from_mat3(&align_rot_mat);
    let new_rotation = align_quat * quat * align_quat.inverse();
    new_rotation
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

fn publish_gimbal_pose_system(
    mut cached_pose: ResMut<CachedPoseData>,
    camera: Single<&GlobalTransform, With<CaptureSource>>,
    gimbal: Single<&GlobalTransform, (With<Controlled>, With<InfantryGimbal>)>,
    muzzle_offset: Single<
        (&GlobalTransform, &Transform),
        (With<InfantryLaunchOffset>, With<Controlled>),
    >,
) {
    let cam_transform = camera.into_inner();
    let gimbal = gimbal.into_inner();
    let cam_rel = cam_transform.reparented_to(gimbal);
    let muzzle_rel = muzzle_offset.0.reparented_to(gimbal);

    let gimbal_ros = to_ros_translation(gimbal.translation());
    cached_pose.odom_translation = [gimbal_ros.x, gimbal_ros.y, gimbal_ros.z];
    cached_pose.odom_quaternion = [1.0, 0.0, 0.0, 0.0];

    let gimbal_rot = gimbal.rotation()
        * muzzle_offset.1.rotation
        * Quat::from_euler(EulerRot::ZYX, 0.0, 0.0, PI / 2.0);
    let gimbal_rot = to_ros_quat(gimbal_rot);
    cached_pose.gimbal_quaternion = [gimbal_rot.w, gimbal_rot.x, gimbal_rot.y, gimbal_rot.z];

    let muzzle = to_ros_translation(muzzle_rel.translation);
    cached_pose.muzzle_translation = [muzzle.x, muzzle.y, muzzle.z];

    let camera = to_ros_translation(cam_rel.translation);
    cached_pose.camera_translation = [camera.x, camera.y, camera.z];

    cached_pose.valid = true;
}

fn high_frequency_publish_system(
    mut timer: ResMut<HighFrequencyTimer>,
    time: Res<Time<Real>>,
    cached_pose: Res<CachedPoseData>,
    context: Option<Res<TalosCaptureContext>>,
) {
    if !cached_pose.valid {
        return;
    }

    let Some(ctx) = context else { return };

    timer.accumulator += time.delta().as_secs_f32();
    const INTERVAL_SECS: f32 = 1.0 / 1000.0;

    while timer.accumulator >= INTERVAL_SECS {
        timer.accumulator -= INTERVAL_SECS;
        let timestamp_ns = now_ns();

        publish_pose(
            &ctx,
            PoseIndex::Odom,
            cached_pose.odom_translation,
            cached_pose.odom_quaternion,
            timestamp_ns,
        );

        publish_pose(
            &ctx,
            PoseIndex::Gimbal,
            [0.0, 0.0, 0.0],
            cached_pose.gimbal_quaternion,
            timestamp_ns,
        );

        publish_pose(
            &ctx,
            PoseIndex::Muzzle,
            cached_pose.muzzle_translation,
            [1.0, 0.0, 0.0, 0.0],
            timestamp_ns,
        );

        publish_pose(
            &ctx,
            PoseIndex::Camera,
            cached_pose.camera_translation,
            [1.0, 0.0, 0.0, 0.0],
            timestamp_ns,
        );
    }
}
