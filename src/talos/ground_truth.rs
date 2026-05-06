use crate::components::{Controlled, Infantry};
use crate::robomaster::prelude::{Activation, MechanismState, PowerRune, RuneMode, Team};
use crate::talos::capture::{TalosCaptureContext, TalosFrameStamp};
use crate::talos::plugin::M_ALIGN_MAT3;
use avian3d::prelude::AngularVelocity;
use bevy::prelude::*;
use talos_ipc::*;

fn to_ros_vec3(v: Vec3) -> Vec3 {
    M_ALIGN_MAT3 * v
}

fn team_to_u8(team: &Team) -> u8 {
    match team {
        Team::Red => 0,
        Team::Blue => 1,
    }
}

fn activation_to_u8(a: &Activation) -> u8 {
    match a {
        Activation::Deactivated => 0,
        Activation::Activating => 1,
        Activation::Activated => 2,
        Activation::Completed => 3,
    }
}

fn mechanism_state_to_u8(s: &MechanismState) -> u8 {
    match s {
        MechanismState::Inactive { .. } => 0,
        MechanismState::Activating(_) => 1,
        MechanismState::Activated { .. } => 2,
        MechanismState::Failed { .. } => 3,
    }
}

fn rune_mode_to_u8(m: &RuneMode) -> u8 {
    match m {
        RuneMode::Small => 0,
        RuneMode::Large => 1,
    }
}

/// Compute yaw in the ROS reference frame from a Bevy GlobalTransform.
///
/// The alignment matrix maps Bevy (Y-up) → ROS (Z-up).
/// We convert the rotation quaternion through the alignment to extract the Z-up yaw.
fn ros_yaw(global_tf: &GlobalTransform) -> f32 {
    let align_quat = Quat::from_mat3(&M_ALIGN_MAT3);
    let ros_rot = align_quat * global_tf.rotation() * align_quat.inverse();
    let (_, _, yaw) = ros_rot.to_euler(EulerRot::ZYX);
    yaw
}

pub fn publish_ground_truth_system(
    context: Option<Res<TalosCaptureContext>>,
    frame_stamp: Res<TalosFrameStamp>,
    infantry_query: Query<
        (&GlobalTransform, Option<&AngularVelocity>, &Infantry),
        Without<Controlled>,
    >,
    controlled_query: Query<
        (&GlobalTransform, Option<&AngularVelocity>, &Infantry),
        With<Controlled>,
    >,
    rune_query: Query<(&GlobalTransform, &Transform, &PowerRune)>,
) {
    let Some(ctx) = context else {
        return;
    };

    let frame_seq = frame_stamp.frame_seq;
    let timestamp_ns = frame_stamp.timestamp_ns;

    let mut batch = GroundTruthBatch::default();
    batch.frame_seq = frame_seq;
    batch.timestamp_ns = timestamp_ns;

    // Collect robot ground truth from all infantry robots
    let all_robots = infantry_query.iter().chain(controlled_query.iter());

    for (global_tf, ang_vel, infantry) in all_robots {
        let pos_ros = to_ros_vec3(global_tf.translation());
        let (team, config) = (&infantry.0, &infantry.1);

        let vyaw = ang_vel
            .map(|av| {
                let ros_ang = to_ros_vec3(av.0);
                ros_ang.z
            })
            .unwrap_or(0.0);

        let yaw = ros_yaw(global_tf);

        if (batch.target_count as usize) < GROUND_TRUTH_MAX_TARGETS {
            let idx = batch.target_count as usize;
            batch.targets[idx] = GroundTruthTarget {
                frame_seq,
                timestamp_ns,
                team: team_to_u8(team),
                armor_label: config.1 as u8,
                is_outpost: 0,
                _pad1: 0,
                position: [pos_ros.x, pos_ros.y, pos_ros.z],
                vyaw,
                yaw,
                _pad: [0; 24],
            };
            batch.target_count += 1;
        }
    }

    // Collect rune ground truth
    for (global_tf, local_tf, power_rune) in rune_query.iter() {
        if (batch.rune_count as usize) >= GROUND_TRUTH_MAX_RUNES {
            break;
        }

        let pos_ros = to_ros_vec3(global_tf.translation());

        // Extract current rotation angle around the actual rune axis (-1, 0, -1).
        // The rune rotates via `rotate_local_axis(direction, angle)`, so we must
        // project the quaternion back onto that axis — not extract an Euler X angle.
        let rune_axis = Dir3::from_xyz(-1.0, 0.0, -1.0).unwrap();
        let (axis, angle) = local_tf.rotation.to_axis_angle();
        let current_angle = angle * axis.dot(*rune_axis).signum();

        let controller = power_rune.rotation_controller();
        let direction = if controller.is_clockwise() { 1 } else { -1 };

        let (sin_amplitude, sin_omega, relative_time, sin_offset) = controller
            .variable_params()
            .map(|(a, omega, t)| (a, omega, t, 2.090 - a))
            .unwrap_or((0.0, 0.0, 0.0, 0.0));

        let mut target_activations = [0u8; 5];
        if let Some(activations) = power_rune.activating_targets() {
            for (i, a) in activations.iter().enumerate() {
                if i < 5 {
                    target_activations[i] = activation_to_u8(a);
                }
            }
        }

        let idx = batch.rune_count as usize;
        batch.runes[idx] = GroundTruthRune {
            frame_seq,
            timestamp_ns,
            team: team_to_u8(&power_rune.team()),
            rune_mode: rune_mode_to_u8(&power_rune.mode()),
            mechanism_state: mechanism_state_to_u8(power_rune.state()),
            _pad1: 0,
            r_center_odom: [pos_ros.x, pos_ros.y, pos_ros.z],
            radius: 0.0,
            current_angle,
            v_roll: 0.0,
            direction,
            sin_amplitude,
            sin_omega,
            sin_phase: 0.0,
            sin_offset,
            relative_time,
            blade_id: -1,
            target_activations,
            _pad: [0; 20],
        };
        batch.rune_count += 1;
    }

    if let Ok(mut publisher) = ctx.publisher.lock() {
        publisher.publish_ground_truth(&batch);
    }
}
