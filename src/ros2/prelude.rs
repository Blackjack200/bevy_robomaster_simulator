use bevy::math::{Mat3, Quat, Vec3};
use bevy::prelude::Transform;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct AverageRateLimiter {
    period: Duration,
    elapsed: Duration,
}

impl AverageRateLimiter {
    pub fn new(period: Duration) -> Self {
        Self {
            period,
            elapsed: period,
        }
    }

    pub fn from_hz(hz: f32) -> Self {
        assert!(hz.is_finite() && hz > 0.0);
        Self::new(Duration::from_secs_f32(1.0 / hz))
    }

    pub fn tick(&mut self, delta: Duration) {
        self.elapsed = self.elapsed.saturating_add(delta).min(self.period);
    }

    pub fn allow(&mut self) -> bool {
        if self.elapsed < self.period {
            return false;
        }
        self.elapsed = Duration::ZERO;
        true
    }
}

pub const M_ALIGN_MAT3: Mat3 = Mat3::from_cols(
    Vec3::new(0.0, -1.0, 0.0), // M[0,0], M[1,0], M[2,0]
    Vec3::new(0.0, 0.0, 1.0),  // M[0,1], M[1,1], M[2,1]
    Vec3::new(-1.0, 0.0, 0.0), // M[0,2], M[1,2], M[2,2]
);

#[inline]
pub fn transform(bevy_transform: Transform) -> r2r::geometry_msgs::msg::Transform {
    let align_rot_mat = M_ALIGN_MAT3;
    let align_quat = Quat::from_mat3(&align_rot_mat);
    let new_rotation = align_quat * bevy_transform.rotation * align_quat.inverse();
    let new_translation = align_rot_mat * bevy_transform.translation;
    r2r::geometry_msgs::msg::Transform {
        translation: r2r::geometry_msgs::msg::Vector3 {
            x: new_translation.x as f64,
            y: new_translation.y as f64,
            z: new_translation.z as f64,
        },
        rotation: r2r::geometry_msgs::msg::Quaternion {
            x: new_rotation.x as f64,
            y: new_rotation.y as f64,
            z: new_rotation.z as f64,
            w: new_rotation.w as f64,
        },
    }
}

#[macro_export]
macro_rules! add_tf_frame {
    ($ls:ident, $hdr:expr, $id:expr, $translation:expr, $rotation:expr) => {
        $ls.push(::r2r::geometry_msgs::msg::TransformStamped {
            header: $hdr.clone(),
            child_frame_id: $id.to_string(),
            transform: $crate::ros2::prelude::transform(
                ::bevy::prelude::Transform::IDENTITY
                    .with_translation($translation)
                    .with_rotation($rotation),
            ),
        });
    };
    ($ls:ident, $hdr:expr, $id:expr, $transform:expr) => {
        $ls.push(::r2r::geometry_msgs::msg::TransformStamped {
            header: $hdr.clone(),
            child_frame_id: $id.to_string(),
            transform: $crate::ros2::prelude::transform($transform),
        });
    };
}

#[macro_export]
macro_rules! pose {
    ($hdr:expr) => {
        ::r2r::geometry_msgs::msg::PoseStamped {
            header: $hdr.clone(),
            pose: ::r2r::geometry_msgs::msg::Pose {
                position: ::r2r::geometry_msgs::msg::Point {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                orientation: ::r2r::geometry_msgs::msg::Quaternion {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 1.0,
                },
            },
        }
    };
}
