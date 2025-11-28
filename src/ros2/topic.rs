use bevy::prelude::Resource;
use r2r::geometry_msgs::msg::PoseStamped;
use r2r::sensor_msgs::msg::{CameraInfo, CompressedImage, Image};
use r2r::tf2_msgs::msg::TFMessage;
use r2r::{QosProfile, WrappedTypesupport};
use std::sync::mpsc::SyncSender;

#[derive(Resource)]
pub struct TopicPublisher<T: RosTopic> {
    sender: SyncSender<T::T>,
}

impl<T: RosTopic> TopicPublisher<T> {
    pub(super) fn new(sender: SyncSender<T::T>) -> Self {
        TopicPublisher { sender }
    }

    pub fn publish(&self, message: T::T) {
        let _ = self.sender.try_send(message);
    }
}

#[macro_export]
macro_rules! publisher {
    ($node:ident,$topic:ty) => {
        {
            let (sender, receiver): (
                ::std::sync::mpsc::SyncSender<<$topic as crate::ros2::topic::RosTopic>::T>,
                ::std::sync::mpsc::Receiver<<$topic as crate::ros2::topic::RosTopic>::T>,
            ) = ::std::sync::mpsc::sync_channel(1024);

            let publisher = $node.create_publisher(
                <$topic>::TOPIC,
                <$topic>::QOS,
            ).unwrap();

            (receiver, sender, publisher)
        }
    };
    ($atomic:expr, $node:ident, $topic:ty) => {{
        let atomic = $atomic.clone();
        let (receiver,sender,publisher) = publisher!($node, $topic);
        ::std::thread::spawn(move || {
            while !atomic.load(::std::sync::atomic::Ordering::Acquire) {
                let mut did_work = false;
                loop {
                    match receiver.recv_timeout(Duration::from_secs(1)) {
                        Ok(m) => {
                            let mut sent = false;
                            while !sent {
                                match publisher.publish(&m) {
                                    Ok(_) => sent = true,
                                    Err(_) => {
                                        let _ = receiver.try_recv();
                                    }
                                }
                            }
                            did_work = true;
                        }
                        Err(::std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(::std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
                if !did_work {
                    ::std::thread::sleep(::std::time::Duration::from_millis(1));
                }
            }
        });
        crate::ros2::topic::TopicPublisher::<$topic>::new(sender)
    }};

    ($atomic:expr, $app:ident, $node:ident, $($topic:ty),* $(,)?) => {
        $(
            $app.insert_resource(publisher!($atomic, $node, $topic));
        )*
    };
}

pub trait RosTopic {
    type T: WrappedTypesupport + 'static;
    const TOPIC: &'static str;
    const QOS: QosProfile;
}

macro_rules! topic {
    ($topic:ident, $typ:ty, $url:literal, $qos:expr) => {
        pub struct $topic;
        impl RosTopic for $topic {
            type T = $typ;
            const TOPIC: &'static str = $url;
            const QOS: QosProfile = $qos;
        }
    };
    ($topic:ident, $typ:ty, $url:expr) => {
        topic!($topic, $typ, $url, ::r2r::QosProfile::default());
    };
    ($($url:literal as $typ:ty as $topic:ident);*$(;)?) => {
        $(
            topic!($topic, $typ, $url);
        )*
    }
}

topic!(
    "/camera_info" as CameraInfo as CameraInfoTopic;
    "/image_raw" as Image as ImageRawTopic;
    "/image_compressed" as CompressedImage as ImageCompressedTopic;
    "/tf" as TFMessage as GlobalTransformTopic;
    "/gimbal_pose" as PoseStamped as GimbalPoseTopic;
    "/odom_pose" as PoseStamped as OdomPoseTopic;
    "/camera_pose" as PoseStamped as CameraPoseTopic
);
