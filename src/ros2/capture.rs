use crate::dataset::prelude::{ArmorOnScreen, DatasetHandle};
use crate::dataset::writer::ArmorEntry;
use crate::ros2::plugin::MainCamera;
use crate::ros2::topic::{CameraInfoTopic, ImageCompressedTopic, ImageRawTopic, TopicPublisher};
use bevy::anti_alias::fxaa::Fxaa;
use bevy::camera::Exposure;
use bevy::post_process::bloom::Bloom;
use bevy::render::texture::GpuImage;
use bevy::render::view::Hdr;
use bevy::tasks::AsyncComputeTaskPool;
use bevy::{
    image::TextureFormatPixelInfo,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        render_asset::RenderAssets,
        render_graph::{self, NodeRunError, RenderGraph, RenderGraphContext, RenderLabel},
        render_resource::{
            Buffer, BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, MapMode,
            PollType, TexelCopyBufferInfo, TexelCopyBufferLayout, TextureFormat, TextureUsages,
        },
        renderer::{RenderContext, RenderDevice, RenderQueue},
    },
};
use r2r::Clock;
use r2r::sensor_msgs::msg::{CameraInfo, RegionOfInterest};
use r2r::std_msgs::msg::Header;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

#[derive(Resource, Clone)]
pub struct CaptureConfig {
    pub width: u32,
    pub height: u32,
    pub texture_format: TextureFormat,
    pub fov_y: f32,
}

#[derive(Resource)]
struct ImageCopier {
    copying: Arc<AtomicBool>,
    src_image: Handle<Image>,
    extent: Extent3d,
    buffer: Option<Arc<Buffer>>,
}

impl ImageCopier {
    pub fn new(src_image: Handle<Image>, extent: Extent3d) -> ImageCopier {
        ImageCopier {
            src_image,
            copying: Arc::new(AtomicBool::new(false)),
            extent,
            buffer: None,
        }
    }

    pub fn init_buffer(&mut self, render_device: &RenderDevice) {
        if self.buffer.is_some() {
            return;
        }
        let padded_bytes_per_row =
            RenderDevice::align_copy_bytes_per_row(self.extent.width as usize) * 4;
        let cpu_buffer = render_device.create_buffer(&BufferDescriptor {
            label: None,
            size: padded_bytes_per_row as u64 * self.extent.height as u64,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.buffer = Some(Arc::new(cpu_buffer));
    }
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

        if image_copier.copying.load(Ordering::Acquire) {
            return Ok(());
        }
        let src_image = gpu_images.get(&image_copier.src_image).unwrap();
        let mut encoder = render_context
            .render_device()
            .create_command_encoder(&CommandEncoderDescriptor::default());
        let block_dimensions = src_image.texture_format.block_dimensions();
        let block_size = src_image.texture_format.block_copy_size(None).unwrap();
        let padded_bytes_per_row = RenderDevice::align_copy_bytes_per_row(
            (src_image.size.width as usize / block_dimensions.0 as usize) * block_size as usize,
        );

        let Some(buffer) = &image_copier.buffer else {
            return Ok(());
        };
        encoder.copy_texture_to_buffer(
            src_image.texture.as_image_copy(),
            TexelCopyBufferInfo {
                buffer,
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
        let render_queue = world.get_resource::<RenderQueue>().unwrap();
        render_queue.submit(std::iter::once(encoder.finish()));

        Ok(())
    }
}

fn setup_copier(render_device: Res<RenderDevice>, mut copier: ResMut<ImageCopier>) {
    copier.init_buffer(&render_device);
}

fn receive_image_from_buffer(
    image_copier: Res<ImageCopier>,
    render_device: Res<RenderDevice>,
    config: Res<CaptureConfig>,
    ctx: Res<RosCaptureContext>,
    dw: Res<DatasetHandle>,
    mut dd: ResMut<ArmorOnScreen>,
) {
    let ctx = Arc::new(ctx.clone());
    let config = Arc::new(config.clone());
    let (width, height, texture_format) = (config.width, config.height, config.texture_format);

    let buffer = image_copier.buffer.clone();
    if buffer.is_none() {
        image_copier.copying.store(false, Ordering::Release);
        return;
    }
    let buffer = buffer.unwrap();
    let render_device = render_device.clone();
    let copying = image_copier.copying.clone();
    let ctx = ctx.clone();
    let config = config.clone();

    let len = dd.0.len();
    let armor = dd.0.drain().fold(Vec::with_capacity(len), |mut v, (_, n)| {
        for (typ, label, color, pos) in n {
            v.push(ArmorEntry {
                color,
                typ,
                label,
                points: pos.map(|v| Vec2::new(v.0 as f32, v.1 as f32).normalize_or_zero()),
            });
        }
        v
    });
    let dw = dw.clone();
    AsyncComputeTaskPool::get()
        .spawn(async move {
            let buffer_slice = buffer.slice(..);
            let (s, r) = crossbeam_channel::bounded(1);

            buffer_slice.map_async(MapMode::Read, move |r| match r {
                Ok(r) => s.send(r).expect("Failed to send map update"),
                Err(err) => panic!("Failed to map buffer {err}"),
            });

            render_device
                .poll(PollType::Wait)
                .expect("Failed to poll device for map async");
            r.recv().expect("Failed to receive the map_async message");

            let mut image_data = buffer_slice.get_mapped_range().to_vec();
            buffer.unmap();
            copying.store(false, Ordering::Release);

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

            let optical_frame_hdr = Header {
                stamp: Clock::to_builtin_time(&ctx.clock.lock().unwrap().get_now().unwrap()),
                frame_id: "camera_optical_frame".to_string(),
            };
            /*ctx.image_compressed.publish(compress_image(
                optical_frame_hdr.clone(),
                width,
                height,
                image_data.clone(),
            ));*/
            if !armor.is_empty() {
                info!("wrote 1 dataset entry: {}", armor.len());
                dw.lock()
                    .unwrap()
                    .write_entry(config.height, config.width, image_data, &armor)
                    .unwrap();
            }
            let (camera_info, image) = compute_camera(
                config.fov_y,
                optical_frame_hdr,
                config.width,
                config.height,
                image_data,
            );
            ctx.camera_info.publish(camera_info);
            ctx.image_raw.publish(image);
        })
        .detach();
}

#[derive(Resource, Clone)]
pub struct RosCaptureContext {
    pub clock: Arc<Mutex<Clock>>,
    pub camera_info: Arc<TopicPublisher<CameraInfoTopic>>,
    pub image_raw: Arc<TopicPublisher<ImageRawTopic>>,
    pub image_compressed: Arc<TopicPublisher<ImageCompressedTopic>>,
}

pub struct RosCapturePlugin {
    pub config: CaptureConfig,
    pub context: RosCaptureContext,
}

#[derive(Resource, Deref)]
struct ImageHandle(Handle<Image>);

#[derive(Resource, Deref, DerefMut)]
struct RateLimiter(Mutex<Timer>);

impl Plugin for RosCapturePlugin {
    fn build(&self, app: &mut App) {
        let config = self.config.clone();
        let extent = Extent3d {
            width: config.width,
            height: config.height,
            ..Default::default()
        };
        let mut render_target_image =
            Image::new_target_texture(extent.width, extent.height, config.texture_format);
        render_target_image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        let render_target_handle = images.add(render_target_image);

        app.insert_resource(config)
            .insert_resource(ImageHandle(render_target_handle.clone()))
            .add_systems(Startup, setup_camera)
            .add_systems(Update, sync_camera);

        let render_app = app.sub_app_mut(RenderApp);

        let mut graph = render_app.world_mut().resource_mut::<RenderGraph>();
        graph.add_node(ImageCopy, ImageCopyDriver);
        graph.add_node_edge(bevy::render::graph::CameraDriverLabel, ImageCopy);

        render_app
            .insert_resource(self.config.clone())
            .insert_resource(self.context.clone())
            .insert_resource(ImageCopier::new(render_target_handle.clone(), extent))
            .insert_resource(RateLimiter(Mutex::new(Timer::new(
                Duration::from_secs_f64(1.0 / 60.0),
                TimerMode::Once,
            ))))
            .add_systems(
                Render,
                (
                    setup_copier,
                    receive_image_from_buffer.run_if(
                        |t: Res<Time>, image_copier: Res<ImageCopier>, r: Res<RateLimiter>| {
                            let mut r = r.lock().unwrap();
                            r.tick(t.delta());
                            if !r.is_finished() {
                                return false;
                            }
                            r.reset();
                            image_copier
                                .copying
                                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                                .is_ok()
                        },
                    ),
                )
                    .chain()
                    .after(RenderSystems::Render),
            );
    }
}

#[derive(Component)]
pub struct CaptureCamera;
fn setup_camera(
    mut commands: Commands,
    render_target_handle: Res<ImageHandle>,
    config: Res<CaptureConfig>,
) {
    commands.spawn((
        Camera3d::default(),
        Bloom::NATURAL,
        Camera {
            target: render_target_handle.0.clone().into(),
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: config.fov_y,
            near: 0.1,
            far: 500000000.0,
            ..default()
        }),
        Exposure::SUNLIGHT,
        Msaa::Off,
        Fxaa::default(),
        Hdr,
        CaptureCamera,
    ));
}

fn sync_camera(
    target: Single<&Transform, (With<MainCamera>, Without<CaptureCamera>)>,
    mut our: Single<&mut Transform, (With<CaptureCamera>, Without<MainCamera>)>,
) {
    our.translation = target.translation;
    our.scale = target.scale;
    our.rotation = target.rotation;
}

fn compute_camera(
    fov_y: f32,
    hdr: Header,
    width: u32,
    height: u32,
    data: &[u8],
) -> (CameraInfo, r2r::sensor_msgs::msg::Image) {
    let fov_y = fov_y as f64;
    let (width, height) = (width, height);

    let (fov_y, fov_x) = {
        let aspect = width as f64 / height as f64;
        let fov_x = 2.0 * ((fov_y / 2.0).tan() * aspect).atan();
        (fov_y, fov_x)
    };

    let f_x = width as f64 / (2.0 * (fov_x / 2.0).tan());
    let f_y = height as f64 / (2.0 * (fov_y / 2.0).tan());

    let c_x = width as f64 / 2.0;
    let c_y = height as f64 / 2.0;

    // Removed x-axis flip; rely on optical rotation instead

    (
        CameraInfo {
            header: hdr.clone(),
            height,
            width,
            distortion_model: "plumb_bob".to_string(),
            d: vec![0.000, 0.000, 0.000, 0.000, 0.000],
            k: vec![f_x, 0.0, c_x, 0.0, f_y, c_y, 0.0, 0.0, 1.0],
            p: vec![f_x, 0.0, c_x, 0.0, 0.0, f_y, c_y, 0.0, 0.0, 0.0, 1.0, 0.0],
            r: vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            binning_x: 0,
            binning_y: 0,
            roi: RegionOfInterest {
                x_offset: 0,
                y_offset: 0,
                height,
                width,
                do_rectify: true,
            },
        },
        r2r::sensor_msgs::msg::Image {
            header: hdr,
            height,
            width,
            encoding: "rgb8".to_string(),
            is_bigendian: 0,
            step: width * 3,
            data: Vec::from(data),
        },
    )
}
