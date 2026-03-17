use avian3d::prelude::*;
use bevy::prelude::*;
use core::f32::consts::PI;

use crate::components::{
    Controlled, GameLayer, Infantry, InfantryChassis, InfantryGimbal, InfantryLaunchOffset,
    ProjectileCooldown, ProjectileLifetime, ProjectileSetting,
};
use crate::config::SimulationConfig;
use crate::robomaster::prelude::Projectile;
use crate::statistic::ProjectileStatistics;

pub fn uav_launch(
    time: Res<Time>,
    mut cooldown: ResMut<ProjectileCooldown>,
    mut stats: ResMut<ProjectileStatistics>,
    config: Res<SimulationConfig>,
    _asset_server: Res<AssetServer>,
    mut commands: Commands,
    setting: Res<ProjectileSetting>,
    infantry: Single<
        (&Transform, &LinearVelocity, &AngularVelocity),
        (With<Infantry>, With<Controlled>),
    >,
    gimbal: Single<
        (&GlobalTransform, &InfantryGimbal),
        (With<Controlled>, Without<InfantryChassis>),
    >,
    asset_server: Res<AssetServer>,
    launch_offset: Single<&Transform, (With<Controlled>, With<InfantryLaunchOffset>)>,
) {
    cooldown.tick(time.delta());
    if !cooldown.is_finished() {
        return;
    }
    cooldown.reset();

    stats.increase_launch();
    let direction = (gimbal.0.rotation() * launch_offset.rotation)
        .mul_vec3(Vec3::Y)
        .normalize_or_zero();
    if direction == Vec3::ZERO {
        return;
    }
    let vel = infantry.1.0 + direction * config.projectile.uav_vel;
    commands.spawn((
        RigidBody::Dynamic,
        Collider::cuboid(
            config.projectile.uav_size,
            config.projectile.uav_size,
            config.projectile.uav_size,
        ),
        Mass(config.projectile.mass),
        Friction::new(config.projectile.friction),
        Restitution::ZERO,
        LinearDamping(config.projectile.linear_damping),
        CollisionLayers::new(
            GameLayer::VehicleSelf,
            [
                GameLayer::Default,
                GameLayer::ProjectileOther,
                GameLayer::Environment,
            ],
        ),
        SceneRoot(asset_server.load("uav.glb#Scene0")),
        LinearVelocity(vel),
        AngularVelocity(infantry.2.0),
        Transform::IDENTITY.with_translation(
            infantry.0.translation + (gimbal.0.rotation() * launch_offset.translation),
        ),
        Projectile,
    ));
}
