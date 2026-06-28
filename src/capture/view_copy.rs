use crate::capture::driver::create_capture_image_handle;
use bevy::asset::RenderAssetUsages;
use bevy::core_pipeline::{Core3dSystems, schedule::Core3d};
use bevy::prelude::*;
use bevy::render::RenderApp;
use bevy::render::camera::ExtractedCamera;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{
    Extent3d, Origin3d, TexelCopyTextureInfo, TextureAspect, TextureFormat, TextureUsages,
};
use bevy::render::renderer::{RenderContext, ViewQuery};
use bevy::render::texture::GpuImage;
use bevy::render::view::ViewDepthTexture;

#[derive(Resource, Clone, Deref, DerefMut)]
struct CopyViewTextureTarget(Handle<Image>);

#[derive(Resource, Clone, Copy)]
struct CopyViewTextureCameraOrder(isize);

#[derive(Resource, Default)]
struct CopyViewTextureSystemInstalled(bool);

#[derive(Resource, Clone, Copy)]
enum ViewTextureCopySource {
    Depth,
}

fn copy_view_texture_system(
    view: ViewQuery<(&ExtractedCamera, &ViewDepthTexture)>,
    target_order: Res<CopyViewTextureCameraOrder>,
    target: Res<CopyViewTextureTarget>,
    source: Res<ViewTextureCopySource>,
    image_assets: Res<RenderAssets<GpuImage>>,
    mut render_context: RenderContext,
) {
    let (camera, depth_texture) = view.into_inner();
    if camera.order != target_order.0 {
        return;
    }

    let Some(output_image) = image_assets.get(target.0.id()) else {
        return;
    };

    let aspect = match *source {
        ViewTextureCopySource::Depth => TextureAspect::DepthOnly,
    };
    let encoder = render_context.command_encoder();
    encoder.push_debug_group("copy capture view texture to image");
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
            width: output_image.texture_descriptor.size.width,
            height: output_image.texture_descriptor.size.height,
            depth_or_array_layers: 1,
        },
    );
    encoder.pop_debug_group();
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
            .init_resource::<CopyViewTextureSystemInstalled>();
        render_app
            .world_mut()
            .insert_resource(CopyViewTextureTarget(self.target_texture.clone()));
        render_app
            .world_mut()
            .insert_resource(CopyViewTextureCameraOrder(self.camera_order));
        render_app.world_mut().insert_resource(self.source);

        let installed = render_app
            .world()
            .resource::<CopyViewTextureSystemInstalled>()
            .0;
        if installed {
            return;
        }

        render_app.add_systems(
            Core3d,
            copy_view_texture_system
                .after(Core3dSystems::Prepass)
                .before(Core3dSystems::MainPass),
        );
        render_app
            .world_mut()
            .resource_mut::<CopyViewTextureSystemInstalled>()
            .0 = true;
    }
}
