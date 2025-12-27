use crate::capture::{
    CameraFov, ImageHandle, compute_camera_intrinsics,
    driver::{CameraCapturePlugin, CaptureConfig, GpuCaptureHandler, SnapshotAsync, SnapshotSync},
    setup_capture_camera, sync_capture_camera,
};
use crate::dataset::prelude::DatasetSnapshotCreator;
use crate::talos::layout::*;
use crate::talos::publisher::ShmPublisher;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use bevy::render::RenderApp;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

static FRAME_SEQ: AtomicU64 = AtomicU64::new(0);

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

struct TalosSnapshotSync;

impl SnapshotSync for TalosSnapshotSync {
    fn captured(
        self: Box<Self>,
        world: &mut DeferredWorld,
        _config: &CaptureConfig,
    ) -> Box<dyn SnapshotAsync> {
        let ctx = world.resource::<TalosCaptureContextShared>().0.clone();
        Box::new(TalosSnapshot { ctx })
    }
}

struct TalosSnapshot {
    ctx: Arc<Mutex<ShmPublisher>>,
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
            let timestamp_ns = now_ns();
            publisher.publish_image(image, seq, timestamp_ns);
        }
    }
}

#[derive(Default)]
struct TalosSnapshotCreator {}

impl GpuCaptureHandler for TalosSnapshotCreator {
    fn captured(&self, _world: &World) -> Option<Box<dyn SnapshotSync>> {
        Some(Box::new(TalosSnapshotSync))
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
            .add_systems(Update, sync_capture_camera);

        app.sub_app_mut(RenderApp)
            .insert_resource(TalosCaptureContextShared(self.context.publisher.clone()))
            .insert_resource(self.context.clone());
    }
}
