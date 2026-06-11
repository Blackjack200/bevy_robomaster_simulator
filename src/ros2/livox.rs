use crate::capture::CaptureBundle;
use crate::capture::compute_camera_intrinsics;
use crate::capture::depth::{
    DepthCameraSettings, setup_depth_capture_camera, sync_depth_capture_camera,
};
use crate::capture::driver::{
    CaptureConfig, CapturedFrame, CapturedFrameKind, GpuCaptureHandler, SnapshotAsync, SnapshotSync,
};
use crate::ros2::topic::{LivoxPointCloudTopic, TopicPublisher};
use crate::systems::GameplaySystems;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use bevy::render::{RenderApp, RenderSystems};
use r2r::Clock;
use r2r::sensor_msgs::msg::{PointCloud2, PointField};
use r2r::std_msgs::msg::Header;
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Resource, Clone, Deref, DerefMut)]
pub struct RosLivoxContextShared(pub Arc<RosLivoxContext>);

#[derive(Resource, Clone)]
pub struct RosLivoxContext {
    pub clock: Arc<Mutex<Clock>>,
    pub frame_id: String,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
    pub publish_period_ns: u64,
    pub points_per_publish: usize,
    pub line_num: u8,
    pub tag_default: u8,
    pub intensity_default: f32,
    pub pointcloud: TopicPublisher<LivoxPointCloudTopic>,
    pub last_publish_ns: Arc<AtomicU64>,
}

pub struct RosLivoxPlugin {
    pub config: CaptureConfig,
    pub context: RosLivoxContext,
}

fn stamp_to_ns(stamp: &r2r::builtin_interfaces::msg::Time) -> u64 {
    let sec = stamp.sec.max(0) as u64;
    sec.saturating_mul(1_000_000_000) + stamp.nanosec as u64
}

fn point_fields() -> Vec<PointField> {
    vec![
        PointField {
            name: "x".to_string(),
            offset: 0,
            datatype: PointField::FLOAT32 as u8,
            count: 1,
        },
        PointField {
            name: "y".to_string(),
            offset: 4,
            datatype: PointField::FLOAT32 as u8,
            count: 1,
        },
        PointField {
            name: "z".to_string(),
            offset: 8,
            datatype: PointField::FLOAT32 as u8,
            count: 1,
        },
        PointField {
            name: "intensity".to_string(),
            offset: 12,
            datatype: PointField::FLOAT32 as u8,
            count: 1,
        },
        PointField {
            name: "tag".to_string(),
            offset: 16,
            datatype: PointField::UINT8 as u8,
            count: 1,
        },
        PointField {
            name: "line".to_string(),
            offset: 17,
            datatype: PointField::UINT8 as u8,
            count: 1,
        },
    ]
}

fn linearize_reverse_z(depth: f32, near: f32) -> f32 {
    if depth <= f32::EPSILON {
        return f32::INFINITY;
    }
    near / depth
}

struct RosLivoxSnapshotSync {
    stamp: RefCell<r2r::builtin_interfaces::msg::Time>,
}

impl SnapshotSync for RosLivoxSnapshotSync {
    fn captured(
        self: Box<Self>,
        world: &mut DeferredWorld,
        _config: &CaptureConfig,
    ) -> Box<dyn SnapshotAsync> {
        Box::new(RosLivoxSnapshot {
            stamp: self.stamp,
            ctx: world.resource::<RosLivoxContextShared>().0.clone(),
        })
    }
}

struct RosLivoxSnapshot {
    stamp: RefCell<r2r::builtin_interfaces::msg::Time>,
    ctx: Arc<RosLivoxContext>,
}

impl SnapshotAsync for RosLivoxSnapshot {
    fn captured(&mut self, frame: CapturedFrame<'_>) {
        if frame.kind != CapturedFrameKind::Depth32F {
            return;
        }
        if frame.width == 0 || frame.height == 0 || frame.data.len() < 4 {
            return;
        }

        let intrinsics = compute_camera_intrinsics(frame.width, frame.height, self.ctx.fov_y);
        let pixel_count = frame.data.len() / 4;
        let target_points = self.ctx.points_per_publish.max(1);
        let sample_step = ((pixel_count as f32 / target_points as f32).ceil() as usize).max(1);
        let line_num = self.ctx.line_num.max(1);
        let mut data = Vec::with_capacity(target_points * 18);
        let mut valid_points = 0usize;

        for idx in (0..pixel_count).step_by(sample_step) {
            let off = idx * 4;
            let depth = f32::from_le_bytes([
                frame.data[off],
                frame.data[off + 1],
                frame.data[off + 2],
                frame.data[off + 3],
            ]);
            if !depth.is_finite() {
                continue;
            }
            let z = linearize_reverse_z(depth, self.ctx.near);
            if !z.is_finite() || z <= self.ctx.near || z > self.ctx.far {
                continue;
            }

            let u = (idx as u32 % frame.width) as f32;
            let v = (idx as u32 / frame.width) as f32;
            let x = ((u - intrinsics.cx as f32) / intrinsics.fx as f32) * z;
            let y = ((v - intrinsics.cy as f32) / intrinsics.fy as f32) * z;

            let x_livox = z;
            let y_livox = -x;
            let z_livox = -y;
            let line = ((v * line_num as f32 / frame.height as f32).floor() as i32)
                .clamp(0, line_num as i32 - 1) as u8;

            data.extend_from_slice(&x_livox.to_le_bytes());
            data.extend_from_slice(&y_livox.to_le_bytes());
            data.extend_from_slice(&z_livox.to_le_bytes());
            data.extend_from_slice(&self.ctx.intensity_default.to_le_bytes());
            data.push(self.ctx.tag_default);
            data.push(line);
            valid_points += 1;
            if valid_points >= target_points {
                break;
            }
        }

        if valid_points == 0 {
            return;
        }

        self.ctx.pointcloud.publish(PointCloud2 {
            header: Header {
                stamp: self.stamp.take(),
                frame_id: self.ctx.frame_id.clone(),
            },
            height: 1,
            width: valid_points as u32,
            fields: point_fields(),
            is_bigendian: false,
            point_step: 18,
            row_step: (valid_points as u32) * 18,
            data,
            is_dense: true,
        });
    }
}

#[derive(Default)]
struct RosLivoxSnapshotCreator;

impl GpuCaptureHandler for RosLivoxSnapshotCreator {
    fn captured(&self, world: &World) -> Option<Box<dyn SnapshotSync>> {
        let ctx = world.resource::<RosLivoxContextShared>().0.clone();
        let now = ctx.clock.lock().ok()?.get_now().ok()?;
        let stamp = Clock::to_builtin_time(&now);
        let now_ns = stamp_to_ns(&stamp);
        let last = ctx.last_publish_ns.load(Ordering::Relaxed);
        if now_ns < last.saturating_add(ctx.publish_period_ns) {
            return None;
        }
        if ctx
            .last_publish_ns
            .compare_exchange(last, now_ns, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return None;
        }
        Some(Box::new(RosLivoxSnapshotSync {
            stamp: RefCell::new(stamp),
        }))
    }
}

impl Plugin for RosLivoxPlugin {
    fn build(&self, app: &mut App) {
        let capture = CaptureBundle::depth_from_camera_order(
            app,
            self.config.clone(),
            crate::capture::depth::DEPTH_CAPTURE_CAMERA_ORDER,
            vec![Box::new(RosLivoxSnapshotCreator)],
        );

        app.add_plugins(capture)
            .insert_resource(DepthCameraSettings {
                width: self.config.width,
                height: self.config.height,
                fov_y: self.context.fov_y,
                near: self.context.near,
                far: self.context.far,
            })
            .insert_resource(self.context.clone())
            .add_systems(Startup, setup_depth_capture_camera)
            .add_systems(
                Update,
                sync_depth_capture_camera
                    .after(GameplaySystems::Camera)
                    .before(RenderSystems::Render),
            );

        app.sub_app_mut(RenderApp)
            .insert_resource(self.context.clone())
            .insert_resource(RosLivoxContextShared(Arc::new(self.context.clone())));
    }
}
