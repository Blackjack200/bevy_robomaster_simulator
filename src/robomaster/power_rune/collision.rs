use crate::robomaster::power_rune::common::RuneAction;
use crate::robomaster::power_rune::rune::PowerRune;
use crate::robomaster::power_rune::state::MechanismState;
use crate::robomaster::visibility::StatefulAppearance;
use avian3d::prelude::{CollisionEnd, CollisionEventsEnabled};
use bevy::prelude::{ChildOf, Commands, Component, Entity, EntityEvent, On, Query, With};
use rand::Rng;

#[derive(Component)]
#[require(CollisionEventsEnabled)]
pub struct Projectile;

#[derive(Component, Clone)]
pub struct RuneIndex(pub usize, pub Entity);

pub struct HitResult {
    pub accurate: bool,
    pub change_state: bool,
}

#[derive(EntityEvent)]
pub struct RuneActivated {
    #[event_target]
    pub rune: Entity,
}

#[derive(EntityEvent)]
pub struct RuneHit {
    #[event_target]
    pub rune: Entity,
    pub result: HitResult,
}

fn handle_rune_collision(
    event: On<CollisionEnd>,
    mut commands: Commands,
    mut runes: Query<&mut PowerRune>,
    targets: Query<&RuneIndex>,
    projectiles: Query<Entity, With<Projectile>>,
    child_of: Query<&ChildOf>,
    mut appearance: StatefulAppearance,
) {
    if event.body1.is_none() || event.body2.is_none() {
        return;
    }
    let projectile = event.body1.unwrap();
    let Ok(projectile_entity) = projectiles.get(projectile) else {
        return;
    };
    for ancestor in child_of.iter_ancestors(event.collider2) {
        let Ok(&RuneIndex(index, rune_ent)) = targets.get(ancestor) else {
            return;
        };
        let Ok(mut rune) = runes.get_mut(rune_ent) else {
            return;
        };

        let mut rng = rand::rng();

        let result = rune.on_target_hit(index, &mut rng, &mut appearance);
        match rune.state {
            MechanismState::Inactive { .. } => {
                commands.trigger(RuneHit {
                    rune: rune_ent,
                    result,
                });
            }
            MechanismState::Activating(_) => {
                // Disable collision events for this projectile so it only counts once
                commands
                    .entity(projectile_entity)
                    .remove::<CollisionEventsEnabled>();
                commands.trigger(RuneHit {
                    rune: rune_ent,
                    result,
                });
            }
            MechanismState::Activated { .. } => {
                // Disable collision events for this projectile so it only counts once
                commands
                    .entity(projectile_entity)
                    .remove::<CollisionEventsEnabled>();
                if result.change_state {
                    commands.trigger(RuneActivated { rune: rune_ent });
                } else {
                    commands.trigger(RuneHit {
                        rune: rune_ent,
                        result,
                    });
                }
            }
            MechanismState::Failed { .. } => {
                commands.trigger(RuneHit {
                    rune: rune_ent,
                    result,
                });
            }
        }
    }
}

impl PowerRune {
    fn on_target_hit(
        &mut self,
        target_index: usize,
        rng: &mut impl Rng,
        appearance: &mut StatefulAppearance,
    ) -> HitResult {
        match &mut self.state {
            MechanismState::Activating(state) => {
                let action = state.on_hit(target_index);
                let Some(action) = action else {
                    return HitResult {
                        accurate: false,
                        change_state: false,
                    };
                };
                let change_state = matches!(
                    action,
                    RuneAction::PartialActivate(_) | RuneAction::FullActivate(_)
                );
                self.handle_action(rng, action, appearance);
                HitResult {
                    accurate: true,
                    change_state,
                }
            }
            _ => HitResult {
                accurate: true,
                change_state: false,
            },
        }
    }
}

#[derive(Default)]
pub(super) struct PowerRuneCollisionPlugin;

impl bevy::app::Plugin for PowerRuneCollisionPlugin {
    fn build(&self, app: &mut bevy::app::App) {
        app.add_observer(handle_rune_collision);
    }
}
