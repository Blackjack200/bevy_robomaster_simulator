pub mod driver;

use bevy::anti_alias::fxaa::Fxaa;
use bevy::camera::RenderTarget;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::view::Hdr;

#[derive(Component)]
pub struct CaptureSource;

#[derive(Component)]
pub struct CaptureCamera;

#[derive(Resource, Deref, Clone)]
pub struct ImageHandle(pub Handle<Image>);

#[derive(Resource, Clone, Copy)]
pub struct CameraFov(pub f32);

pub fn setup_capture_camera(
    mut commands: Commands,
    render_target_handle: Res<ImageHandle>,
    fov: Res<CameraFov>,
) {
    commands.spawn((
        Camera3d::default(),
        Bloom::NATURAL,
        Tonemapping::None,
        RenderTarget::Image(render_target_handle.0.clone().into()),
        Camera { ..default() },
        Projection::Perspective(PerspectiveProjection {
            fov: fov.0,
            near: 0.1,
            far: 500000000.0,
            ..default()
        }),
        Msaa::Off,
        Fxaa::default(),
        Hdr,
        CaptureCamera,
    ));
}

pub fn sync_capture_camera(
    target: Single<&Transform, (With<CaptureSource>, Without<CaptureCamera>)>,
    mut our: Single<&mut Transform, (With<CaptureCamera>, Without<CaptureSource>)>,
) {
    our.translation = target.translation;
    our.scale = target.scale;
    our.rotation = target.rotation;
}

#[derive(Clone, Copy, Debug)]
pub struct CameraIntrinsics {
    pub fx: f64,
    pub fy: f64,
    pub cx: f64,
    pub cy: f64,
    pub width: u32,
    pub height: u32,
}

pub fn compute_camera_intrinsics(width: u32, height: u32, fov_y: f32) -> CameraIntrinsics {
    let fov_y = fov_y as f64;
    let aspect = width as f64 / height as f64;
    let fov_x = 2.0 * ((fov_y / 2.0).tan() * aspect).atan();

    let fx = width as f64 / (2.0 * (fov_x / 2.0).tan());
    let fy = height as f64 / (2.0 * (fov_y / 2.0).tan());

    let cx = width as f64 / 2.0;
    let cy = height as f64 / 2.0;

    CameraIntrinsics {
        fx,
        fy,
        cx,
        cy,
        width,
        height,
    }
}

pub const IMAGE_WIDTH: u32 = 1440;
pub const IMAGE_HEIGHT: u32 = 1080;
