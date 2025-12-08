use crate::robomaster::outpost::update::{Outpost, OutpostRotator};
use crate::robomaster::prelude::{ArmorLabel, ArmorType, ScanArmor, Team};
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
        param.commands.entity(root).insert((
            Outpost::new(team),
            ScanArmor(team, ArmorType::Small, ArmorLabel::OutpostZeo),
        ));
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
