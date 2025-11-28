use crate::robomaster::power_rune::common::RuneMode;
use crate::robomaster::power_rune::consts::ROTATION_BASELINE_SMALL;
use bevy::math::Dir3;
use bevy::prelude::Transform;
use rand::Rng;

struct VariableRotation {
    a: f32,
    omega: f32,
    t: f32,
}

impl VariableRotation {
    pub fn random(rng: &mut impl Rng) -> Self {
        let a = rng.random_range(0.780..=1.045);
        let omega = rng.random_range(1.884..=2.0);
        Self { a, omega, t: 0.0 }
    }

    pub fn advance(&mut self, dt: f32) {
        self.t += dt;
    }

    pub fn speed(&self) -> f32 {
        let b = 2.090 - self.a;
        self.a * (self.omega * self.t).sin() + b
    }
}

pub struct RotationController {
    baseline: f32,
    direction: Dir3,
    variable: Option<VariableRotation>,
    clockwise: bool,
}

impl RotationController {
    pub fn new(clockwise: bool) -> Self {
        Self {
            baseline: ROTATION_BASELINE_SMALL,
            direction: Dir3::from_xyz(-1.0, 0.0, -1.0).unwrap(),
            variable: None,
            clockwise,
        }
    }

    pub fn rotate(&self, transform: &mut Transform, angle: f32) {
        transform.rotate_local_axis(self.direction, angle);
    }

    pub fn set_variable(&mut self, rng: &mut impl Rng) {
        self.variable = Some(VariableRotation::random(rng));
        // 确保重置时间参数
        if let Some(ref mut variable) = self.variable {
            variable.t = 0.0;
        }
    }

    pub fn clear_variable(&mut self) {
        self.variable = None;
    }

    pub fn current_speed(&mut self, mode: RuneMode, dt: f32) -> f32 {
        let sgn = if self.clockwise { 1.0 } else { -1.0 };
        if mode == RuneMode::Small {
            return sgn * self.baseline;
        }
        // 大机关只有在激活状态下使用变量旋转
        if let Some(variable) = &mut self.variable {
            variable.advance(dt);
            return sgn * variable.speed();
        }
        sgn * self.baseline
    }
}
