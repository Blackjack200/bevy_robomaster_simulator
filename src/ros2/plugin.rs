use crate::capture::driver::CaptureConfig;
use crate::robomaster::prelude::{PowerRune, RuneIndex};
use crate::ros2::capture::{RosCaptureContext, RosCapturePlugin};
use crate::ros2::topic::*;
use crate::{
    Controlled, InfantryChassis, InfantryGimbal, InfantryLaunchOffset, arc_mutex, projectile_launch,
};
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use r2r::ClockType::SystemTime;
use r2r::rm_interfaces::msg::GimbalCmd;
use r2r::{Clock, Context, Node, std_msgs::msg::Header, tf2_msgs::msg::TFMessage};
use std::collections::HashMap;
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

macro_rules! tf_tree {
    (stamp: $stamp:expr;$root:literal { $($content:tt)* }) => {{
        let stamp = $stamp;
        let mut transform_stamped = vec![];
        let _parent = $root;
        let _current = $root;
        tf_tree!(@frame transform_stamped, stamp, _parent, _current, $($content)*);

        transform_stamped
    }};

    (@header $stamp:ident, $current:ident) => {
        Header {
            stamp: $stamp.clone(),
            frame_id: $current.to_string(),
        }
    };

    (@frame $tf_vec:ident, $stamp:ident, $parent:ident, $current:ident,
        $curr_name:literal as ($translation:expr, $rotation:expr) $(for $pub_:ident)?
        {$($children:tt)*}
        $($remaining:tt)*
    ) => {
        {
            let $parent = &$current;
            let $current = $curr_name;
            $crate::add_tf_frame!($tf_vec, tf_tree!(@header $stamp, $parent), $current, $translation, $rotation);
            $(
                $pub_.publish($crate::pose!(tf_tree!(@header $stamp, $current)));
            )*
            tf_tree!(@frame $tf_vec, $stamp, $parent, $current, $($children)*);
        }
        tf_tree!(@frame $tf_vec, $stamp, $parent, $current, $($remaining)*);
    };

    (@frame $tf_vec:ident, $stamp:ident, $parent:ident, $current:ident,
    $(let $p_name:ident = $p_expr:expr;)*
        for ($($elem:tt),+$(,)?) in $iter:ident {
            $(let $name:ident = $expr:expr;)*
            pub $curr_name:ident as ($translation:expr, $rotation:expr) $(for $pub_:ident)?;
            $($children:tt)*
        }
        $($remaining:tt)*
    ) => {
        $(let $p_name = $p_expr;)*
        for ($($elem),+) in $iter {
            $(let $name = $expr;)*
            let $parent = &$current;
            let $current = $curr_name;
            $crate::add_tf_frame!($tf_vec, tf_tree!(@header $stamp, $parent), $current, $translation, $rotation);
            $(
                $pub_.publish($crate::pose!(tf_tree!(@header $stamp, $current)));
            )*
            tf_tree!(@frame $tf_vec, $stamp, $parent, $current, $($children)*);
        }
        tf_tree!(@frame $tf_vec, $stamp, $parent, $current, $($remaining)*);
    };

    (@frame $tf_vec:ident, $stamp:ident, $parent:ident, $current:ident, $(;)? $(,)? $({})?) => { };
}

fn capture_rune(
    camera: Single<&GlobalTransform, With<MainCamera>>,
    gimbal: Single<&GlobalTransform, (With<Controlled>, With<InfantryGimbal>)>,
    muzzle_offset: Single<
        (&GlobalTransform, &Transform),
        (With<InfantryLaunchOffset>, With<Controlled>),
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
    let gimbal = gimbal.into_inner();
    let cam_rel = cam_transform.reparented_to(gimbal);
    let muzzle_rel = muzzle_offset.0.reparented_to(gimbal);
    let mut targets = targets.into_iter().fold(
        HashMap::<&PowerRune, Vec<(String, Transform)>>::new(),
        |mut map, (tf, target, name)| {
            // only use one target
            if !name.contains("_ACTIVATED") {
                return map;
            }
            let Ok((rune_tf, rune)) = runes.get(target.1) else {
                return map;
            };
            map.entry(rune).or_default().push((
                format!("power_rune_{:?}_{:?}", rune.mode(), target.0)
                    .to_string()
                    .to_lowercase(),
                tf.reparented_to(rune_tf),
            ));
            map
        },
    );

    let transform_stamped = tf_tree! {
        stamp: Clock::to_builtin_time(&res_unwrap!(clock).get_now().unwrap());

        "map" {
            "odom" as (gimbal.translation(), Quat::IDENTITY) for odom_pose_pub {
                "gimbal_link" as (Vec3::ZERO, gimbal.rotation() * muzzle_offset.1.rotation * Quat::from_euler(EulerRot::ZYX, 0.0, 0.0, PI / 2.0)) for gimbal_pose_pub {
                    "muzzle" as (muzzle_rel.translation, Quat::IDENTITY) {
                        "muzzle_link" as (Vec3::ZERO, Quat::IDENTITY) for muzzle_pose_pub{}
                    }
                    "camera_link" as (cam_rel.translation, Quat::IDENTITY) for camera_pose_pub {
                        "camera_optical_frame" as (Vec3::ZERO, Quat::from_euler(EulerRot::ZYX, -PI / 2.0, PI, PI / 2.0)) {}
                    }
                }
            }
            for (transform, rune) in runes {
                let name = format!("power_rune_{:?}", rune.mode()).to_string().to_lowercase();
                let tf = transform.compute_transform();
                pub name as (tf.translation, tf.rotation);
                let targets = targets.remove(rune).unwrap();
                for (name, tf) in targets {
                    pub name as (tf.translation, tf.rotation);
                }
            }
        }
    };

    tf_publisher.publish(TFMessage {
        transforms: transform_stamped,
    });
}

fn process_subscription(
    mut commands: Commands,
    gimbal_cmd: ResMut<TopicSubscriber<GimbalCmdTopic>>,
    gimbal: Single<
        (&mut Transform, &mut InfantryGimbal),
        (
            With<Controlled>,
            Without<InfantryChassis>,
            Without<InfantryLaunchOffset>,
        ),
    >,
    muzzle_offset: Single<
        (&GlobalTransform, &Transform),
        (With<InfantryLaunchOffset>, With<Controlled>),
    >,
) {
    let (mut gimbal_transform, mut gimbal_data) = gimbal.into_inner();
    loop {
        let Ok(Some(cmd)) = gimbal_cmd.try_recv() else {
            return;
        };
        if cmd.distance == -1.0 {
            return;
        }
        if cmd.fire_advice {
            if rand::random::<f32>() > 0.1 {
                return;
            }
            commands.queue(|w: &mut World| {
                w.run_system_once(projectile_launch).unwrap();
            });
        }
        let yaw_f32 = (cmd.yaw as f32).to_radians();
        let pitch_f32 = (cmd.pitch as f32 - 90.0).to_radians();
        gimbal_data.local_yaw = yaw_f32;
        gimbal_data.pitch = pitch_f32;
        let expected_rotation = Quat::from_euler(EulerRot::YXZ, yaw_f32, pitch_f32, 0.0);
        let current_rotation = muzzle_offset.0.rotation();
        let delta = expected_rotation * current_rotation.inverse();
        gimbal_transform.rotation = delta * gimbal_transform.rotation;
    }
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

#[derive(Resource, Deref, DerefMut)]
pub struct SubscribeAutoAim(AtomicBool);

#[derive(Default)]
pub struct ROS2Plugin {}

impl Plugin for ROS2Plugin {
    fn build(&self, app: &mut App) {
        let mut node = Node::create(Context::create().unwrap(), "simulator", "robomaster").unwrap();
        let signal_arc = Arc::new(AtomicBool::new(false));

        register_pub(signal_arc.clone(), app, &mut node);
        register_sub(signal_arc.clone(), app, &mut node);

        let camera_info = app
            .world_mut()
            .remove_resource::<TopicPublisher<CameraInfoTopic>>()
            .unwrap();
        let image_raw = app
            .world_mut()
            .remove_resource::<TopicPublisher<ImageRawTopic>>()
            .unwrap();
        let image_compressed = app
            .world_mut()
            .remove_resource::<TopicPublisher<ImageCompressedTopic>>()
            .unwrap();

        let clock = arc_mutex!(Clock::create(SystemTime).unwrap());

        app.insert_resource(RoboMasterClock(clock.clone()))
            .insert_resource(StopSignal(signal_arc.clone()))
            .insert_resource(SubscribeAutoAim(AtomicBool::new(false)))
            .add_plugins(RosCapturePlugin {
                config: CaptureConfig {
                    width: 1440,
                    height: 1080,
                    texture_format: TextureFormat::bevy_default(),
                },
                context: RosCaptureContext {
                    clock,
                    camera_info,
                    image_raw,
                    image_compressed,
                    fov_y: PI / 180.0 * 45.0,
                    publish_compressed: false,
                },
            })
            .add_systems(Last, cleanup_ros2_system)
            .add_systems(
                Update,
                process_subscription
                    .run_if(|enabled: Res<SubscribeAutoAim>| enabled.load(Ordering::Acquire)),
            )
            .add_systems(
                Update,
                |keyboard: Res<ButtonInput<KeyCode>>, enabled: Res<SubscribeAutoAim>| {
                    if keyboard.just_pressed(KeyCode::F5) {
                        info!("Toggling auto-aim subscription.");
                        let new_state = !enabled.fetch_xor(true, Ordering::AcqRel);
                        info!(
                            "Auto-aim subscription is now {}.",
                            if new_state { "ENABLED" } else { "DISABLED" }
                        );
                    }
                },
            )
            .add_systems(Update, capture_rune.after(TransformSystems::Propagate))
            .insert_resource(SpinThreadHandle(Some(thread::spawn(move || {
                while !signal_arc.load(Ordering::Acquire) {
                    node.spin_once(Duration::from_millis(1));
                }
            }))));
    }
}
