use bevy::prelude::*;
use std::sync::atomic::Ordering;

use crate::components::{
    ActiveSlapper, Controlled, Infantry, InfantryChassis, InfantryGimbal, SlapperInfantry,
    SubscribeAutoAim,
};
use crate::config::SimulationConfig;
use crate::robomaster::vehicle::movement::VehicleDynamic;
use avian3d::prelude::*;

macro_rules! input {
    ($keyboard:ident, $forward:ident,$left:ident,$backward:ident,$right:ident) => {{
        let mut input = Vec2::ZERO;
        if $keyboard.pressed(KeyCode::$forward) {
            input.y += 1.0;
        }
        if $keyboard.pressed(KeyCode::$backward) {
            input.y -= 1.0;
        }
        if $keyboard.pressed(KeyCode::$right) {
            input.x += 1.0;
        }
        if $keyboard.pressed(KeyCode::$left) {
            input.x -= 1.0;
        }
        input
    }};
    ($keyboard:ident, $left:ident,$right:ident) => {{
        let mut input: f32 = 0.0;
        if $keyboard.pressed(KeyCode::$left) {
            input += 1.0;
        }
        if $keyboard.pressed(KeyCode::$right) {
            input += -1.0;
        }
        input
    }};
}

const CHASSIS_ROTATION_RESPONSE: f32 = 40.0;
const CHASSIS_ROTATION_STOP_EPSILON: f32 = 1e-3;

fn update_chassis_rotation(
    chassis_transform: &mut Transform,
    chassis_data: &mut InfantryChassis,
    input: f32,
    rotation_speed: f32,
    dt: f32,
) {
    let target_yaw_velocity = input * rotation_speed;
    let response = 1.0 - (-CHASSIS_ROTATION_RESPONSE * dt).exp();
    chassis_data.yaw_velocity += (target_yaw_velocity - chassis_data.yaw_velocity) * response;

    if chassis_data.yaw_velocity.abs() < CHASSIS_ROTATION_STOP_EPSILON
        && target_yaw_velocity.abs() < CHASSIS_ROTATION_STOP_EPSILON
    {
        chassis_data.yaw_velocity = 0.0;
    }

    chassis_data.yaw += chassis_data.yaw_velocity * dt;
    chassis_transform.rotation = Quat::from_euler(EulerRot::YXZ, chassis_data.yaw, 0.0, 0.0);
}

pub fn auto_aim_switch(keyboard: Res<ButtonInput<KeyCode>>, enabled: Res<SubscribeAutoAim>) {
    if keyboard.just_pressed(KeyCode::F5) {
        info!("Toggling auto-aim subscription.");
        let new_state = !enabled.fetch_xor(true, Ordering::AcqRel);
        info!(
            "Auto-aim subscription is now {}.",
            if new_state { "ENABLED" } else { "DISABLED" }
        );
    }
}

pub fn vehicle_controls(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    config: Res<SimulationConfig>,
    infantry: Single<(Forces, &Mass, &mut VehicleDynamic), (With<Infantry>, With<Controlled>)>,
    gimbal: Single<
        (&GlobalTransform, &InfantryGimbal),
        (With<Controlled>, Without<InfantryChassis>),
    >,
    chassis: Single<
        (&mut Transform, &mut InfantryChassis),
        (
            With<Controlled>,
            Without<InfantryGimbal>,
            With<InfantryChassis>,
            Without<Infantry>,
        ),
    >,
) {
    let input = input!(keyboard, KeyW, KeyA, KeyS, KeyD);
    let boost = if keyboard.pressed(KeyCode::ShiftLeft) {
        2.0
    } else {
        1.0
    };

    let (mut forces, &Mass(mass), mut dynamic) = infantry.into_inner();

    let dt = time.delta_secs();
    dynamic.linear(
        &mut forces,
        mass,
        gimbal.into_inner().0,
        input,
        time.delta_secs(),
        boost,
    );

    let input = input!(keyboard, KeyQ, KeyE);
    let (mut chassis_transform, mut chassis_data) = chassis.into_inner();
    update_chassis_rotation(
        &mut chassis_transform,
        &mut chassis_data,
        input,
        config.vehicle.rotation_speed,
        dt,
    );
}

pub fn remote_vehicle_controls(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    config: Res<SimulationConfig>,
    infantry: Single<
        (Forces, &Mass, &mut VehicleDynamic),
        (With<ActiveSlapper>, With<Infantry>, Without<Controlled>),
    >,
    gimbal: Single<
        (&GlobalTransform, &InfantryGimbal),
        (With<ActiveSlapper>, Without<InfantryChassis>),
    >,
    chassis: Single<
        (&mut Transform, &mut InfantryChassis),
        (With<ActiveSlapper>, Without<InfantryGimbal>),
    >,
) {
    let input = input!(keyboard, KeyI, KeyJ, KeyK, KeyL);
    let boost = if keyboard.pressed(KeyCode::ShiftRight) {
        2.0
    } else {
        1.0
    };

    let (mut forces, &Mass(mass), mut dynamic) = infantry.into_inner();

    let dt = time.delta_secs();
    dynamic.linear(
        &mut forces,
        mass,
        gimbal.into_inner().0,
        input,
        time.delta_secs(),
        boost,
    );

    let input = input!(keyboard, KeyU, KeyO);
    let (mut chassis_transform, mut chassis_data) = chassis.into_inner();
    update_chassis_rotation(
        &mut chassis_transform,
        &mut chassis_data,
        input,
        config.vehicle.rotation_speed,
        dt,
    );
}

pub fn gimbal_controls(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    config: Res<SimulationConfig>,
    // enabled: Res<SubscribeAutoAim>,
    gimbal: Single<
        (&mut Transform, &mut InfantryGimbal),
        (With<Controlled>, Without<InfantryChassis>),
    >,
) {
    //if enabled.load(Ordering::Acquire) {
    //    return;
    //}

    let dt = time.delta_secs();
    let (mut gimbal_transform, mut gimbal_data) = gimbal.into_inner();

    (gimbal_data.local_yaw, gimbal_data.pitch, _) =
        gimbal_transform.rotation.to_euler(EulerRot::YXZ);

    gimbal_data.local_yaw +=
        input!(keyboard, ArrowLeft, ArrowRight) * config.vehicle.gimbal_rotation_speed * dt;
    gimbal_data.pitch +=
        input!(keyboard, ArrowUp, ArrowDown) * config.vehicle.gimbal_rotation_speed * dt;

    gimbal_data.pitch = gimbal_data.pitch.clamp(
        -config.vehicle.gimbal_pitch_limit,
        config.vehicle.gimbal_pitch_limit,
    );

    let gimbal_rotation =
        Quat::from_euler(EulerRot::YXZ, gimbal_data.local_yaw, gimbal_data.pitch, 0.0);

    gimbal_transform.rotation = gimbal_rotation;
}

pub fn remote_gimbal_controls(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    config: Res<SimulationConfig>,
    gimbal: Single<
        (&mut Transform, &mut InfantryGimbal),
        (With<ActiveSlapper>, Without<InfantryChassis>),
    >,
) {
    let dt = time.delta_secs();
    let (mut gimbal_transform, mut gimbal_data) = gimbal.into_inner();

    (gimbal_data.local_yaw, gimbal_data.pitch, _) =
        gimbal_transform.rotation.to_euler(EulerRot::YXZ);

    if !keyboard.pressed(KeyCode::ShiftLeft) {
        gimbal_data.local_yaw +=
            input!(keyboard, KeyC, KeyB) * config.vehicle.gimbal_rotation_speed * dt;
    }
    gimbal_data.pitch += input!(keyboard, KeyF, KeyV) * config.vehicle.gimbal_rotation_speed * dt;
    gimbal_data.pitch = gimbal_data.pitch.clamp(
        -config.vehicle.gimbal_pitch_limit,
        config.vehicle.gimbal_pitch_limit,
    );

    let gimbal_rotation =
        Quat::from_euler(EulerRot::YXZ, gimbal_data.local_yaw, gimbal_data.pitch, 0.0);

    gimbal_transform.rotation = gimbal_rotation;
}

pub fn switch_slapper_control(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    children: Query<&Children>,
    slapper_roots: Query<Entity, (With<Infantry>, With<SlapperInfantry>)>,
    active_root: Query<Entity, (With<Infantry>, With<SlapperInfantry>, With<ActiveSlapper>)>,
) {
    if !keyboard.just_pressed(KeyCode::Tab) {
        return;
    }

    let roots: Vec<Entity> = slapper_roots.iter().collect();
    if roots.len() <= 1 {
        return;
    }

    let current = active_root.single().ok();
    let current_idx = current.and_then(|e| roots.iter().position(|&r| r == e));
    let next_idx = match current_idx {
        Some(idx) => (idx + 1) % roots.len(),
        None => 0,
    };

    // Remove ActiveSlapper from current
    if let Some(current_root) = current {
        commands.entity(current_root).remove::<ActiveSlapper>();
        for descendant in children.iter_descendants(current_root) {
            commands.entity(descendant).remove::<ActiveSlapper>();
        }
    }

    // Add ActiveSlapper to next
    let next_root = roots[next_idx];
    commands.entity(next_root).insert(ActiveSlapper);
    for descendant in children.iter_descendants(next_root) {
        commands.entity(descendant).insert(ActiveSlapper);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chassis_rotation_smoothly_ramps_towards_target_speed() {
        let mut transform = Transform::default();
        let mut chassis = InfantryChassis::default();

        update_chassis_rotation(&mut transform, &mut chassis, 1.0, 9.42, 0.016);

        assert!(chassis.yaw_velocity > 0.0);
        assert!(chassis.yaw_velocity < 9.42);
        assert!(chassis.yaw > 0.0);
    }

    #[test]
    fn chassis_rotation_smoothly_brakes_to_stop() {
        let mut transform = Transform::default();
        let mut chassis = InfantryChassis {
            yaw: 0.0,
            yaw_velocity: 9.42,
        };

        for _ in 0..60 {
            update_chassis_rotation(&mut transform, &mut chassis, 0.0, 9.42, 0.016);
        }

        assert!(chassis.yaw_velocity.abs() < 1e-2);
    }
}
