use crate::robomaster::power_rune::collision::RuneIndex;
use crate::robomaster::power_rune::common::RuneMode;
use crate::robomaster::power_rune::rune::PowerRune;
use crate::robomaster::power_rune::visual::RuneVisual;
use crate::robomaster::prelude::Team;
use crate::robomaster::visibility::{Activation, Controller, StatefulAppearanceCreator};
use crate::util::bevy::{drain_entities_by, insert_all_child};
use crate::{material, visibility};
use avian3d::prelude::CollisionEventsEnabled;
use bevy::asset::Assets;
use bevy::ecs::system::SystemParam;
use bevy::gizmos::GizmoAsset;
use bevy::prelude::{
    Children, Commands, Component, Entity, Name, On, Query, Res, ResMut, SceneSpawner, With,
};
use bevy::scene::SceneInstanceReady;
use std::collections::HashMap;

#[derive(Component)]
pub struct PowerRuneRoot;

fn build_targets(
    face_index: usize,
    face_entity: Entity,
    name_map: &mut HashMap<&str, Entity>,
    param: &mut PowerRuneParam,
    creator: &mut StatefulAppearanceCreator,
) -> Vec<RuneVisual> {
    let mut targets = Vec::new();
    for target_idx in 1..=5 {
        let prefix = format!("FACE_{}_TARGET_{}", face_index, target_idx);

        let padding_segments = creator.create_controller(
            drain_entities_by(name_map, |name| {
                name.starts_with(&format!("{}_PADDING", prefix))
            }),
            material!(on = { completed }),
        );
        let progress_segments = creator.create_controller(
            drain_entities_by(name_map, |name| {
                name.starts_with(&format!("{}_LEGGING_PROGRESSING", prefix))
            }),
            visibility!(activating),
        );

        let ad = format!("{}_ACTIVATED", prefix);
        let at = format!("{}_ACTIVE", prefix);
        let d = format!("{}_DISABLED", prefix);
        let c = format!("{}_COMPLETED", prefix);
        let activated = ad.as_str();
        let active = at.as_str();
        let deactivated = d.as_str();
        let completed = c.as_str();

        let activated = name_map.remove(activated);
        let activating = name_map.remove(active);
        let deactivated = name_map.remove(deactivated);
        let completed = name_map.remove(completed);

        let logical_index = targets.len();
        /*
        let mut gizmo = GizmoAsset::new();
        gizmo
            .sphere(Isometry3d::IDENTITY, 0.15, CRIMSON)
            .resolution(30_000 / 3);
        let handle = param.gizmo_assets.add(gizmo);
        */
        for entity in [deactivated, activating, activated, completed]
            .into_iter()
            .flatten()
        {
            insert_all_child(&mut param.commands, entity, &param.children, || {
                (
                    RuneIndex(logical_index, face_entity),
                    CollisionEventsEnabled,
                    /*Gizmo {
                        handle: handle.clone(),
                        line_config: GizmoLineConfig {
                            width: 2.,
                            ..default()
                        },
                        ..default()
                    },*/
                )
            });
        }

        let mut legging_segments: [Controller; 3] = [
            Controller::new_combined(vec![]),
            Controller::new_combined(vec![]),
            Controller::new_combined(vec![]),
        ];
        for legging_idx in 1..=3 {
            legging_segments[legging_idx - 1] = creator.create_controller(
                drain_entities_by(name_map, |name| {
                    name.starts_with(&format!("{}_LEGGING_{}", prefix, legging_idx))
                        && !name.contains("PROGRESSING")
                }),
                material!(on = {activated, completed}),
            )
        }

        targets.push(RuneVisual::new(
            Controller::new_visibility(deactivated, activating, activated, completed),
            legging_segments,
            padding_segments,
            progress_segments,
        ));
    }
    targets
}

#[derive(SystemParam)]
struct PowerRuneParam<'w, 's> {
    commands: Commands<'w, 's>,
    scene_spawner: Res<'w, SceneSpawner>,

    _gizmo_assets: ResMut<'w, Assets<GizmoAsset>>,

    power_query: Query<'w, 's, (), With<PowerRuneRoot>>,
    names: Query<'w, 's, &'static Name>,
    children: Query<'w, 's, &'static Children>,
}

fn setup_power_rune(
    events: On<SceneInstanceReady>,
    mut param: PowerRuneParam,
    mut creator: StatefulAppearanceCreator,
) {
    if !param.power_query.contains(events.entity) {
        return;
    }

    let names = param.names;
    let mut name_map = param
        .scene_spawner
        .iter_instance_entities(events.instance_id)
        .filter_map(|entity| names.get(entity).map(|n| (n.as_str(), entity)).ok())
        .fold(HashMap::new(), |mut m, (name, entity)| {
            m.insert(name, entity);
            m
        });

    if name_map.is_empty() {
        return;
    }

    let mut faces: Vec<(usize, Entity)> = name_map
        .iter()
        .filter_map(|(name, &entity)| {
            let rest = name.strip_prefix("FACE_")?;
            if rest.contains('_') {
                return None;
            }
            let index = rest.parse::<usize>().ok()?;
            Some((index, entity))
        })
        .collect();

    faces.sort_by_key(|(idx, _)| *idx);
    if faces.is_empty() {
        return;
    }

    for (index, face_entity) in faces {
        let mode = if index & 2 > 0 {
            RuneMode::Large
        } else {
            RuneMode::Small
        };

        let deactivated = name_map.remove(format!("FACE_{}_R_UNPOWERED", index).as_str());
        let activated = name_map.remove(format!("FACE_{}_R_POWERED", index).as_str());

        let mut targets =
            build_targets(index, face_entity, &mut name_map, &mut param, &mut creator);
        for target in &mut targets {
            target.apply(mode, Activation::Deactivated, &mut creator.appearance);
        }

        if targets.is_empty() {
            continue;
        }

        param.commands.entity(face_entity).insert(PowerRune::new(
            if (index & 1) > 0 {
                Team::Red
            } else {
                Team::Blue
            },
            mode,
            Controller::new_visibility(deactivated, activated, activated, activated),
            targets,
            (index & 1) > 0,
        ));
    }
}

#[derive(Default)]
pub(super) struct PowerRuneConstructorPlugin;

impl bevy::app::Plugin for PowerRuneConstructorPlugin {
    fn build(&self, app: &mut bevy::app::App) {
        app.add_observer(setup_power_rune);
    }
}
