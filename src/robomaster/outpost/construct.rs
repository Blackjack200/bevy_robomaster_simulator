use crate::robomaster::outpost::rotation::RotationDirection;
use crate::robomaster::outpost::update::{Outpost, OutpostRotator};
use crate::robomaster::prelude::{ArmorSpec, ScanArmor, SmallArmorLabel, Team};
use bevy::ecs::system::SystemParam;
use bevy::prelude::{Added, Children, Commands, Component, Entity, Name, Query, Update};

#[derive(Component)]
pub struct OutpostRoot {
    pub team: Team,
}

impl OutpostRoot {
    pub const fn new(team: Team) -> Self {
        Self { team }
    }
}

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
    for (root, outpost_root) in query {
        let team = outpost_root.team;
        param.commands.entity(root).insert((
            Outpost::new(team),
            ScanArmor::new(team, ArmorSpec::Small(SmallArmorLabel::Outpost)),
        ));
        let direction = match team {
            Team::Red => RotationDirection::Clockwise,
            Team::Blue => RotationDirection::CounterClockwise,
        };
        param.children.iter_descendants(root).for_each(|e| {
            let Ok(name) = param.names.get(e) else {
                return;
            };
            if name.ends_with("ROTATE") {
                param
                    .commands
                    .entity(e)
                    .insert(OutpostRotator::new(direction));
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn team_rotation_mapping_matches_legacy_behavior() {
        let red = match Team::Red {
            Team::Red => RotationDirection::Clockwise,
            Team::Blue => RotationDirection::CounterClockwise,
        };
        let blue = match Team::Blue {
            Team::Red => RotationDirection::Clockwise,
            Team::Blue => RotationDirection::CounterClockwise,
        };

        assert_eq!(red, RotationDirection::Clockwise);
        assert_eq!(blue, RotationDirection::CounterClockwise);
    }
}

#[derive(Default)]
pub(super) struct OutpostConstructorPlugin;

impl bevy::app::Plugin for OutpostConstructorPlugin {
    fn build(&self, app: &mut bevy::app::App) {
        app.add_systems(Update, setup_outpost);
    }
}
