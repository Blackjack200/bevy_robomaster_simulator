use avian3d::prelude::*;
use bevy::prelude::*;

use crate::components::{
    Controlled, Infantry, InfantryChassis, InfantryGimbal, InfantryLaunchOffset, ProjectileCooldown,
};

pub fn uav_launch(
    time: Res<Time>,
    mut commands: Commands,
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
    mut timer: Local<Option<Timer>>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    let mut timer = timer.get_or_insert(Timer::from_seconds(1.0, TimerMode::Once));
    timer.tick(time.delta());
    if !timer.is_finished() {
        return;
    }
    timer.reset();
    if keyboard.pressed(KeyCode::KeyP) {
        commands.spawn((
            RigidBody::Static,
            WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("uav.glb"))),
            Transform::IDENTITY.with_translation(
                infantry.0.translation + (gimbal.0.rotation() * launch_offset.translation),
            ),
        ));
    }
}
