use crate::ros2::plugin::MainCamera;
use crate::ros2::topic::{CameraInfoTopic, ImageCompressedTopic, ImageRawTopic, TopicPublisher};
use bevy::anti_alias::fxaa::Fxaa;
use bevy::camera::Exposure;
use bevy::core_pipeline::core_3d::graph::{Core3d, Node3d};
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::ecs::query::QueryItem;
use bevy::ecs::system::lifetimeless::Read;
use bevy::post_process::bloom::Bloom;
use bevy::render::camera::ExtractedCamera;
use bevy::render::render_graph::{RenderGraphExt, ViewNodeRunner};
use bevy::render::render_resource::{
    Origin3d, TextureAspect,
};
use bevy::render::view::{Hdr, ViewDepthTexture};
use bevy::tasks::AsyncComputeTaskPool;
use bevy::{
    image::TextureFormatPixelInfo,
    prelude::*,
    render::{
        render_asset::RenderAssets, render_graph::{self, NodeRunError, RenderGraph, RenderGraphContext, RenderLabel}, render_resource::{
            Buffer, BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, MapMode,
            PollType, TexelCopyBufferInfo, TexelCopyBufferLayout, TextureFormat, TextureUsages,
        }, renderer::{RenderContext, RenderDevice, RenderQueue},
        Extract,
        Render,
        RenderApp,
        RenderSystems,
    },
};
use r2r::sensor_msgs::msg::{CameraInfo, RegionOfInterest};
use r2r::std_msgs::msg::Header;
use r2r::Clock;
use std::sync::{
    atomic::{AtomicBool, Ordering}, Arc,
    Mutex,
};
use std::time::Duration;
use bevy::render::texture::GpuImage;

#[derive(Resource, Clone)]
pub struct CaptureConfig {
    pub width: u32,
    pub height: u32,
    pub texture_format: TextureFormat,
    pub fov_y: f32,
}

#[derive(Clone, Default, Resource, Deref, DerefMut)]
struct ImageCopiers(pub Vec<ImageCopier>);

#[derive(Clone, Component)]
struct ImageCopier {
    buffer: Buffer,
    depth_buffer: Buffer,
    copying: Arc<AtomicBool>,
    enabled: Arc<AtomicBool>,
    src_image: Handle<Image>,
}

impl ImageCopier {
    pub fn new(
        src_image: Handle<Image>,
        size: Extent3d,
        render_device: &RenderDevice,
    ) -> ImageCopier {
        let padded_bytes_per_row = RenderDevice::align_copy_bytes_per_row(size.width as usize) * 4;

        let cpu_buffer = render_device.create_buffer(&BufferDescriptor {
            label: None,
            size: padded_bytes_per_row as u64 * size.height as u64,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let depth_buffer = render_device.create_buffer(&BufferDescriptor {
            label: None,
            size: padded_bytes_per_row as u64 * size.height as u64,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        ImageCopier {
            buffer: cpu_buffer,
            depth_buffer,
            src_image,
            copying: Arc::new(AtomicBool::new(false)),
            enabled: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }
}

fn image_copy_extract(mut commands: Commands, image_copiers: Extract<Query<&ImageCopier>>) {
    commands.insert_resource(ImageCopiers(
        image_copiers.iter().cloned().collect::<Vec<ImageCopier>>(),
    ));
}

#[derive(Debug, PartialEq, Eq, Clone, Hash, RenderLabel)]
struct ImageCopy;

#[derive(Default)]
struct ImageCopyDriver;
impl render_graph::ViewNode for CopyDepthTextureNode {
    type ViewQuery = (Read<ExtractedCamera>, Read<ViewDepthTexture>);

    fn run<'w>(
        &self,
        _: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        (camera, depth_texture): QueryItem<'w, '_, Self::ViewQuery>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        // only run on depth-only camera
        if camera.order >= 0 {
            return Ok(());
        }

       
        Ok(())
    }
}

impl render_graph::Node for ImageCopyDriver {
    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let image_copiers = world.get_resource::<ImageCopiers>().unwrap();
        let gpu_images = world
            .get_resource::<RenderAssets<bevy::render::texture::GpuImage>>()
            .unwrap();

        for image_copier in image_copiers.iter() {
            if !image_copier.enabled() || image_copier.copying.load(Ordering::Acquire) {
                continue;
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

            encoder.copy_texture_to_buffer(
                src_image.texture.as_image_copy(),
                TexelCopyBufferInfo {
                    buffer: &image_copier.buffer,
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
        }
        Ok(())
    }
}

fn receive_image_from_buffer(
    t: Res<Time>,
    mut r: ResMut<RateLimiter>,
    image_copiers: Res<ImageCopiers>,
    render_device: Res<RenderDevice>,
    config: Res<CaptureConfig>,
    ctx: Res<RosCaptureContext>,
) {
    r.tick(t.delta());
    if !r.is_finished() {
        return;
    }
    r.reset();
    let ctx = Arc::new(ctx.clone());
    let config = Arc::new(config.clone());
    let (width, height, texture_format) = (config.width, config.height, config.texture_format);
    for image_copier in image_copiers.0.iter() {
        if !image_copier.enabled() {
            continue;
        }

        if image_copier
            .copying
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            let render_device = render_device.clone();
            let image_copier = image_copier.clone();
            let ctx = ctx.clone();
            let config = config.clone();
            AsyncComputeTaskPool::get()
                .spawn(async move {
                    let buffer_slice = image_copier.buffer.slice(..);
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
                    image_copier.buffer.unmap();
                    image_copier.copying.store(false, Ordering::Release);

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
                        let mut bevy_image =
                            Image::new_target_texture(width, height, texture_format);
                        bevy_image.data = Some(image_data);
                        bevy_image.try_into_dynamic().unwrap().to_rgb8().into_raw()
                    };
                    let optical_frame_hdr = Header {
                        stamp: Clock::to_builtin_time(
                            &ctx.clock.lock().unwrap().get_now().unwrap(),
                        ),
                        frame_id: "camera_optical_frame".to_string(),
                    };
                    /*ctx.image_compressed.publish(compress_image(
                        optical_frame_hdr.clone(),
                        width,
                        height,
                        image_data.clone(),
                    ));*/
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
    }
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

#[derive(Resource, Deref, DerefMut)]
struct RateLimiter(Timer);

/// A label for the render node that copies the depth buffer from that of the
/// camera to the [`DemoDepthTexture`].
#[derive(Clone, PartialEq, Eq, Hash, Debug, RenderLabel)]
struct CopyDepthTexturePass;

/// The render node that copies the depth buffer from that of the camera to the
/// [`DemoDepthTexture`].
#[derive(Default)]
struct CopyDepthTextureNode;

impl Plugin for RosCapturePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.config.clone())
            .add_systems(Update, sync_camera)
            .add_systems(Startup, setup_capture_scene);
        let render_app = app.sub_app_mut(RenderApp);

        render_app.add_render_graph_node::<ViewNodeRunner<CopyDepthTextureNode>>(
            Core3d,
            CopyDepthTexturePass,
        );
        // We have the texture copy operation run in between the prepasses and
        // the opaque pass. Since the depth rendering is part of the prepass, this
        // is a reasonable time to perform the operation.
        render_app.add_render_graph_edges(
            Core3d,
            (
                Node3d::EndPrepasses,
                CopyDepthTexturePass,
                Node3d::MainOpaquePass,
            ),
        );

        let mut graph = render_app.world_mut().resource_mut::<RenderGraph>();
        graph.add_node(ImageCopy, ImageCopyDriver);
        graph.add_node_edge(bevy::render::graph::CameraDriverLabel, ImageCopy);

        render_app
            .insert_resource(self.config.clone())
            .insert_resource(self.context.clone())
            .add_systems(ExtractSchedule, image_copy_extract)
            .insert_resource(RateLimiter(Timer::new(
                Duration::from_secs_f64(1.0 / 60.0),
                TimerMode::Once,
            )))
            .add_systems(
                Render,
                receive_image_from_buffer.after(RenderSystems::Render),
            );
    }
}

#[derive(Component)]
#[require(Camera, Camera3d, DepthPrepass)]
pub struct CaptureCamera;

fn setup_capture_scene(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    render_device: Res<RenderDevice>,
    config: Res<CaptureConfig>,
) {
    let size = Extent3d {
        width: config.width,
        height: config.height,
        ..Default::default()
    };
    let mut render_target_image =
        Image::new_target_texture(size.width, size.height, config.texture_format);
    render_target_image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    let render_target_handle = images.add(render_target_image);

    commands.spawn(ImageCopier::new(
        render_target_handle.clone(),
        size,
        &render_device,
    ));

    commands.spawn((
        Camera3d::default(),
        Bloom::NATURAL,
        Camera {
            target: render_target_handle.clone().into(),
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
        DepthPrepass,
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
    data: Vec<u8>,
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
            data,
        },
    )
}
