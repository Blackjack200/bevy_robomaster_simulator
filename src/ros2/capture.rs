use crate::ros2::plugin::MainCamera;
use crate::ros2::topic::{CameraInfoTopic, ImageCompressedTopic, ImageRawTopic, TopicPublisher};
use crate::util::image::compress_image;
use bevy::anti_alias::fxaa::Fxaa;
use bevy::camera::Exposure;
use bevy::core_pipeline::tonemapping::Tonemapping;
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
            Buffer, BufferDescriptor, BufferUsages, Extent3d, MapMode, TexelCopyBufferInfo,
            TexelCopyBufferLayout, TextureFormat, TextureUsages,
        },
        renderer::{RenderContext, RenderDevice},
    },
};
use r2r::Clock;
use r2r::sensor_msgs::msg::{CameraInfo, RegionOfInterest};
use r2r::std_msgs::msg::Header;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Resource, Clone)]
pub struct CaptureConfig {
    pub width: u32,
    pub height: u32,
    pub texture_format: TextureFormat,
    pub fov_y: f32,
    pub publish_compressed: bool,
}

#[derive(Resource)]
struct ImageCopier {
    src_image: Handle<Image>,
    extent: Extent3d,
    queue: Mutex<VecDeque<(Buffer, r2r::builtin_interfaces::msg::Time)>>,
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
        let ros_ctx = world.get_resource::<RosCaptureContext>().unwrap();

        let src_image = gpu_images.get(&image_copier.src_image).unwrap();

        let block_dimensions = src_image.texture_format.block_dimensions();
        let block_size = src_image.texture_format.block_copy_size(None).unwrap();
        let padded_bytes_per_row = RenderDevice::align_copy_bytes_per_row(
            (src_image.size.width as usize / block_dimensions.0 as usize) * block_size as usize,
        );

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
        image_copier.queue.lock().unwrap().push_back((
            buffer,
            Clock::to_builtin_time(&ros_ctx.clock.lock().unwrap().get_now().unwrap()),
        ));
        Ok(())
    }
}

unsafe fn escape_may_ub<T>(r: &T) -> &'static T {
    unsafe { &*(r as *const T) }
}

fn receive_image_from_buffer(
    image_copier: Res<ImageCopier>,
    config: Res<CaptureConfig>,
    ctx: Res<RosCaptureContext>,
) {
    let (width, height, texture_format) = (config.width, config.height, config.texture_format);

    let mut guard = image_copier.queue.lock().unwrap();
    let all = guard.drain(..).fold(vec![], |mut all, (buffer, time)| {
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
        all.push((time, r));
        all
    });
    drop(guard);

    // what fuck
    let config = unsafe { escape_may_ub(config.into_inner()) };
    for (time, r) in all.into_iter() {
        let mut ctx = ctx.clone();
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
                let optical_frame_hdr = Header {
                    stamp: time.clone(),
                    frame_id: "camera_optical_frame".to_string(),
                };
                ctx.camera_info.publish(compute_camera_intrinsic(
                    optical_frame_hdr.clone(),
                    width,
                    height,
                    config.fov_y,
                ));
                if config.publish_compressed {
                    ctx.image_compressed.publish(compress_image(
                        optical_frame_hdr,
                        width,
                        height,
                        image_data,
                    ));
                } else {
                    ctx.image_raw.publish(raw_image(
                        optical_frame_hdr,
                        config.width,
                        config.height,
                        image_data,
                    ));
                }
            })
            .detach();
    }
}

#[derive(Resource, Clone)]
pub struct RosCaptureContext {
    pub clock: Arc<Mutex<Clock>>,
    pub camera_info: TopicPublisher<CameraInfoTopic>,
    pub image_raw: TopicPublisher<ImageRawTopic>,
    pub image_compressed: TopicPublisher<ImageCompressedTopic>,
}

pub struct RosCapturePlugin {
    pub config: CaptureConfig,
    pub context: RosCaptureContext,
}

#[derive(Resource, Deref)]
pub struct ImageHandle(Handle<Image>);

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
            .add_systems(
                Render,
                receive_image_from_buffer.after(RenderSystems::Render),
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
        Tonemapping::None,
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

fn raw_image(hdr: Header, width: u32, height: u32, data: &[u8]) -> r2r::sensor_msgs::msg::Image {
    r2r::sensor_msgs::msg::Image {
        header: hdr,
        height,
        width,
        encoding: "rgb8".to_string(),
        is_bigendian: 0,
        step: width * 3,
        data: Vec::from(data),
    }
}

fn compute_camera_intrinsic(hdr: Header, width: u32, height: u32, fov_y: f32) -> CameraInfo {
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
    CameraInfo {
        header: hdr,
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
    }
}
