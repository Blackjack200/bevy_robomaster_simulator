use crate::robomaster::prelude::{PowerRune, Projectile, RuneIndex};
use crate::ros2::capture::{CaptureConfig, RosCaptureContext, RosCapturePlugin};
use crate::ros2::topic::*;
use crate::{
    InfantryGimbal, InfantryLaunchOffset, LocalInfantry, add_tf_frame, arc_mutex, pose, publisher,
};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use r2r::ClockType::SystemTime;
use r2r::{Clock, Context, Node, std_msgs::msg::Header, tf2_msgs::msg::TFMessage};
use std::f32::consts::PI;
use std::time::Duration;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};

macro_rules! res_unwrap {
    ($res:tt) => {
        $res.0.lock().unwrap()
    };
}

#[derive(Resource, Deref, DerefMut)]
struct StopSignal(Arc<AtomicBool>);

#[derive(Resource, Deref, DerefMut)]
struct SpinThreadHandle(Option<JoinHandle<()>>);

#[derive(Component)]
pub struct MainCamera;

#[derive(Resource, Deref, DerefMut)]
pub struct RoboMasterClock(pub Arc<Mutex<Clock>>);

fn capture_rune(
    camera: Single<&GlobalTransform, With<MainCamera>>,
    gimbal: Single<&GlobalTransform, (With<LocalInfantry>, With<InfantryGimbal>)>,
    muzzle_offset: Single<
        (&GlobalTransform, &Transform),
        (With<InfantryLaunchOffset>, With<LocalInfantry>),
    >,

    runes: Query<(&GlobalTransform, &PowerRune)>,
    targets: Query<(&GlobalTransform, &RuneIndex, &Name)>,

    clock: ResMut<RoboMasterClock>,
    tf_publisher: ResMut<TopicPublisher<GlobalTransformTopic>>,
    gimbal_pose_pub: ResMut<TopicPublisher<GimbalPoseTopic>>,
    odom_pose_pub: ResMut<TopicPublisher<OdomPoseTopic>>,
    muzzle_pose_pub: ResMut<TopicPublisher<MuzzlePoseTopic>>,
    camera_pose_pub: ResMut<TopicPublisher<CameraPoseTopic>>,
) {
    let cam_transform = camera.into_inner();
    let stamp = Clock::to_builtin_time(&res_unwrap!(clock).get_now().unwrap());
    let mut transform_stamped = vec![];
    let map_hdr = Header {
        stamp: stamp.clone(),
        frame_id: "map".to_string(),
    };
    let odom_hdr = Header {
        stamp: stamp.clone(),
        frame_id: "odom".to_string(),
    };
    let gimbal_hdr = Header {
        stamp: stamp.clone(),
        frame_id: "gimbal_link".to_string(),
    };
    let camera_hdr = Header {
        stamp: stamp.clone(),
        frame_id: "camera_link".to_string(),
    };
    let muzzle_hdr = Header {
        stamp: stamp.clone(),
        frame_id: "muzzle".to_string(),
    };
    let muzzle_link_hdr = Header {
        stamp: stamp.clone(),
        frame_id: "muzzle_link".to_string(),
    };

    gimbal_pose_pub.publish(pose!(gimbal_hdr));
    odom_pose_pub.publish(pose!(odom_hdr));
    muzzle_pose_pub.publish(pose!(muzzle_link_hdr));

    add_tf_frame!(
        transform_stamped,
        map_hdr.clone(),
        "odom",
        gimbal.translation(),
        Quat::IDENTITY
    );
    add_tf_frame!(
        transform_stamped,
        odom_hdr.clone(),
        "gimbal_link",
        Vec3::ZERO,
        gimbal.rotation()
    );
    let gimbal = gimbal.into_inner();
    let cam_rel = cam_transform.reparented_to(gimbal);
    let muzzle_rel = muzzle_offset.0.reparented_to(gimbal);
    add_tf_frame!(
        transform_stamped,
        gimbal_hdr.clone(),
        "muzzle",
        muzzle_rel.translation,
        muzzle_rel.rotation
    );
    add_tf_frame!(
        transform_stamped,
        muzzle_hdr.clone(),
        "muzzle_link",
        Vec3::ZERO,
        Quat::from_euler(EulerRot::ZYX, 0.0, 0.0, PI / 2.0)
    );
    add_tf_frame!(
        transform_stamped,
        gimbal_hdr.clone(),
        "camera_link",
        cam_rel.translation,
        cam_rel.rotation
    );
    add_tf_frame!(
        transform_stamped,
        camera_hdr.clone(),
        "camera_optical_frame",
        Vec3::ZERO,
        Quat::from_euler(EulerRot::ZYX, -PI / 2.0, PI, PI / 2.0)
    );
    for (transform, rune) in runes {
        add_tf_frame!(
            transform_stamped,
            map_hdr.clone(),
            format!("power_rune_{:?}", rune.mode())
                .to_string()
                .to_lowercase(),
            transform.compute_transform()
        );
    }
    for (target_transform, target, name) in targets {
        if !name.contains("_ACTIVATED") {
            continue;
        }
        if let Ok((_rune_transform, rune)) = runes.get(target.1) {
            add_tf_frame!(
                transform_stamped,
                Header {
                    stamp: stamp.clone(),
                    frame_id: format!("power_rune_{:?}", rune.mode())
                        .to_string()
                        .to_lowercase(),
                },
                format!("power_rune_{:?}_{:?}", rune.mode(), target.0)
                    .to_string()
                    .to_lowercase(),
                target_transform.reparented_to(_rune_transform)
            );
        }
    }
    tf_publisher.publish(TFMessage {
        transforms: transform_stamped,
    });
}

fn cleanup_ros2_system(
    mut exit: MessageReader<AppExit>,
    stop_signal: Res<StopSignal>,
    mut handle_res: ResMut<SpinThreadHandle>,
) {
    if exit.read().len() > 0 {
        stop_signal.store(true, Ordering::Release);
        if let Some(handle) = handle_res.take() {
            info!("Waiting for ROS 2 spin thread to join...");
            match handle.join() {
                Ok(_) => info!("ROS 2 thread successfully joined. Safe to exit."),
                Err(_) => error!("WARNING: ROS 2 thread panicked or failed to join."),
            }
        }
    }
}

#[derive(Default)]
pub struct ROS2Plugin {}

impl Plugin for ROS2Plugin {
    fn build(&self, app: &mut App) {
        let mut node = Node::create(Context::create().unwrap(), "simulator", "robomaster").unwrap();
        let signal_arc = Arc::new(AtomicBool::new(false));

        register_pub(signal_arc.clone(), app, &mut node);

        let camera_info = Arc::new(publisher!(signal_arc, node, CameraInfoTopic));
        let image_raw = Arc::new(publisher!(signal_arc, node, ImageRawTopic));
        let image_compressed = Arc::new(publisher!(signal_arc, node, ImageCompressedTopic));

        let clock = arc_mutex!(Clock::create(SystemTime).unwrap());

        app.insert_resource(RoboMasterClock(clock.clone()))
            .insert_resource(StopSignal(signal_arc.clone()))
            .add_plugins(RosCapturePlugin {
                config: CaptureConfig {
                    width: 1440,
                    height: 1080,
                    texture_format: TextureFormat::bevy_default(),
                    fov_y: PI / 180.0 * 45.0,
                },
                context: RosCaptureContext {
                    clock,
                    camera_info,
                    image_raw,
                    image_compressed,
                },
            })
            .add_systems(Last, cleanup_ros2_system)
            .add_systems(Update, capture_rune.after(TransformSystems::Propagate))
            .insert_resource(SpinThreadHandle(Some(thread::spawn(move || {
                while !signal_arc.load(Ordering::Acquire) {
                    node.spin_once(Duration::from_millis(1000));
                }
            }))));
    }
}
