use crate::robomaster::outpost::consts::ROTATION_SPEED;
use bevy::prelude::Transform;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum RotationDirection {
    Clockwise,
    CounterClockwise,
}

impl RotationDirection {
    pub const fn sign(self) -> f32 {
        match self {
            Self::Clockwise => 1.0,
            Self::CounterClockwise => -1.0,
        }
    }
}

pub struct RotationController {
    speed: f32,
    direction: RotationDirection,
}

impl RotationController {
    pub fn new(direction: RotationDirection) -> Self {
        Self {
            speed: ROTATION_SPEED,
            direction,
        }
    }

    fn rotate(&self, transform: &mut Transform, angle: f32) {
        transform.rotate_y(angle);
    }

    pub fn step(&self, transform: &mut Transform, dt: f32) {
        self.rotate(transform, self.direction.sign() * self.speed * dt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_direction_sign_matches_legacy_bool() {
        assert_eq!(RotationDirection::Clockwise.sign(), 1.0);
        assert_eq!(RotationDirection::CounterClockwise.sign(), -1.0);
    }
}
