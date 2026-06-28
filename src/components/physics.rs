use avian3d::prelude::*;
use bevy::prelude::*;
use std::collections::HashMap;

#[derive(PhysicsLayer, Default, Clone, Copy, Debug)]
pub enum GameLayer {
    #[default]
    Default,
    VehicleSelf,
    VehicleOther,
    ProjectileSelf,
    ProjectileOther,
    Environment,
}

#[derive(Component, Deref, DerefMut)]
pub struct ProjectileLifetime(pub Timer);

#[derive(Resource, Deref, DerefMut)]
pub struct ProjectileCooldown(pub Timer);

#[derive(Resource)]
pub struct ProjectileSetting(pub Handle<Mesh>, pub Handle<StandardMaterial>);

#[derive(Resource)]
pub struct DartSetting(pub Handle<WorldAsset>);

#[derive(Component)]
pub struct GroundRoot;

#[derive(Component)]
pub struct DartLaunch;

#[derive(Component)]
pub struct DartProjectile;

#[derive(Component, Deref, DerefMut)]
pub struct PreciousCollision(
    pub  HashMap<
        String,
        (
            ColliderConstructorHierarchy,
            CollisionLayers,
            Visibility,
            Option<RigidBody>,
        ),
    >,
);

pub const PROJECTILE_LIFETIME_SECS: f32 = 5.0;
