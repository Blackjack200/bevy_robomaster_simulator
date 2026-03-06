use crate::capture::{CaptureSource, copy_transform};
use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::core_pipeline::{
    core_3d::graph::{Core3d, Node3d},
    prepass::DepthPrepass,
};
use bevy::ecs::{query::QueryItem, system::lifetimeless::Read};
use bevy::prelude::*;
use bevy::render::RenderApp;
use bevy::render::camera::ExtractedCamera;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_graph::{
    NodeRunError, RenderGraphContext, RenderGraphExt, RenderLabel, ViewNode, ViewNodeRunner,
};
use bevy::render::render_resource::{
    CommandEncoderDescriptor, Extent3d, Origin3d, TexelCopyTextureInfo, TextureAspect,
    TextureDimension, TextureFormat, TextureUsages,
};
use bevy::render::texture::GpuImage;
use bevy::render::view::ViewDepthTexture;

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

#[derive(Clone, PartialEq, Eq, Hash, Debug, RenderLabel)]
struct CopyDepthTexturePass;

#[derive(Default)]
struct CopyDepthTextureNode;

#[derive(Resource, Clone, Deref, DerefMut)]
struct CopyDepthTarget(Handle<Image>);

#[derive(Resource, Default)]
struct CopyDepthNodeInstalled(bool);

impl ViewNode for CopyDepthTextureNode {
    type ViewQuery = (Read<ExtractedCamera>, Read<ViewDepthTexture>);

    fn run<'w>(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut bevy::render::renderer::RenderContext<'w>,
        (camera, depth_texture): QueryItem<'w, '_, Self::ViewQuery>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        if camera.order != DEPTH_CAPTURE_CAMERA_ORDER {
            return Ok(());
        }

        let target = world.resource::<CopyDepthTarget>();
        let image_assets = world.resource::<RenderAssets<GpuImage>>();
        let Some(depth_image) = image_assets.get(target.0.id()) else {
            return Ok(());
        };

        render_context.add_command_buffer_generation_task(move |render_device| {
            let mut encoder = render_device.create_command_encoder(&CommandEncoderDescriptor {
                label: Some("copy capture depth to texture"),
            });
            encoder.copy_texture_to_texture(
                TexelCopyTextureInfo {
                    texture: &depth_texture.texture,
                    mip_level: 0,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::DepthOnly,
                },
                TexelCopyTextureInfo {
                    texture: &depth_image.texture,
                    mip_level: 0,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::DepthOnly,
                },
                Extent3d {
                    width: depth_image.size.width,
                    height: depth_image.size.height,
                    depth_or_array_layers: 1,
                },
            );
            encoder.finish()
        });

        Ok(())
    }
}

pub struct DepthTextureCopyPlugin {
    depth_texture: Handle<Image>,
}

impl DepthTextureCopyPlugin {
    pub fn new(app: &mut App, width: u32, height: u32) -> (Self, Handle<Image>) {
        let mut depth_image = Image::new_uninit(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            TextureFormat::Depth32Float,
            RenderAssetUsages::default(),
        );
        depth_image.texture_descriptor.usage =
            TextureUsages::COPY_DST | TextureUsages::COPY_SRC | TextureUsages::TEXTURE_BINDING;

        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        let depth_texture = images.add(depth_image);

        (
            Self {
                depth_texture: depth_texture.clone(),
            },
            depth_texture,
        )
    }
}

impl Plugin for DepthTextureCopyPlugin {
    fn build(&self, app: &mut App) {
        let render_app = app.sub_app_mut(RenderApp);
        render_app
            .world_mut()
            .init_resource::<CopyDepthNodeInstalled>();
        render_app
            .world_mut()
            .insert_resource(CopyDepthTarget(self.depth_texture.clone()));

        let installed = render_app.world().resource::<CopyDepthNodeInstalled>().0;
        if installed {
            return;
        }

        render_app.add_render_graph_node::<ViewNodeRunner<CopyDepthTextureNode>>(
            Core3d,
            CopyDepthTexturePass,
        );
        render_app.add_render_graph_edges(
            Core3d,
            (
                Node3d::EndPrepasses,
                CopyDepthTexturePass,
                Node3d::MainOpaquePass,
            ),
        );
        render_app
            .world_mut()
            .resource_mut::<CopyDepthNodeInstalled>()
            .0 = true;
    }
}
