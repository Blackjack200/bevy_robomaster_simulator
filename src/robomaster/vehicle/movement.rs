use avian3d::prelude::forces::ForcesItem;
use avian3d::prelude::*;
use bevy::prelude::*;

#[derive(Component, Clone, Debug)]
pub struct VehicleDynamic {
    pub max_speed: f32,           // m/s
    pub linear_acceleration: f32, // m/s^2

    n: f32,
}

impl Default for VehicleDynamic {
    fn default() -> Self {
        Self {
            max_speed: 4.0,
            linear_acceleration: 10.0,
            n: 10.0,
        }
    }
}

impl VehicleDynamic {
    pub fn linear(
        &mut self,
        forces: &mut ForcesItem,
        mass: f32,
        gimbal_transform: &GlobalTransform,
        input: Vec2,
        dt: f32,
    ) {
        let lin_vel = forces.linear_velocity();
        let acceleration = self.linear_accelerate(input, gimbal_transform, lin_vel);
        forces.apply_linear_impulse(acceleration * mass * dt);
    }

    fn linear_accelerate(
        &mut self,
        input: Vec2,
        gimbal_transform: &GlobalTransform,
        current_velocity: Vec3,
    ) -> Vec3 {
        if input.length_squared() == 0.0 {
            return Vec3::ZERO;
        }
        let forward = gimbal_transform.forward().with_y(0.0);
        let right = gimbal_transform.right().with_y(0.0);
        let forward_xz = forward.with_y(0.0).normalize_or_zero();
        let right_xz = right.with_y(0.0).normalize_or_zero();
        let dirc = (forward_xz * input.y + right_xz * input.x).normalize_or_zero();
        dirc * self.linear_acceleration
            * (1.0 - (current_velocity.length() / self.max_speed).powf(self.n))
    }
}
