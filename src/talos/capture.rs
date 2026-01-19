use crate::capture::{
    CameraFov, CaptureSource, ImageHandle, compute_camera_intrinsics,
    driver::{CameraCapturePlugin, CaptureConfig, GpuCaptureHandler, SnapshotAsync, SnapshotSync},
    setup_capture_camera, setup_preview_window, sync_capture_camera,
};
use crate::components::{Controlled, InfantryGimbal, InfantryLaunchOffset};
use crate::dataset::prelude::DatasetSnapshotCreator;
use crate::talos::layout::*;
use crate::talos::plugin::{to_ros_quat, to_ros_translation};
use crate::talos::publisher::ShmPublisher;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use bevy::render::{Extract, ExtractSchedule, RenderApp};
use std::f32::consts::PI;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

static FRAME_SEQ: AtomicU64 = AtomicU64::new(0);

/// Shared timestamp for synchronizing pose and image publication.
/// Updated by TalosSnapshotCreator, read by publish_gimbal_pose_system.
pub static SHARED_TIMESTAMP_NS: AtomicU64 = AtomicU64::new(0);

/// Extracted pose data from MainApp to RenderApp for synchronized publishing
#[derive(Resource, Clone, Default)]
pub struct ExtractedPoseData {
    pub gimbal_translation: Vec3,
    pub gimbal_rotation: Quat,
    pub muzzle_rel_translation: Vec3,
    pub camera_rel_translation: Vec3,
    pub valid: bool,
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

struct TalosSnapshotSync {
    timestamp_ns: u64,
}

impl SnapshotSync for TalosSnapshotSync {
    fn captured(
        self: Box<Self>,
        world: &mut DeferredWorld,
        _config: &CaptureConfig,
    ) -> Box<dyn SnapshotAsync> {
        // Store timestamp for backward compatibility
        SHARED_TIMESTAMP_NS.store(self.timestamp_ns, Ordering::Release);

        let ctx = world.resource::<TalosCaptureContextShared>().0.clone();
        let pose_data = world.resource::<ExtractedPoseData>().clone();

        // Publish pose data with the same timestamp as the image
        if pose_data.valid {
            if let Ok(mut publisher) = ctx.lock() {
                // Odom
                let gimbal_ros = to_ros_translation(pose_data.gimbal_translation);
                publisher.publish_pose(
                    PoseIndex::Odom,
                    [gimbal_ros.x, gimbal_ros.y, gimbal_ros.z],
                    [1.0, 0.0, 0.0, 0.0],
                    self.timestamp_ns,
                );

                // Gimbal rotation
                let gimbal_rot = to_ros_quat(pose_data.gimbal_rotation);
                publisher.publish_pose(
                    PoseIndex::Gimbal,
                    [0.0, 0.0, 0.0],
                    [gimbal_rot.w, gimbal_rot.x, gimbal_rot.y, gimbal_rot.z],
                    self.timestamp_ns,
                );

                // Muzzle
                let muzzle = to_ros_translation(pose_data.muzzle_rel_translation);
                publisher.publish_pose(
                    PoseIndex::Muzzle,
                    [muzzle.x, muzzle.y, muzzle.z],
                    [1.0, 0.0, 0.0, 0.0],
                    self.timestamp_ns,
                );

                // Camera
                let camera = to_ros_translation(pose_data.camera_rel_translation);
                publisher.publish_pose(
                    PoseIndex::Camera,
                    [camera.x, camera.y, camera.z],
                    [1.0, 0.0, 0.0, 0.0],
                    self.timestamp_ns,
                );
            }
        }

        Box::new(TalosSnapshot {
            ctx,
            timestamp_ns: self.timestamp_ns,
        })
    }
}

struct TalosSnapshot {
    ctx: Arc<Mutex<ShmPublisher>>,
    timestamp_ns: u64,
}

impl SnapshotAsync for TalosSnapshot {
    fn captured(&mut self, width: u32, height: u32, image: &[u8]) {
        let expected_size = (width * height * 3) as usize;
        if image.len() != expected_size {
            warn!(
                "图像大小不匹配: expected {} bytes, got {} bytes",
                expected_size,
                image.len()
            );
            return;
        }

        if width != IMAGE_WIDTH || height != IMAGE_HEIGHT {
            warn!(
                "image reesolution mismatched: expected {}x{}, got {}x{}",
                IMAGE_WIDTH, IMAGE_HEIGHT, width, height
            );
            return;
        }

        let seq = FRAME_SEQ.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut publisher) = self.ctx.lock() {
            publisher.publish_image(image, seq, self.timestamp_ns);
        }
    }
}

#[derive(Default)]
struct TalosSnapshotCreator {}

impl GpuCaptureHandler for TalosSnapshotCreator {
    fn captured(&self, _world: &World) -> Option<Box<dyn SnapshotSync>> {
        Some(Box::new(TalosSnapshotSync {
            timestamp_ns: now_ns(),
        }))
    }
}

#[derive(Resource, Clone, Deref, DerefMut)]
pub struct TalosCaptureContextShared(pub Arc<Mutex<ShmPublisher>>);

#[derive(Resource, Clone)]
pub struct TalosCaptureContext {
    pub publisher: Arc<Mutex<ShmPublisher>>,
    pub fov_y: f32,
}

pub struct TalosCapturePlugin {
    pub config: CaptureConfig,
    pub context: TalosCaptureContext,
}

impl Plugin for TalosCapturePlugin {
    fn build(&self, app: &mut App) {
        let (plugin, render_target_handle) = CameraCapturePlugin::new(
            app,
            self.config.clone(),
            vec![
                Box::new(TalosSnapshotCreator::default()),
                Box::new(DatasetSnapshotCreator::default()),
            ],
        );

        {
            let mut publisher = self.context.publisher.lock().unwrap();
            let intrinsics = compute_camera_intrinsics(
                self.config.width,
                self.config.height,
                self.context.fov_y,
            );

            publisher.set_camera_info(CameraInfo {
                timestamp_ns: now_ns(),
                fx: intrinsics.fx,
                fy: intrinsics.fy,
                cx: intrinsics.cx,
                cy: intrinsics.cy,
                distortion: [0.0; 5],
                width: intrinsics.width,
                height: intrinsics.height,
                _pad: [0; 24],
            });
        }

        app.add_plugins(plugin)
            .insert_resource(ImageHandle(render_target_handle))
            .insert_resource(CameraFov(self.context.fov_y))
            .insert_resource(self.context.clone())
            .add_systems(Startup, setup_capture_camera)
            .add_systems(Startup, setup_preview_window)
            .add_systems(Update, sync_capture_camera);

        app.sub_app_mut(RenderApp)
            .insert_resource(TalosCaptureContextShared(self.context.publisher.clone()))
            .insert_resource(self.context.clone())
            .insert_resource(ExtractedPoseData::default())
            .add_systems(ExtractSchedule, extract_pose_data);
    }
}

/// Extract pose data from MainApp to RenderApp
fn extract_pose_data(
    mut pose_data: ResMut<ExtractedPoseData>,
    camera: Extract<Query<&GlobalTransform, With<CaptureSource>>>,
    gimbal: Extract<Query<&GlobalTransform, (With<Controlled>, With<InfantryGimbal>)>>,
    muzzle_offset: Extract<
        Query<(&GlobalTransform, &Transform), (With<InfantryLaunchOffset>, With<Controlled>)>,
    >,
) {
    let Ok(cam_transform) = camera.single() else {
        pose_data.valid = false;
        return;
    };
    let Ok(gimbal_transform) = gimbal.single() else {
        pose_data.valid = false;
        return;
    };
    let Ok((muzzle_global, muzzle_local)) = muzzle_offset.single() else {
        pose_data.valid = false;
        return;
    };

    let cam_rel = cam_transform.reparented_to(gimbal_transform);
    let muzzle_rel = muzzle_global.reparented_to(gimbal_transform);

    // Compute gimbal rotation (same as in plugin.rs)
    let gimbal_rot = gimbal_transform.rotation()
        * muzzle_local.rotation
        * Quat::from_euler(EulerRot::ZYX, 0.0, 0.0, PI / 2.0);

    pose_data.gimbal_translation = gimbal_transform.translation();
    pose_data.gimbal_rotation = gimbal_rot;
    pose_data.muzzle_rel_translation = muzzle_rel.translation;
    pose_data.camera_rel_translation = cam_rel.translation;
    pose_data.valid = true;
}
