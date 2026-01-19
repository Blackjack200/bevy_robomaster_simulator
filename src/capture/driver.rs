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

const MAX_IN_FLIGHT_FRAMES: usize = 2;

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
    free_buffers: Arc<Mutex<Vec<Buffer>>>,
}

impl ImageCopier {
    pub fn new(src_image: Handle<Image>, extent: Extent3d) -> ImageCopier {
        ImageCopier {
            src_image,
            extent,
            queue: Mutex::new(VecDeque::new()),
            free_buffers: Arc::new(Mutex::new(Vec::new())),
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
        let buffer_size = padded_bytes_per_row as u64 * src_image.size.height as u64;
        let buffer = image_copier.acquire_buffer(render_context.render_device(), buffer_size);

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

        // Keep only a small number of in-flight readbacks; drop old frames to avoid stalls/jitter.
        let mut queue = image_copier.queue.lock().unwrap();
        while queue.len() > MAX_IN_FLIGHT_FRAMES {
            if let Some((buffer, _)) = queue.pop_front() {
                image_copier.free_buffers.lock().unwrap().push(buffer);
            }
        }
        Ok(())
    }
}

fn receive_image_from_buffer(mut world: DeferredWorld) {
    let image_copier = world.resource::<ImageCopier>();
    let config = world.resource::<CaptureConfig>().clone();
    let (width, height, texture_format) = (config.width, config.height, config.texture_format);

    let (buffer, snapshots) = {
        let mut guard = image_copier.queue.lock().unwrap();
        let Some(next) = guard.pop_front() else {
            return;
        };
        next
    };

    let free_buffers = image_copier.free_buffers.clone();
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
        s.send(dat).expect("Failed to send map update")
    });

    let snapshots: Vec<Box<dyn SnapshotAsync>> = snapshots
        .into_iter()
        .map(|v| v.captured(&mut world, &config))
        .collect();

    AsyncComputeTaskPool::get()
        .spawn(async move {
            let padded = r.await.expect("Failed to receive the map_async message");
            let image_data = padded_rgba_to_rgb(&padded, width, height, texture_format)
                .unwrap_or_else(|| {
                    let pixel_size = texture_format
                        .pixel_size()
                        .expect("Unsupported capture texture format");
                    let row_bytes = width as usize * pixel_size;
                    let aligned_row_bytes = RenderDevice::align_copy_bytes_per_row(row_bytes);
                    let unpadded = unpad_rows(&padded, row_bytes, aligned_row_bytes, height);
                    let mut bevy_image = Image::new_target_texture(
                        width,
                        height,
                        texture_format,
                        Some(texture_format),
                    );
                    bevy_image.data = Some(unpadded);
                    bevy_image.try_into_dynamic().unwrap().to_rgb8().into_raw()
                });
            let image_data = image_data.as_slice();
            for mut snapshot in snapshots {
                snapshot.captured(width, height, image_data);
            }
        })
        .detach();
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
        let mut render_target_image = Image::new_target_texture(
            extent.width,
            extent.height,
            config.texture_format,
            Some(config.texture_format),
        );
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
