use crate::robomaster::outpost::consts::ROTATION_SPEED;
use bevy::prelude::Transform;

pub struct RotationController {
    speed: f32,
    clockwise: bool,
}

impl RotationController {
    pub fn new(clockwise: bool) -> Self {
        Self {
            speed: ROTATION_SPEED,
            clockwise,
        }
    }

    fn rotate(&self, transform: &mut Transform, angle: f32) {
        transform.rotate_y(angle);
    }

    pub fn step(&self, transform: &mut Transform, dt: f32) {
        let sgn = if self.clockwise { 1.0 } else { -1.0 };
        self.rotate(transform, sgn * self.speed * dt);
    }
}
