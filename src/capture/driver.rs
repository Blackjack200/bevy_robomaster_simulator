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
            Buffer, BufferDescriptor, BufferUsages, Extent3d, MapMode, TexelCopyBufferInfo,
            TexelCopyBufferLayout, TextureFormat, TextureUsages,
        },
        renderer::{RenderContext, RenderDevice},
    },
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Resource, Clone)]
pub struct CaptureConfig {
    pub width: u32,
    pub height: u32,
    pub texture_format: TextureFormat,
}

type ToSyncSnapshot = Box<dyn GpuCaptureHandler>;
type DynSnapshotSync = Box<dyn SnapshotSync>;
#[derive(Resource, Deref, DerefMut)]
pub struct GlobalCaptureHandler(pub Arc<Vec<ToSyncSnapshot>>);

#[derive(Resource)]
struct ImageCopier {
    src_image: Handle<Image>,
    extent: Extent3d,
    queue: Mutex<VecDeque<(Buffer, Vec<DynSnapshotSync>)>>,
}

impl ImageCopier {
    pub fn new(src_image: Handle<Image>, extent: Extent3d) -> ImageCopier {
        ImageCopier {
            src_image,
            extent,
            queue: Mutex::new(VecDeque::new()),
        }
    }

    pub fn create_buffer(&self, render_device: &RenderDevice) -> Buffer {
        let padded_bytes_per_row =
            RenderDevice::align_copy_bytes_per_row(self.extent.width as usize) * 4;
        render_device.create_buffer(&BufferDescriptor {
            label: None,
            size: padded_bytes_per_row as u64 * self.extent.height as u64,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
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
    fn captured(&mut self, width: u32, height: u32, image: &[u8]);
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
        let image_copier = world.resource::<ImageCopier>();
        let gpu_images = world.get_resource::<RenderAssets<GpuImage>>().unwrap();
        let hdr = world.get_resource::<GlobalCaptureHandler>();

        let src_image = gpu_images.get(&image_copier.src_image).unwrap();

        let block_dimensions = src_image.texture_format.block_dimensions();
        let block_size = src_image.texture_format.block_copy_size(None).unwrap();
        let padded_bytes_per_row = RenderDevice::align_copy_bytes_per_row(
            (src_image.size.width as usize / block_dimensions.0 as usize) * block_size as usize,
        );

        let snapshot: Vec<DynSnapshotSync> = hdr
            .map(|hdr| hdr.iter().filter_map(|v| v.captured(world)).collect())
            .unwrap_or_default();
        let buffer = image_copier.create_buffer(render_context.render_device());

        render_context.command_encoder().copy_texture_to_buffer(
            src_image.texture.as_image_copy(),
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
        image_copier
            .queue
            .lock()
            .unwrap()
            .push_back((buffer, snapshot));
        Ok(())
    }
}

fn receive_image_from_buffer(mut world: DeferredWorld) {
    let image_copier = world.resource::<ImageCopier>();
    let config = world.resource::<CaptureConfig>().clone();
    let (width, height, texture_format) = (config.width, config.height, config.texture_format);

    let mut guard = image_copier.queue.lock().unwrap();
    let all = guard
        .drain(..)
        .fold(vec![], |mut all, (buffer, snapshots)| {
            let (s, r) = futures::channel::oneshot::channel();
            let buffer_slice = buffer.slice(..);
            let buffer = buffer.clone();
            buffer_slice.map_async(MapMode::Read, move |res| {
                res.expect("Failed to map buffer");
                let buffer_slice = buffer.slice(..);
                let data = buffer_slice.get_mapped_range();
                let dat = data.to_vec();
                drop(data);
                buffer.unmap();
                s.send(dat).expect("Failed to send map update")
            });
            all.push((r, snapshots));
            all
        });
    drop(guard);

    // what fuck
    for (r, snapshots) in all.into_iter() {
        let snapshots: Vec<Box<dyn SnapshotAsync>> = snapshots
            .into_iter()
            .map(|v| v.captured(&mut world, &config))
            .collect();
        AsyncComputeTaskPool::get()
            .spawn(async move {
                let mut image_data = r.await.expect("Failed to receive the map_async message");
                let image_data = {
                    let row_bytes = width as usize * texture_format.pixel_size().unwrap();
                    let aligned_row_bytes = RenderDevice::align_copy_bytes_per_row(row_bytes);
                    if row_bytes != aligned_row_bytes {
                        image_data = image_data
                            .chunks(aligned_row_bytes)
                            .take(height as usize)
                            .flat_map(|row| &row[..row_bytes.min(row.len())])
                            .cloned()
                            .collect();
                    }
                    let mut bevy_image = Image::new_target_texture(width, height, texture_format);
                    bevy_image.data = Some(image_data);
                    bevy_image.try_into_dynamic().unwrap().to_rgb8().into_raw()
                };
                let image_data = image_data.as_slice();
                for mut snapshot in snapshots {
                    snapshot.captured(width, height, image_data);
                }
            })
            .detach();
    }
}

pub struct CameraCapturePlugin {
    config: CaptureConfig,
    extent: Extent3d,
    texture_format: TextureFormat,
    snapshots: Arc<Vec<ToSyncSnapshot>>,
    handle: Handle<Image>,
}

#[derive(Resource, Deref, DerefMut)]
struct RateLimiter(Mutex<Timer>);

impl CameraCapturePlugin {
    pub fn new(
        app: &mut App,
        config: CaptureConfig,
        snapshots: Vec<ToSyncSnapshot>,
    ) -> (Self, Handle<Image>) {
        let extent = Extent3d {
            width: config.width,
            height: config.height,
            ..Default::default()
        };
        let mut render_target_image =
            Image::new_target_texture(extent.width, extent.height, config.texture_format);
        render_target_image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        let handle = images.add(render_target_image);

        (
            Self {
                config: config.clone(),
                extent,
                snapshots: Arc::new(snapshots),
                handle: handle.clone(),
                texture_format: config.texture_format,
            },
            handle,
        )
    }
}

impl Plugin for CameraCapturePlugin {
    fn build(&self, app: &mut App) {
        let config = self.config.clone();

        app.insert_resource(config);

        let render_app = app.sub_app_mut(RenderApp);

        let mut graph = render_app.world_mut().resource_mut::<RenderGraph>();
        graph.add_node(ImageCopy, ImageCopyDriver);
        graph.add_node_edge(bevy::render::graph::CameraDriverLabel, ImageCopy);

        render_app
            .insert_resource(GlobalCaptureHandler(self.snapshots.clone()))
            .insert_resource(self.config.clone())
            .insert_resource(ImageCopier::new(self.handle.clone(), self.extent))
            .add_systems(
                Render,
                receive_image_from_buffer.after(RenderSystems::Render),
            );
    }
}
