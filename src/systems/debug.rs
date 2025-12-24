use bevy::prelude::*;
use bevy::render::view::screenshot::{Capturing, Screenshot, save_to_disk};
use bevy::window::{CursorIcon, SystemCursorIcon, Window};

use crate::components::SlapperInfantry;
use crate::robomaster::prelude::{Armor, ArmorLabel, ArmorRoot};
use crate::statistic::ProjectileStatistics;

fn create_help_text(auto_aim: bool, stats: &ProjectileStatistics) -> Text {
    format!(
        "auto-aim={} total={} accurate={} pct={:.2}\nControls: F2-Screenshot F3-Change Camera | WASD-Move Mouse-Look Space-Shoot",
        if auto_aim { "ON " } else { "OFF" },
        stats.launch_count,
        stats.accurate_count,
        stats.accurate_pct()
    )
        .into()
}

pub fn spawn_text(commands: &mut Commands) {
    commands.spawn((
        Text::new(""),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        },
    ));
}

#[cfg(feature = "ros2")]
pub fn update_help_text(
    mut text: Query<&mut Text>,
    auto_aim: Res<crate::ros2::plugin::SubscribeAutoAim>,
    stats: Res<ProjectileStatistics>,
) {
    for mut text in text.iter_mut() {
        *text = create_help_text(auto_aim.load(std::sync::atomic::Ordering::Acquire), &stats);
    }
}

#[cfg(not(feature = "ros2"))]
pub fn update_help_text(mut text: Query<&mut Text>, stats: Res<ProjectileStatistics>) {
    for mut text in text.iter_mut() {
        *text = create_help_text(false, &stats);
    }
}

pub fn change_appearance(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    armor: Query<&mut ArmorRoot, With<SlapperInfantry>>,
    owned: Query<&mut Armor, With<SlapperInfantry>>,
) {
    if keyboard.pressed(KeyCode::ShiftLeft) && keyboard.just_pressed(KeyCode::KeyC) {
        let mut n_type = None;
        for mut armor in armor {
            let seq = ArmorLabel::sequence_small();
            armor.counter += 1;
            armor.counter %= seq.len();
            let new_typ = seq[armor.counter];
            n_type = Some(new_typ);
            armor.set_sticker(new_typ, &mut commands);
        }
        if let Some(n_type) = n_type {
            for mut own in owned {
                own.label = n_type;
            }
        }
    }
}

pub fn screenshot_on_f2(mut commands: Commands, mut counter: Local<u32>) {
    let path = format!("./screenshot-{}.png", *counter);
    *counter += 1;
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
}

pub fn screenshot_saving(
    mut commands: Commands,
    screenshot_saving: Query<Entity, With<Capturing>>,
    window: Single<Entity, With<Window>>,
) {
    match screenshot_saving.iter().count() {
        0 => {
            commands.entity(*window).remove::<CursorIcon>();
        }
        x if x > 0 => {
            commands
                .entity(*window)
                .insert(CursorIcon::from(SystemCursorIcon::Progress));
        }
        _ => {}
    }
}
