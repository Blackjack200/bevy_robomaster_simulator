use bevy::asset::RenderAssetUsages;
use bevy::ecs::world::DeferredWorld;
use bevy::render::texture::GpuImage;
use bevy::tasks::AsyncComputeTaskPool;
use bevy::{
    image::TextureFormatPixelInfo,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        render_asset::RenderAssets,
        render_graph::{self, NodeRunError, RenderGraph, RenderGraphContext, RenderLabel},
        render_resource::{
            Buffer, BufferDescriptor, BufferUsages, Extent3d, MapMode, Origin3d,
            TexelCopyBufferInfo, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect,
            TextureFormat, TextureUsages,
        },
        renderer::{RenderContext, RenderDevice},
    },
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

const MAX_IN_FLIGHT_FRAMES: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapturedFrameKind {
    Rgb8,
    Depth32F,
}

#[derive(Resource, Clone)]
pub struct CaptureConfig {
    pub width: u32,
    pub height: u32,
    pub texture_format: TextureFormat,
    pub frame_kind: CapturedFrameKind,
}

pub struct CapturedFrame<'a> {
    pub kind: CapturedFrameKind,
    pub width: u32,
    pub height: u32,
    pub data: &'a [u8],
}

pub fn create_capture_image_handle(
    app: &mut App,
    width: u32,
    height: u32,
    texture_format: TextureFormat,
    asset_usages: RenderAssetUsages,
    texture_usages: TextureUsages,
) -> Handle<Image> {
    let extent = Extent3d {
        width,
        height,
        ..Default::default()
    };

    let mut image = if matches!(
        texture_format,
        TextureFormat::Depth16Unorm
            | TextureFormat::Depth24Plus
            | TextureFormat::Depth24PlusStencil8
            | TextureFormat::Depth32Float
            | TextureFormat::Depth32FloatStencil8
    ) {
        Image::new_uninit(
            extent,
            bevy::render::render_resource::TextureDimension::D2,
            texture_format,
            asset_usages,
        )
    } else {
        Image::new_target_texture(width, height, texture_format, Some(texture_format))
    };

    image.texture_descriptor.usage |= texture_usages;
    let mut images = app.world_mut().resource_mut::<Assets<Image>>();
    images.add(image)
}

enum CapturePluginNode {
    Camera(CameraCapturePlugin),
    ViewCopy(crate::capture::view_copy::ViewTextureCopyPlugin),
}

pub struct CaptureBundle {
    plugins: Vec<CapturePluginNode>,
    color_target: Option<Handle<Image>>,
    depth_target: Option<Handle<Image>>,
}

impl CaptureBundle {
    pub fn color(
        app: &mut App,
        config: CaptureConfig,
        snapshots: Vec<Box<dyn GpuCaptureHandler>>,
    ) -> Self {
        let (plugin, color_target) = CameraCapturePlugin::new(app, config, snapshots);
        Self {
            plugins: vec![CapturePluginNode::Camera(plugin)],
            color_target: Some(color_target),
            depth_target: None,
        }
    }

    pub fn color_and_depth(
        app: &mut App,
        color_config: CaptureConfig,
        color_snapshots: Vec<Box<dyn GpuCaptureHandler>>,
        depth_snapshots: Vec<Box<dyn GpuCaptureHandler>>,
    ) -> Self {
        Self::color(app, color_config.clone(), color_snapshots).with_depth_from_camera_order(
            app,
            CaptureConfig {
                width: color_config.width,
                height: color_config.height,
                texture_format: TextureFormat::Depth32Float,
                frame_kind: CapturedFrameKind::Depth32F,
            },
            crate::capture::CAPTURE_CAMERA_ORDER,
            depth_snapshots,
        )
    }

    pub fn depth_from_camera_order(
        app: &mut App,
        config: CaptureConfig,
        camera_order: isize,
        snapshots: Vec<Box<dyn GpuCaptureHandler>>,
    ) -> Self {
        let mut bundle = Self {
            plugins: Vec::new(),
            color_target: None,
            depth_target: None,
        };
        bundle.push_depth_from_camera_order(app, config, camera_order, snapshots);
        bundle
    }

    pub fn with_depth_from_camera_order(
        mut self,
        app: &mut App,
        config: CaptureConfig,
        camera_order: isize,
        snapshots: Vec<Box<dyn GpuCaptureHandler>>,
    ) -> Self {
        self.push_depth_from_camera_order(app, config, camera_order, snapshots);
        self
    }

    pub fn color_target(&self) -> Option<&Handle<Image>> {
        self.color_target.as_ref()
    }

    pub fn depth_target(&self) -> Option<&Handle<Image>> {
        self.depth_target.as_ref()
    }

    fn push_depth_from_camera_order(
        &mut self,
        app: &mut App,
        config: CaptureConfig,
        camera_order: isize,
        snapshots: Vec<Box<dyn GpuCaptureHandler>>,
    ) {
        let (view_copy, depth_target) =
            crate::capture::view_copy::ViewTextureCopyPlugin::new_depth_for_camera_order(
                app,
                config.width,
                config.height,
                camera_order,
            );
        let depth_capture =
            CameraCapturePlugin::from_existing_handle(config, depth_target.clone(), snapshots);

        self.plugins.push(CapturePluginNode::ViewCopy(view_copy));
        self.plugins.push(CapturePluginNode::Camera(depth_capture));
        self.depth_target = Some(depth_target);
    }
}

impl Plugin for CaptureBundle {
    fn is_unique(&self) -> bool {
        false
    }

    fn build(&self, app: &mut App) {
        for plugin in &self.plugins {
            match plugin {
                CapturePluginNode::Camera(plugin) => plugin.build(app),
                CapturePluginNode::ViewCopy(plugin) => plugin.build(app),
            }
        }
    }
}

type ToSyncSnapshot = Box<dyn GpuCaptureHandler>;
type DynSnapshotSync = Box<dyn SnapshotSync>;

#[derive(Resource, Default, Deref, DerefMut)]
struct ImageCopiers(Vec<ImageCopier>);

#[derive(Resource, Default)]
struct ImageCopyDriverInstalled(bool);

struct ImageCopier {
    config: CaptureConfig,
    src_image: Handle<Image>,
    queue: Mutex<VecDeque<(Buffer, Vec<DynSnapshotSync>, u32, u32, TextureFormat)>>,
    free_buffers: Arc<Mutex<Vec<Buffer>>>,
    snapshots: Arc<Vec<ToSyncSnapshot>>,
}

impl ImageCopier {
    pub fn new(
        config: CaptureConfig,
        src_image: Handle<Image>,
        snapshots: Arc<Vec<ToSyncSnapshot>>,
    ) -> ImageCopier {
        ImageCopier {
            config,
            src_image,
            queue: Mutex::new(VecDeque::new()),
            free_buffers: Arc::new(Mutex::new(Vec::new())),
            snapshots,
        }
    }

    fn acquire_buffer(&self, render_device: &RenderDevice, size: u64) -> Buffer {
        if let Some(buf) = self.free_buffers.lock().unwrap().pop() {
            return buf;
        }
        render_device.create_buffer(&BufferDescriptor {
            label: None,
            size,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }
}

fn unpad_rows(padded: &[u8], row_bytes: usize, aligned_row_bytes: usize, height: u32) -> Vec<u8> {
    if row_bytes == aligned_row_bytes {
        return padded.to_vec();
    }
    let mut out = Vec::with_capacity(row_bytes * height as usize);
    for row in padded.chunks(aligned_row_bytes).take(height as usize) {
        out.extend_from_slice(&row[..row_bytes.min(row.len())]);
    }
    out
}

fn padded_rgba_to_rgb(
    padded: &[u8],
    width: u32,
    height: u32,
    format: TextureFormat,
) -> Option<Vec<u8>> {
    let pixel_size = format.pixel_size().ok()?;
    if pixel_size != 4 {
        return None;
    }
    let row_bytes = width as usize * pixel_size;
    let aligned_row_bytes = RenderDevice::align_copy_bytes_per_row(row_bytes);

    match format {
        TextureFormat::Bgra8UnormSrgb | TextureFormat::Bgra8Unorm => {
            let mut out = Vec::with_capacity(width as usize * height as usize * 3);
            for row in padded.chunks(aligned_row_bytes).take(height as usize) {
                let row = &row[..row_bytes.min(row.len())];
                for px in row.chunks_exact(4) {
                    out.extend_from_slice(&[px[2], px[1], px[0]]);
                }
            }
            Some(out)
        }
        TextureFormat::Rgba8UnormSrgb | TextureFormat::Rgba8Unorm => {
            let mut out = Vec::with_capacity(width as usize * height as usize * 3);
            for row in padded.chunks(aligned_row_bytes).take(height as usize) {
                let row = &row[..row_bytes.min(row.len())];
                for px in row.chunks_exact(4) {
                    out.extend_from_slice(&[px[0], px[1], px[2]]);
                }
            }
            Some(out)
        }
        _ => None,
    }
}

fn capture_texture_aspect(format: TextureFormat) -> TextureAspect {
    if matches!(
        format,
        TextureFormat::Depth16Unorm
            | TextureFormat::Depth24Plus
            | TextureFormat::Depth24PlusStencil8
            | TextureFormat::Depth32Float
            | TextureFormat::Depth32FloatStencil8
    ) {
        TextureAspect::DepthOnly
    } else {
        TextureAspect::All
    }
}

pub trait SnapshotSync: Send {
    fn captured(
        self: Box<Self>,
        world: &mut DeferredWorld,
        config: &CaptureConfig,
    ) -> Box<dyn SnapshotAsync>;
}

pub trait SnapshotAsync: Send {
    fn captured(&mut self, frame: CapturedFrame<'_>);
}

pub trait GpuCaptureHandler: Send + Sync + 'static {
    fn captured(&self, world: &World) -> Option<Box<dyn SnapshotSync>>;
}

#[derive(Debug, PartialEq, Eq, Clone, Hash, RenderLabel)]
struct ImageCopy;

#[derive(Default)]
struct ImageCopyDriver;

impl render_graph::Node for ImageCopyDriver {
    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let Some(copiers) = world.get_resource::<ImageCopiers>() else {
            return Ok(());
        };
        let Some(gpu_images) = world.get_resource::<RenderAssets<GpuImage>>() else {
            return Ok(());
        };

        for copier in copiers.iter() {
            let Some(src_image) = gpu_images.get(&copier.src_image) else {
                continue;
            };

            let block_dimensions = src_image.texture_format.block_dimensions();
            let block_size = src_image.texture_format.block_copy_size(None).unwrap();
            let padded_bytes_per_row = RenderDevice::align_copy_bytes_per_row(
                (src_image.size.width as usize / block_dimensions.0 as usize) * block_size as usize,
            );
            let buffer_size = padded_bytes_per_row as u64 * src_image.size.height as u64;
            let buffer = copier.acquire_buffer(render_context.render_device(), buffer_size);

            let snapshot: Vec<DynSnapshotSync> = copier
                .snapshots
                .iter()
                .filter_map(|handler| handler.captured(world))
                .collect();

            render_context.command_encoder().copy_texture_to_buffer(
                TexelCopyTextureInfo {
                    texture: &src_image.texture,
                    mip_level: 0,
                    origin: Origin3d::ZERO,
                    aspect: capture_texture_aspect(src_image.texture_format),
                },
                TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(
                            std::num::NonZero::<u32>::new(padded_bytes_per_row as u32)
                                .unwrap()
                                .into(),
                        ),
                        rows_per_image: None,
                    },
                },
                src_image.size,
            );

            let mut queue = copier.queue.lock().unwrap();
            queue.push_back((
                buffer,
                snapshot,
                src_image.size.width,
                src_image.size.height,
                src_image.texture_format,
            ));
            while queue.len() > MAX_IN_FLIGHT_FRAMES {
                if let Some((buffer, _, _, _, _)) = queue.pop_front() {
                    copier.free_buffers.lock().unwrap().push(buffer);
                }
            }
        }
        Ok(())
    }
}

fn receive_image_from_buffer(mut world: DeferredWorld) {
    let copier_count = world.resource::<ImageCopiers>().len();
    if copier_count == 0 {
        return;
    }

    for idx in 0..copier_count {
        let next = {
            let copiers = world.resource::<ImageCopiers>();
            let Some(copier) = copiers.get(idx) else {
                continue;
            };
            let mut guard = copier.queue.lock().unwrap();
            guard
                .pop_front()
                .map(|(buffer, snapshots, width, height, texture_format)| {
                    (
                        buffer,
                        snapshots,
                        width,
                        height,
                        texture_format,
                        copier.free_buffers.clone(),
                        copier.config.clone(),
                    )
                })
        };

        let Some((buffer, snapshots, width, height, texture_format, free_buffers, config)) = next
        else {
            continue;
        };

        let (s, r) = futures::channel::oneshot::channel();
        let buffer_slice = buffer.slice(..);
        let buffer_for_map = buffer.clone();
        buffer_slice.map_async(MapMode::Read, move |res| {
            res.expect("Failed to map buffer");
            let buffer_slice = buffer_for_map.slice(..);
            let data = buffer_slice.get_mapped_range();
            let dat = data.to_vec();
            drop(data);
            buffer_for_map.unmap();
            free_buffers.lock().unwrap().push(buffer_for_map);
            s.send(dat).expect("Failed to send map update");
        });

        let snapshots: Vec<Box<dyn SnapshotAsync>> = snapshots
            .into_iter()
            .map(|v| v.captured(&mut world, &config))
            .collect();
        let frame_kind = config.frame_kind;

        AsyncComputeTaskPool::get()
            .spawn(async move {
                let padded = r.await.expect("Failed to receive the map_async message");
                let frame_bytes = match frame_kind {
                    CapturedFrameKind::Rgb8 => {
                        padded_rgba_to_rgb(&padded, width, height, texture_format).unwrap_or_else(
                            || {
                                let pixel_size = texture_format
                                    .pixel_size()
                                    .expect("Unsupported capture texture format");
                                let row_bytes = width as usize * pixel_size;
                                let aligned_row_bytes =
                                    RenderDevice::align_copy_bytes_per_row(row_bytes);
                                let unpadded =
                                    unpad_rows(&padded, row_bytes, aligned_row_bytes, height);
                                let mut bevy_image = Image::new_target_texture(
                                    width,
                                    height,
                                    texture_format,
                                    Some(texture_format),
                                );
                                bevy_image.data = Some(unpadded);
                                bevy_image.try_into_dynamic().unwrap().to_rgb8().into_raw()
                            },
                        )
                    }
                    CapturedFrameKind::Depth32F => {
                        let pixel_size = texture_format
                            .pixel_size()
                            .expect("Unsupported depth capture texture format");
                        let row_bytes = width as usize * pixel_size;
                        let aligned_row_bytes = RenderDevice::align_copy_bytes_per_row(row_bytes);
                        unpad_rows(&padded, row_bytes, aligned_row_bytes, height)
                    }
                };

                for mut snapshot in snapshots {
                    snapshot.captured(CapturedFrame {
                        kind: frame_kind,
                        width,
                        height,
                        data: frame_bytes.as_slice(),
                    });
                }
            })
            .detach();
    }
}

pub struct CameraCapturePlugin {
    config: CaptureConfig,
    snapshots: Arc<Vec<ToSyncSnapshot>>,
    handle: Handle<Image>,
    expose_config_resource: bool,
}

impl CameraCapturePlugin {
    pub fn new(
        app: &mut App,
        config: CaptureConfig,
        snapshots: Vec<ToSyncSnapshot>,
    ) -> (Self, Handle<Image>) {
        let handle = create_capture_image_handle(
            app,
            config.width,
            config.height,
            config.texture_format,
            RenderAssetUsages::default(),
            TextureUsages::COPY_SRC,
        );

        (
            Self {
                config,
                snapshots: Arc::new(snapshots),
                handle: handle.clone(),
                expose_config_resource: true,
            },
            handle,
        )
    }

    pub fn from_existing_handle(
        config: CaptureConfig,
        handle: Handle<Image>,
        snapshots: Vec<ToSyncSnapshot>,
    ) -> Self {
        Self {
            config,
            snapshots: Arc::new(snapshots),
            handle,
            expose_config_resource: false,
        }
    }
}

impl Plugin for CameraCapturePlugin {
    fn is_unique(&self) -> bool {
        false
    }

    fn build(&self, app: &mut App) {
        if self.expose_config_resource {
            app.insert_resource(self.config.clone());
        }

        let render_app = app.sub_app_mut(RenderApp);
        render_app.world_mut().init_resource::<ImageCopiers>();
        render_app
            .world_mut()
            .init_resource::<ImageCopyDriverInstalled>();

        {
            let mut copiers = render_app.world_mut().resource_mut::<ImageCopiers>();
            copiers.push(ImageCopier::new(
                self.config.clone(),
                self.handle.clone(),
                self.snapshots.clone(),
            ));
        }

        let installed = render_app.world().resource::<ImageCopyDriverInstalled>().0;
        if !installed {
            let mut graph = render_app.world_mut().resource_mut::<RenderGraph>();
            graph.add_node(ImageCopy, ImageCopyDriver);
            graph.add_node_edge(bevy::render::graph::CameraDriverLabel, ImageCopy);
            drop(graph);

            render_app
                .world_mut()
                .resource_mut::<ImageCopyDriverInstalled>()
                .0 = true;
            render_app.add_systems(
                Render,
                receive_image_from_buffer.after(RenderSystems::Render),
            );
        }

        if self.expose_config_resource {
            render_app.insert_resource(self.config.clone());
        }
    }
}
