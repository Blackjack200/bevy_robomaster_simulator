use crate::arc_mutex;
use crate::capture::driver::CaptureConfig;
use crate::capture::{CaptureSource, IMAGE_HEIGHT, IMAGE_WIDTH};
use crate::components::{Controlled, InfantryChassis, InfantryGimbal, InfantryLaunchOffset};
use crate::config::SimulationConfig;
use crate::robomaster::prelude::{ArmorRoot, PowerRune, RuneIndex};
use crate::ros2::capture::{RosCaptureContext, RosCapturePlugin};
use crate::ros2::prelude::AverageRateLimiter;
use crate::ros2::prelude::transform;
use crate::ros2::topic::*;
use crate::systems::projectile_launch;
use crate::util::entity_query::HierarchyQuery;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use r2r::ClockType::SystemTime;
use r2r::geometry_msgs::msg::{Point, Pose, Vector3};
use r2r::std_msgs::msg::ColorRGBA;
use r2r::visualization_msgs::msg::Marker;
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

#[derive(Resource, Deref, DerefMut)]
pub struct RoboMasterClock(pub Arc<Mutex<Clock>>);

#[derive(Resource, Deref, DerefMut)]
struct FireRateLimiter(AverageRateLimiter);

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
    camera: Single<&GlobalTransform, With<CaptureSource>>,
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
    center: Query<(Entity, &GlobalTransform)>,
    qq: HierarchyQuery,
    armor: Query<(Entity, &GlobalTransform, &ArmorRoot)>,
    marker_pub: ResMut<TopicPublisher<OutpostMarkerTopic>>,
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
            for (entity, transform, armor) in armor {
                let name = format!("armor_{:?}", armor.id).to_string().to_lowercase();
                let tf = center.get(qq.of(entity).suffix("CENTER").any().one().unwrap()).unwrap().1.compute_transform();
                pub name as (tf.translation, tf.rotation);
            }
        }
    };

    let stamp = Clock::to_builtin_time(&res_unwrap!(clock).get_now().unwrap());
    for (entity, tf, armor) in armor {
        let name = format!("armor_{:?}", armor.id).to_string().to_lowercase();
        let mut tff = center
            .get(qq.of(entity).suffix("CENTER").any().one().unwrap())
            .unwrap()
            .1
            .compute_transform();
        tff.rotation = tf.rotation() * Quat::from_euler(EulerRot::ZYX, 0.0, 0.0, -PI / 2.0);
        let tf = transform(tff);
        marker_pub.publish(Marker {
            header: Header {
                stamp: stamp.clone(),
                frame_id: "map".to_string(),
            }
            .clone(),
            ns: "armors".to_string(),
            id: armor.id as i32,
            type_: Marker::CUBE as i32,
            action: Marker::ADD as i32,
            pose: Pose {
                position: Point {
                    x: tf.translation.x,
                    y: tf.translation.y,
                    z: tf.translation.z,
                },
                orientation: tf.rotation,
            },
            scale: Vector3 {
                x: 0.03,
                y: 0.15,
                z: 0.125,
            },
            color: ColorRGBA {
                r: 0.0,
                g: 1.0,
                b: 0.0,
                a: 0.0,
            },
            lifetime: r2r::builtin_interfaces::msg::Duration {
                sec: 0,
                nanosec: 300000000,
            },
            frame_locked: false,
            points: vec![],
            colors: vec![],
            texture_resource: "".to_string(),
            texture: Default::default(),
            uv_coordinates: vec![],
            text: "".to_string(),
            mesh_resource: "".to_string(),
            mesh_file: Default::default(),
            mesh_use_embedded_materials: false,
        });
    }

    tf_publisher.publish(TFMessage {
        transforms: transform_stamped,
    });
}

fn process_subscription(
    time: Res<Time>,
    mut commands: Commands,
    gimbal_cmd: ResMut<TopicSubscriber<GimbalCmdTopic>>,
    mut fire_rate_limiter: ResMut<FireRateLimiter>,
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
    fire_rate_limiter.tick(time.delta());
    loop {
        let Ok(Some(cmd)) = gimbal_cmd.try_recv() else {
            return;
        };
        if cmd.distance == -1.0 {
            return;
        }
        if cmd.fire_advice {
            if fire_rate_limiter.allow() {
                commands.queue(|w: &mut World| {
                    w.run_system_once(projectile_launch).unwrap();
                });
            }
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
            .insert_resource(FireRateLimiter(AverageRateLimiter::from_hz(10.0)))
            .add_plugins(RosCapturePlugin {
                config: CaptureConfig {
                    width: IMAGE_WIDTH,
                    height: IMAGE_HEIGHT,
                    texture_format: TextureFormat::bevy_default(),
                },
                context: RosCaptureContext {
                    clock,
                    camera_info,
                    image_raw,
                    image_compressed,
                    fov_y: SimulationConfig::default().camera.fov.to_radians(),
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
