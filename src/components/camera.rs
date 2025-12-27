use bevy::prelude::*;
use std::sync::atomic::AtomicBool;

#[derive(Component)]
pub struct MainCamera {
    pub follow_offset: Vec3,
}

#[derive(Resource, PartialEq, Deref, DerefMut)]
pub struct CameraMode(pub FollowingType);

impl Default for CameraMode {
    fn default() -> Self {
        Self(FollowingType::Robot)
    }
}

#[derive(Resource, Deref, DerefMut)]
pub struct SubscribeAutoAim(pub AtomicBool);

#[derive(PartialEq, Clone, Copy)]
pub enum FollowingType {
    Free,
    Robot,
    ThirdPerson,
}
