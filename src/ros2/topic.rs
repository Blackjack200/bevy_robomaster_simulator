use bevy::prelude::{App, Resource};
use bevy::tasks::AsyncComputeTaskPool;
use bevy::tasks::futures_lite::StreamExt;
use bevy::tasks::futures_lite::future::block_on;
use futures::SinkExt;
use futures::channel::mpsc;
use futures::channel::mpsc::{Sender, TryRecvError};
use r2r::geometry_msgs::msg::PoseStamped;
use r2r::rm_interfaces::msg::GimbalCmd;
use r2r::sensor_msgs::msg::{CameraInfo, CompressedImage, Image};
use r2r::tf2_msgs::msg::TFMessage;
use r2r::visualization_msgs::msg::Marker;
use r2r::{Node, QosProfile, WrappedTypesupport};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

#[derive(Resource, Clone)]
pub struct TopicPublisher<T: RosTopic> {
    sender: Sender<T::T>,
}

impl<T: RosTopic> TopicPublisher<T> {
    pub(super) fn new(sender: Sender<T::T>) -> Self {
        TopicPublisher { sender }
    }

    pub fn publish(&self, message: T::T) {
        let mut sender = self.sender.clone();
        AsyncComputeTaskPool::get()
            .spawn(async move {
                let _ = sender.send(message).await;
            })
            .detach();
    }
}

#[derive(Resource)]
pub struct TopicSubscriber<T: RosTopic> {
    receiver: Arc<Mutex<Option<T::T>>>,
}

impl<T: RosTopic> TopicSubscriber<T> {
    pub(super) fn new() -> Self {
        TopicSubscriber {
            receiver: Arc::new(Mutex::new(None)),
        }
    }

    pub fn try_recv(&self) -> Result<Option<T::T>, TryRecvError> {
        Ok(self.receiver.lock().unwrap().take())
    }
}

fn subscriber<T: RosTopic>(node: &mut Node, signal: Arc<AtomicBool>) -> TopicSubscriber<T> {
    let mut subscriber = node.subscribe::<T::T>(T::TOPIC, T::QOS).unwrap();
    let sub = TopicSubscriber::new();
    let mutex = sub.receiver.clone();
    std::thread::spawn(move || {
        while !signal.load(std::sync::atomic::Ordering::Acquire) {
            match block_on(subscriber.next()) {
                Some(msg) => {
                    mutex.lock().unwrap().replace(msg);
                }
                None => continue,
            }
        }
    });
    sub
}

fn publisher<T: RosTopic>(node: &mut Node, signal: Arc<AtomicBool>) -> TopicPublisher<T> {
    let (sender, mut receiver) = mpsc::channel(1024);

    let publisher = node.create_publisher(T::TOPIC, T::QOS).unwrap();

    AsyncComputeTaskPool::get()
        .spawn(async move {
            while !signal.load(std::sync::atomic::Ordering::Acquire) {
                match receiver.next().await {
                    Some(m) => {
                        let _ = publisher.publish(&m);
                    }
                    None => break,
                }
            }
        })
        .detach();
    TopicPublisher::new(sender)
}

#[macro_export]
macro_rules! subscriber {
    ($signal:expr, $app:ident, $node:ident, $($topic:ty),* $(,)?) => {
        $(
            $app.insert_resource($crate::ros2::topic::subscriber::<$topic>($node, $signal));
        )*
    };
}

pub trait RosTopic {
    type T: WrappedTypesupport + Send + 'static;
    const TOPIC: &'static str;
    const QOS: QosProfile;
}

macro_rules! topic {
    ($topic:ident, $msg_typ:ty, $url:literal, $qos:expr) => {
        #[derive(Clone)]
        pub struct $topic;
        impl RosTopic for $topic {
            type T = $msg_typ;
            const TOPIC: &'static str = $url;
            const QOS: QosProfile = $qos;
        }
    };
    ($topic:ident, $msg_typ:ty, $url:literal) => {
        topic!($topic, $msg_typ, $url, ::r2r::QosProfile::default());
    };
    (pub {$($url:literal as $msg_typ:ty as $topic:ident $(with $qos: expr)?;)*} $($remaining:tt)*) => {
        $(
            topic!($topic, $msg_typ, $url $(, $qos)?);
        )*

        pub fn register_pub(atomic:Arc<AtomicBool>, app:&mut App, node:&mut Node) {
            $(
                app.insert_resource(publisher::<$topic>(node, atomic.clone()));
            )*
        }
        topic!($($remaining)*);
    };
    (sub {$($url:literal as $msg_typ:ty as $topic:ident $(with $qos: expr)?;)*} $($remaining:tt)*) => {
        $(
            topic!($topic, $msg_typ, $url $(, $qos)?);
        )*

        pub fn register_sub(atomic:Arc<AtomicBool>, app:&mut App, node:&mut Node) {
            $crate::subscriber!(atomic, app, node, $($topic,)*);
        }
        topic!($($remaining)*);
    };
    ( )=>{}
}

topic!(
    pub {
        "/camera_info" as CameraInfo as CameraInfoTopic;
        "/image_raw" as Image as ImageRawTopic;
        "/image_compressed" as CompressedImage as ImageCompressedTopic;
        "/tf" as TFMessage as GlobalTransformTopic;
        "/simulator/marker" as Marker as OutpostMarkerTopic;
        "/gimbal_pose" as PoseStamped as GimbalPoseTopic;
        "/odom_pose" as PoseStamped as OdomPoseTopic;
        "/muzzle_pose" as PoseStamped as MuzzlePoseTopic;
        "/camera_pose" as PoseStamped as CameraPoseTopic;
    }
    sub {
        "/rm_gimbal/cmd" as GimbalCmd as GimbalCmdTopic with QosProfile::sensor_data();
    }
);
