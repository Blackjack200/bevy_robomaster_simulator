pub mod depth;
pub mod driver;

use bevy::anti_alias::fxaa::Fxaa;
use bevy::camera::RenderTarget;
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::post_process::bloom::{Bloom, BloomCompositeMode, BloomPrefilter};
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

pub const CAPTURE_CAMERA_ORDER: isize = -100;

pub fn setup_capture_camera(world: &mut World) {
    let capture_camera_exists = {
        let mut query = world.query_filtered::<Entity, With<CaptureCamera>>();
        query.iter(world).next().is_some()
    };
    if capture_camera_exists {
        return;
    }

    let render_target_handle = world.resource::<ImageHandle>().0.clone();
    let fov = world.resource::<CameraFov>().0;

    world.spawn((
        Camera3d::default(),
        Bloom::NATURAL,
        Tonemapping::None,
        RenderTarget::Image(render_target_handle.into()),
        Camera {
            order: CAPTURE_CAMERA_ORDER,
            clear_color: ClearColorConfig::Custom(Color::BLACK),

            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov,
            near: 0.1,
            far: 500000000.0,
            ..default()
        }),
        Msaa::Off,
        Fxaa::default(),
        Hdr,
        DepthPrepass,
        CaptureCamera,
    ));
}

#[derive(Component)]
pub struct PreviewCamera;

#[derive(Component)]
pub struct PreviewImageNode;

pub fn setup_preview_window(world: &mut World) {
    let preview_enabled = world
        .resource::<crate::config::SimulationConfig>()
        .preview
        .enabled;
    if !preview_enabled {
        return;
    }

    let render_target_handle = world.resource::<ImageHandle>().0.clone();
    let preview_camera_exists = {
        let mut query = world.query_filtered::<Entity, With<PreviewCamera>>();
        query.iter(world).next().is_some()
    };
    if !preview_camera_exists {
        world.spawn((
            Camera2d::default(),
            Camera {
                order: 1,
                ..default()
            },
            PreviewCamera,
        ));
    }

    let preview_node_exists = {
        let mut query = world.query_filtered::<Entity, With<PreviewImageNode>>();
        query.iter(world).next().is_some()
    };
    if !preview_node_exists {
        world.spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            // Render as a background; help text UI remains on top.
            GlobalZIndex(-1),
            ImageNode::new(render_target_handle),
            PreviewImageNode,
        ));
    }
}

pub fn copy_transform(target: &Transform, our: &mut Transform) {
    our.translation = target.translation;
    our.scale = target.scale;
    our.rotation = target.rotation;
}

pub fn sync_capture_camera(
    target: Single<&Transform, (With<CaptureSource>, Without<CaptureCamera>)>,
    mut our: Single<&mut Transform, (With<CaptureCamera>, Without<CaptureSource>)>,
) {
    copy_transform(&target, &mut our);
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
