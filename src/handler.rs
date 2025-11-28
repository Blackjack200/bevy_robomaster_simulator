use bevy::{
    asset::AssetServer,
    audio::AudioPlayer,
    ecs::{
        observer::On,
        system::{Commands, Query, Res},
    },
    transform::components::Transform,
};

use crate::{
    robomaster::prelude::{PowerRune, RuneActivated, RuneHit},
    statistic::increase_accurate,
};

pub fn on_activate(
    ev: On<RuneActivated>,
    mut commands: Commands,
    query: Query<&PowerRune>,
    asset_server: Res<AssetServer>,
) {
    let Ok(_rune) = query.get(ev.rune) else {
        return;
    };
    commands.spawn(AudioPlayer::new(asset_server.load("rune_activated.ogg")));
}

pub fn on_hit(ev: On<RuneHit>, _commands: Commands, query: Query<(&Transform, &PowerRune)>) {
    let Ok((_transform, _rune)) = query.get(ev.rune) else {
        return;
    };
    if ev.result.accurate {
        increase_accurate();
        //commands.spawn(AudioPlayer::new(asset_server.load("rune_activated.ogg")));
    }
}
