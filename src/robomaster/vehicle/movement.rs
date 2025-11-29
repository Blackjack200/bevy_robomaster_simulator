use avian3d::prelude::forces::ForcesItem;
use avian3d::prelude::*;
use bevy::prelude::*;
use std::ops;

trait Number<Div: Copy + ops::Mul<Self, Output = Self>>:
    Copy
    + ops::Add<Output = Self>
    + ops::Sub<Output = Self>
    + ops::Mul<Output = Self>
    + ops::Div<Div, Output = Self>
    + Default
{
}

impl<Div, T> Number<Div> for T
where
    Div: Copy + ops::Mul<T, Output = T>,
    T: Copy
        + ops::Add<Output = T>
        + ops::Sub<Output = T>
        + ops::Mul<Output = T>
        + ops::Div<Div, Output = T>
        + Default,
{
}

#[derive(Debug, Clone)]
struct PIDController<DT: Copy + ops::Mul<T, Output = T>, T: Number<DT>> {
    k_p: DT,
    k_i: DT,
    k_d: DT,
    integral: T,
    prev_error: T,
}

impl<T: Number<f32>> Default for PIDController<f32, T>
where
    f32: Copy + ops::Mul<T, Output = T>,
{
    fn default() -> Self {
        Self {
            k_p: 10.0,
            k_i: 1.0,
            k_d: 0.1,
            integral: default(),
            prev_error: default(),
        }
    }
}

impl<DT: Copy + ops::Mul<T, Output = T>, T: Number<DT>> PIDController<DT, T> {
    pub fn step(&mut self, error: T, dt: DT) -> T {
        self.integral = self.integral + self.k_i * (dt * error);
        let derivative_term = (error - self.prev_error) / dt;
        self.prev_error = error;
        let p_term = self.k_p * error;
        let d_term = self.k_d * derivative_term;
        p_term + self.integral + d_term
    }
}

#[derive(Component, Clone, Debug)]
pub struct VehicleDynamic {
    pub max_speed: f32,           // m/s
    pub linear_acceleration: f32, // m/s^2

    vel: PIDController<f32, Vec3>,
}

impl Default for VehicleDynamic {
    fn default() -> Self {
        Self {
            max_speed: 4.0,
            linear_acceleration: 5.0,
            vel: default(),
        }
    }
}

impl VehicleDynamic {
    pub fn linear(
        &mut self,
        forces: &mut ForcesItem,
        gimbal_transform: &GlobalTransform,
        input: Vec2,
        dt: f32,
    ) {
        let lin_vel = forces.linear_velocity();
        let acceleration = self.linear_accelerate(input, gimbal_transform, lin_vel, dt);
        forces.apply_linear_acceleration(acceleration);
    }

    fn linear_accelerate(
        &mut self,
        input: Vec2,
        gimbal_transform: &GlobalTransform,
        current_velocity: Vec3,
        dt: f32,
    ) -> Vec3 {
        if input.length_squared() == 0.0 {
            return Vec3::ZERO;
        }
        let forward = gimbal_transform.forward().with_y(0.0);
        let right = gimbal_transform.right().with_y(0.0);
        let forward_xz = forward.with_y(0.0).normalize_or_zero();
        let right_xz = right.with_y(0.0).normalize_or_zero();
        let target_velocity =
            (forward_xz * input.y + right_xz * input.x).normalize_or_zero() * self.max_speed;
        let velocity_error = target_velocity - current_velocity;
        self.vel
            .step(velocity_error, dt)
            .clamp_length_max(self.linear_acceleration)
    }
}
