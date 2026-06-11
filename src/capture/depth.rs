use crate::capture::{CaptureSource, copy_transform};
use bevy::camera::RenderTarget;
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::prelude::*;

pub const DEPTH_CAPTURE_CAMERA_ORDER: isize = -101;

#[derive(Resource, Clone, Copy)]
pub struct DepthCameraSettings {
    pub width: u32,
    pub height: u32,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
}

#[derive(Component)]
pub struct DepthCaptureCamera;

pub fn setup_depth_capture_camera(world: &mut World) {
    let depth_camera_exists = {
        let mut query = world.query_filtered::<Entity, With<DepthCaptureCamera>>();
        query.iter(world).next().is_some()
    };
    if depth_camera_exists {
        return;
    }

    let settings = *world.resource::<DepthCameraSettings>();

    world.spawn((
        Camera3d::default(),
        Camera {
            order: DEPTH_CAPTURE_CAMERA_ORDER,
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: settings.fov_y,
            near: settings.near,
            far: settings.far,
            ..default()
        }),
        RenderTarget::None {
            size: UVec2::new(settings.width, settings.height),
        },
        Msaa::Off,
        DepthPrepass,
        DepthCaptureCamera,
    ));
}

pub fn sync_depth_capture_camera(
    target: Single<&Transform, (With<CaptureSource>, Without<DepthCaptureCamera>)>,
    mut our: Single<&mut Transform, (With<DepthCaptureCamera>, Without<CaptureSource>)>,
) {
    copy_transform(&target, &mut our);
}
