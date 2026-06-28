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

impl GameLayer {
    pub fn environment_collision_layers() -> CollisionLayers {
        CollisionLayers::new(
            Self::Environment,
            [
                Self::Default,
                Self::VehicleSelf,
                Self::VehicleOther,
                Self::ProjectileSelf,
                Self::ProjectileOther,
            ],
        )
    }

    pub fn vehicle_body_collision_layers(is_self: bool) -> CollisionLayers {
        if is_self {
            CollisionLayers::new(
                Self::VehicleSelf,
                [Self::Default, Self::VehicleOther, Self::Environment],
            )
        } else {
            CollisionLayers::new(
                Self::VehicleOther,
                [
                    Self::Default,
                    Self::VehicleSelf,
                    Self::VehicleOther,
                    Self::Environment,
                ],
            )
        }
    }

    pub fn vehicle_armor_collision_layers(is_self: bool) -> CollisionLayers {
        if is_self {
            CollisionLayers::new(
                Self::VehicleSelf,
                [
                    Self::Default,
                    Self::VehicleOther,
                    Self::ProjectileOther,
                    Self::Environment,
                ],
            )
        } else {
            CollisionLayers::new(
                Self::VehicleOther,
                [
                    Self::Default,
                    Self::VehicleSelf,
                    Self::ProjectileSelf,
                    Self::Environment,
                ],
            )
        }
    }

    pub fn projectile_collision_layers(is_self: bool) -> CollisionLayers {
        if is_self {
            CollisionLayers::new(
                Self::ProjectileSelf,
                [Self::Default, Self::VehicleOther, Self::Environment],
            )
        } else {
            CollisionLayers::new(
                Self::ProjectileOther,
                [Self::Default, Self::VehicleSelf, Self::Environment],
            )
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_projectile_ignores_self_vehicle_and_projectiles() {
        let projectile = GameLayer::projectile_collision_layers(true);

        assert!(!projectile.interacts_with(GameLayer::vehicle_body_collision_layers(true)));
        assert!(!projectile.interacts_with(GameLayer::vehicle_armor_collision_layers(true)));
        assert!(!projectile.interacts_with(GameLayer::projectile_collision_layers(true)));
        assert!(!projectile.interacts_with(GameLayer::projectile_collision_layers(false)));
    }

    #[test]
    fn projectiles_hit_opposing_armor_and_environment() {
        let self_projectile = GameLayer::projectile_collision_layers(true);
        let other_projectile = GameLayer::projectile_collision_layers(false);

        assert!(self_projectile.interacts_with(GameLayer::vehicle_armor_collision_layers(false)));
        assert!(other_projectile.interacts_with(GameLayer::vehicle_armor_collision_layers(true)));
        assert!(self_projectile.interacts_with(GameLayer::environment_collision_layers()));
        assert!(other_projectile.interacts_with(GameLayer::environment_collision_layers()));
    }

    #[test]
    fn projectiles_do_not_hit_vehicle_body_colliders() {
        let self_projectile = GameLayer::projectile_collision_layers(true);
        let other_projectile = GameLayer::projectile_collision_layers(false);

        assert!(!self_projectile.interacts_with(GameLayer::vehicle_body_collision_layers(false)));
        assert!(!other_projectile.interacts_with(GameLayer::vehicle_body_collision_layers(true)));
    }
}
