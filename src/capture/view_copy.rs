use crate::capture::driver::create_capture_image_handle;
use bevy::asset::RenderAssetUsages;
use bevy::core_pipeline::core_3d::graph::{Core3d, Node3d};
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
    TextureFormat, TextureUsages,
};
use bevy::render::texture::GpuImage;
use bevy::render::view::ViewDepthTexture;

#[derive(Clone, PartialEq, Eq, Hash, Debug, RenderLabel)]
struct CopyViewTexturePass;

#[derive(Default)]
struct CopyViewTextureNode;

#[derive(Resource, Clone, Deref, DerefMut)]
struct CopyViewTextureTarget(Handle<Image>);

#[derive(Resource, Clone, Copy)]
struct CopyViewTextureCameraOrder(isize);

#[derive(Resource, Default)]
struct CopyViewTextureNodeInstalled(bool);

#[derive(Resource, Clone, Copy)]
enum ViewTextureCopySource {
    Depth,
}

impl ViewNode for CopyViewTextureNode {
    type ViewQuery = (Read<ExtractedCamera>, Read<ViewDepthTexture>);

    fn run<'w>(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut bevy::render::renderer::RenderContext<'w>,
        (camera, depth_texture): QueryItem<'w, '_, Self::ViewQuery>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let target_order = world.resource::<CopyViewTextureCameraOrder>().0;
        if camera.order != target_order {
            return Ok(());
        }

        let target = world.resource::<CopyViewTextureTarget>();
        let source = *world.resource::<ViewTextureCopySource>();
        let image_assets = world.resource::<RenderAssets<GpuImage>>();
        let Some(output_image) = image_assets.get(target.0.id()) else {
            return Ok(());
        };

        render_context.add_command_buffer_generation_task(move |render_device| {
            let mut encoder = render_device.create_command_encoder(&CommandEncoderDescriptor {
                label: Some("copy capture view texture to image"),
            });
            let aspect = match source {
                ViewTextureCopySource::Depth => TextureAspect::DepthOnly,
            };
            encoder.copy_texture_to_texture(
                TexelCopyTextureInfo {
                    texture: &depth_texture.texture,
                    mip_level: 0,
                    origin: Origin3d::ZERO,
                    aspect,
                },
                TexelCopyTextureInfo {
                    texture: &output_image.texture,
                    mip_level: 0,
                    origin: Origin3d::ZERO,
                    aspect,
                },
                Extent3d {
                    width: output_image.size.width,
                    height: output_image.size.height,
                    depth_or_array_layers: 1,
                },
            );
            encoder.finish()
        });

        Ok(())
    }
}

pub struct ViewTextureCopyPlugin {
    target_texture: Handle<Image>,
    camera_order: isize,
    source: ViewTextureCopySource,
}

impl ViewTextureCopyPlugin {
    pub fn new_depth(app: &mut App, width: u32, height: u32) -> (Self, Handle<Image>) {
        Self::new_depth_for_camera_order(
            app,
            width,
            height,
            crate::capture::depth::DEPTH_CAPTURE_CAMERA_ORDER,
        )
    }

    pub fn new_depth_for_camera_order(
        app: &mut App,
        width: u32,
        height: u32,
        camera_order: isize,
    ) -> (Self, Handle<Image>) {
        let depth_texture = create_capture_image_handle(
            app,
            width,
            height,
            TextureFormat::Depth32Float,
            RenderAssetUsages::default(),
            TextureUsages::COPY_DST | TextureUsages::COPY_SRC | TextureUsages::TEXTURE_BINDING,
        );

        (
            Self {
                target_texture: depth_texture.clone(),
                camera_order,
                source: ViewTextureCopySource::Depth,
            },
            depth_texture,
        )
    }
}

impl Plugin for ViewTextureCopyPlugin {
    fn build(&self, app: &mut App) {
        let render_app = app.sub_app_mut(RenderApp);
        render_app
            .world_mut()
            .init_resource::<CopyViewTextureNodeInstalled>();
        render_app
            .world_mut()
            .insert_resource(CopyViewTextureTarget(self.target_texture.clone()));
        render_app
            .world_mut()
            .insert_resource(CopyViewTextureCameraOrder(self.camera_order));
        render_app.world_mut().insert_resource(self.source);

        let installed = render_app
            .world()
            .resource::<CopyViewTextureNodeInstalled>()
            .0;
        if installed {
            return;
        }

        render_app.add_render_graph_node::<ViewNodeRunner<CopyViewTextureNode>>(
            Core3d,
            CopyViewTexturePass,
        );
        render_app.add_render_graph_edges(
            Core3d,
            (
                Node3d::EndPrepasses,
                CopyViewTexturePass,
                Node3d::MainOpaquePass,
            ),
        );
        render_app
            .world_mut()
            .resource_mut::<CopyViewTextureNodeInstalled>()
            .0 = true;
    }
}
