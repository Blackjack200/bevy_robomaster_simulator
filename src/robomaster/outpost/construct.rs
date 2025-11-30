use crate::Armor;
use crate::robomaster::outpost::update::{Outpost, OutpostRotator};
use crate::robomaster::prelude::{ArmorLabel, ArmorType, Team};
use crate::util::bevy::insert_all_child;
use avian3d::prelude::{ColliderConstructor, ColliderConstructorHierarchy, TrimeshFlags};
use bevy::ecs::system::SystemParam;
use bevy::prelude::{Added, Children, Commands, Component, Entity, Name, Query, Update};

#[derive(Component)]
pub struct OutpostRoot(pub Team);

#[derive(SystemParam)]
struct OutpostParam<'w, 's> {
    commands: Commands<'w, 's>,
    names: Query<'w, 's, &'static Name>,
    children: Query<'w, 's, &'static Children>,
}

fn setup_outpost(
    query: Query<(Entity, &OutpostRoot), Added<OutpostRoot>>,
    mut param: OutpostParam,
) {
    for (root, &OutpostRoot(team)) in query {
        param.commands.entity(root).insert(Outpost::new(team));
        let clockwise = team == Team::Red;
        param.children.iter_descendants(root).for_each(|e| {
            let Ok(name) = param.names.get(e) else {
                return;
            };
            if name.ends_with("ROTATE") {
                param
                    .commands
                    .entity(e)
                    .insert(OutpostRotator::new(clockwise));
                return;
            }
            if name.ends_with("_P") {
                insert_all_child(&mut param.commands, e, &param.children, || {
                    Armor(team, ArmorType::Small, ArmorLabel::OutpostZeo)
                });
                return;
            }
            if name.contains("_ARMOR")
                && !name.contains("ROOT")
                && !name.contains("BASE")
                && !name.ends_with("_P")
            {
                param
                    .commands
                    .entity(e)
                    .insert(ColliderConstructorHierarchy::new(
                        ColliderConstructor::TrimeshFromMeshWithConfig(TrimeshFlags::all()),
                    ));
            }
        })
    }
}

#[derive(Default)]
pub(super) struct OutpostConstructorPlugin;

impl bevy::app::Plugin for OutpostConstructorPlugin {
    fn build(&self, app: &mut bevy::app::App) {
        app.add_systems(Update, setup_outpost);
    }
}
