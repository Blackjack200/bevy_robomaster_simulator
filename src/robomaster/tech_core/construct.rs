use super::consts::{BLUE_LIGHT_NAMES, FLOW_DUTY, FLOW_HZ, RED_LIGHT_NAMES};
use crate::robomaster::common::Team;
use bevy::app::{App, Update};
use bevy::color::LinearRgba;
use bevy::ecs::system::Local;
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::prelude::{
    Assets, ButtonInput, Children, Color, Commands, Component, Entity, Handle, KeyCode, Name, On,
    Plugin, Query, Res, ResMut, SceneSpawner, Time, With, info, warn,
};
use bevy::scene::SceneInstanceReady;
use std::collections::HashMap;

#[derive(Component, Debug)]
pub struct TechCoreRoot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TechCoreLightGroup {
    First,
    Second,
    Third,
}

impl TechCoreLightGroup {
    pub const ALL: [Self; 3] = [Self::First, Self::Second, Self::Third];

    const fn index(self) -> usize {
        match self {
            Self::First => 0,
            Self::Second => 1,
            Self::Third => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LightColor {
    White,
    Team,
    Green,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlinkRate {
    Hz1,
    Hz3,
}

impl BlinkRate {
    const fn hz(self) -> f64 {
        match self {
            Self::Hz1 => 1.0,
            Self::Hz3 => 3.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LightProgram {
    Off,
    Solid(LightColor),
    Blink { color: LightColor, rate: BlinkRate },
    Flow { color: LightColor },
}

impl LightProgram {
    fn active_color(self, elapsed_secs: f64) -> Option<LightColor> {
        match self {
            Self::Off => None,
            Self::Solid(color) => Some(color),
            Self::Blink { color, rate } => {
                ((elapsed_secs * rate.hz()).fract() < 0.5).then_some(color)
            }
            Self::Flow { color } => ((elapsed_secs * FLOW_HZ).fract() < FLOW_DUTY).then_some(color),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TechCorePhase {
    MatchRunningIdle,
    DifficultySelectedArmNotReady,
    DifficultySelectedArmReady,
    Step2Completed,
    Step3Completed,
    Step4Completed,
    Step5InProgress,
    Step5Completed,
    ConfirmedRecovering,
}

impl TechCorePhase {
    pub const DEBUG_SEQUENCE: [Self; 9] = [
        Self::MatchRunningIdle,
        Self::DifficultySelectedArmNotReady,
        Self::DifficultySelectedArmReady,
        Self::Step2Completed,
        Self::Step3Completed,
        Self::Step4Completed,
        Self::Step5InProgress,
        Self::Step5Completed,
        Self::ConfirmedRecovering,
    ];

    pub const fn programs(self) -> [LightProgram; 3] {
        use BlinkRate::{Hz1, Hz3};
        use LightColor::{Green, Team, White};
        use LightProgram::{Blink, Flow, Off, Solid};

        match self {
            Self::MatchRunningIdle => [Off, Solid(Team), Solid(Team)],
            Self::DifficultySelectedArmNotReady => {
                [Flow { color: White }, Solid(Team), Solid(Team)]
            }
            Self::DifficultySelectedArmReady => [
                Blink {
                    color: White,
                    rate: Hz1,
                },
                Blink {
                    color: Team,
                    rate: Hz1,
                },
                Blink {
                    color: Team,
                    rate: Hz1,
                },
            ],
            Self::Step2Completed => [
                Blink {
                    color: White,
                    rate: Hz1,
                },
                Blink {
                    color: Team,
                    rate: Hz1,
                },
                Blink {
                    color: Team,
                    rate: Hz3,
                },
            ],
            Self::Step3Completed => [
                Blink {
                    color: White,
                    rate: Hz1,
                },
                Blink {
                    color: Team,
                    rate: Hz3,
                },
                Blink {
                    color: Team,
                    rate: Hz3,
                },
            ],
            Self::Step4Completed => [
                Blink {
                    color: White,
                    rate: Hz1,
                },
                Solid(Team),
                Solid(Team),
            ],
            Self::Step5InProgress => [Solid(Team), Solid(Team), Solid(Team)],
            Self::Step5Completed => [Solid(Green), Solid(Team), Solid(Team)],
            Self::ConfirmedRecovering => [
                Blink {
                    color: White,
                    rate: Hz3,
                },
                Blink {
                    color: Team,
                    rate: Hz3,
                },
                Blink {
                    color: Team,
                    rate: Hz3,
                },
            ],
        }
    }

    pub fn next_debug(self) -> Self {
        let index = Self::DEBUG_SEQUENCE
            .iter()
            .position(|phase| *phase == self)
            .unwrap_or(0);
        Self::DEBUG_SEQUENCE[(index + 1) % Self::DEBUG_SEQUENCE.len()]
    }
}

#[derive(Debug, Clone, Copy)]
struct TeamCoreLights {
    team: Team,
    groups: [Entity; 3],
}

impl TeamCoreLights {
    fn new(team: Team, groups: [Entity; 3]) -> Self {
        Self { team, groups }
    }

    fn entity(self, group: TechCoreLightGroup) -> Entity {
        self.groups[group.index()]
    }
}

#[derive(Component, Debug)]
pub struct TechCore {
    phase: TechCorePhase,
    red: TeamCoreLights,
    blue: TeamCoreLights,
}

impl TechCore {
    fn new(red: TeamCoreLights, blue: TeamCoreLights) -> Self {
        Self {
            phase: TechCorePhase::MatchRunningIdle,
            red,
            blue,
        }
    }

    pub fn phase(&self) -> TechCorePhase {
        self.phase
    }

    pub fn set_phase(&mut self, phase: TechCorePhase) {
        self.phase = phase;
    }

    pub fn advance_debug(&mut self) {
        self.phase = self.phase.next_debug();
    }

    fn teams(&self) -> [TeamCoreLights; 2] {
        [self.red, self.blue]
    }
}

#[derive(Clone)]
struct TechCoreMaterialHandles {
    off: Handle<StandardMaterial>,
    white: Handle<StandardMaterial>,
    red: Handle<StandardMaterial>,
    blue: Handle<StandardMaterial>,
    green: Handle<StandardMaterial>,
}

impl TechCoreMaterialHandles {
    fn new(materials: &mut Assets<StandardMaterial>) -> Self {
        Self {
            off: materials.add(material(0.02, 0.02, 0.02, 0.0)),
            white: materials.add(material(1.0, 1.0, 1.0, 1.5)),
            red: materials.add(material(1.0, 0.0, 0.0, 1.8)),
            blue: materials.add(material(0.0, 0.12, 1.0, 1.8)),
            green: materials.add(material(0.0, 1.0, 0.18, 1.8)),
        }
    }

    fn resolve(
        &self,
        team: Team,
        program: LightProgram,
        elapsed_secs: f64,
    ) -> Handle<StandardMaterial> {
        let Some(color) = program.active_color(elapsed_secs) else {
            return self.off.clone();
        };

        match color {
            LightColor::White => self.white.clone(),
            LightColor::Green => self.green.clone(),
            LightColor::Team => match team {
                Team::Red => self.red.clone(),
                Team::Blue => self.blue.clone(),
            },
        }
    }
}

fn material(red: f32, green: f32, blue: f32, emissive_strength: f32) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::srgb(red, green, blue),
        emissive: LinearRgba::new(
            red * emissive_strength,
            green * emissive_strength,
            blue * emissive_strength,
            1.0,
        ),
        emissive_exposure_weight: -1.0,
        ..Default::default()
    }
}

fn find_light_group(name_map: &HashMap<String, Entity>, names: [&str; 3]) -> Option<[Entity; 3]> {
    let [first, second, third] = names;
    Some([
        *name_map.get(first)?,
        *name_map.get(second)?,
        *name_map.get(third)?,
    ])
}

fn setup_tech_core(
    events: On<SceneInstanceReady>,
    mut commands: Commands,
    scene_spawner: Res<SceneSpawner>,
    roots: Query<(), With<TechCoreRoot>>,
    names: Query<&Name>,
) {
    if !roots.contains(events.entity) {
        return;
    }

    let name_map = scene_spawner
        .iter_instance_entities(events.instance_id)
        .filter_map(|entity| {
            names
                .get(entity)
                .map(|name| (name.to_string(), entity))
                .ok()
        })
        .collect::<HashMap<_, _>>();

    let Some(red) = find_light_group(&name_map, RED_LIGHT_NAMES) else {
        warn!("TECH_CORE.glb is missing one of {:?}", RED_LIGHT_NAMES);
        return;
    };
    let Some(blue) = find_light_group(&name_map, BLUE_LIGHT_NAMES) else {
        warn!("TECH_CORE.glb is missing one of {:?}", BLUE_LIGHT_NAMES);
        return;
    };

    commands.entity(events.entity).insert(TechCore::new(
        TeamCoreLights::new(Team::Red, red),
        TeamCoreLights::new(Team::Blue, blue),
    ));
    info!("Tech core lights bound");
}

fn assign_material(
    root: Entity,
    handle: Handle<StandardMaterial>,
    children: &Query<&Children>,
    mesh_materials: &mut Query<&mut MeshMaterial3d<StandardMaterial>>,
) {
    if let Ok(mut mesh_material) = mesh_materials.get_mut(root) {
        mesh_material.0 = handle.clone();
    }

    for child in children.iter_descendants(root) {
        if let Ok(mut mesh_material) = mesh_materials.get_mut(child) {
            mesh_material.0 = handle.clone();
        }
    }
}

fn update_tech_core_lights(
    time: Res<Time>,
    mut handles: Local<Option<TechCoreMaterialHandles>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    children: Query<&Children>,
    mut mesh_materials: Query<&mut MeshMaterial3d<StandardMaterial>>,
    cores: Query<&TechCore>,
) {
    let handles = handles.get_or_insert_with(|| TechCoreMaterialHandles::new(&mut materials));
    let elapsed_secs = time.elapsed_secs_f64();

    for core in &cores {
        let programs = core.phase.programs();
        for team in core.teams() {
            for group in TechCoreLightGroup::ALL {
                let program = programs[group.index()];
                let handle = handles.resolve(team.team, program, elapsed_secs);
                assign_material(team.entity(group), handle, &children, &mut mesh_materials);
            }
        }
    }
}

fn debug_cycle_tech_core_phase(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut cores: Query<&mut TechCore>,
) {
    if !(keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight))
        || !keyboard.just_pressed(KeyCode::KeyC)
    {
        return;
    }

    for mut core in &mut cores {
        core.advance_debug();
        info!("Tech core phase: {:?}", core.phase());
    }
}

#[derive(Default)]
pub(super) struct TechCorePlugin;

impl Plugin for TechCorePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(setup_tech_core).add_systems(
            Update,
            (debug_cycle_tech_core_phase, update_tech_core_lights),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tech_core_phase_programs_match_spec() {
        use BlinkRate::{Hz1, Hz3};
        use LightColor::{Green, Team, White};
        use LightProgram::{Blink, Flow, Off, Solid};

        assert_eq!(
            TechCorePhase::MatchRunningIdle.programs(),
            [Off, Solid(Team), Solid(Team)]
        );
        assert_eq!(
            TechCorePhase::DifficultySelectedArmNotReady.programs(),
            [Flow { color: White }, Solid(Team), Solid(Team)]
        );
        assert_eq!(
            TechCorePhase::DifficultySelectedArmReady.programs(),
            [
                Blink {
                    color: White,
                    rate: Hz1
                },
                Blink {
                    color: Team,
                    rate: Hz1
                },
                Blink {
                    color: Team,
                    rate: Hz1
                },
            ]
        );
        assert_eq!(
            TechCorePhase::Step2Completed.programs(),
            [
                Blink {
                    color: White,
                    rate: Hz1
                },
                Blink {
                    color: Team,
                    rate: Hz1
                },
                Blink {
                    color: Team,
                    rate: Hz3
                },
            ]
        );
        assert_eq!(
            TechCorePhase::Step3Completed.programs(),
            [
                Blink {
                    color: White,
                    rate: Hz1
                },
                Blink {
                    color: Team,
                    rate: Hz3
                },
                Blink {
                    color: Team,
                    rate: Hz3
                },
            ]
        );
        assert_eq!(
            TechCorePhase::Step4Completed.programs(),
            [
                Blink {
                    color: White,
                    rate: Hz1
                },
                Solid(Team),
                Solid(Team),
            ]
        );
        assert_eq!(
            TechCorePhase::Step5InProgress.programs(),
            [Solid(Team), Solid(Team), Solid(Team)]
        );
        assert_eq!(
            TechCorePhase::Step5Completed.programs(),
            [Solid(Green), Solid(Team), Solid(Team)]
        );
        assert_eq!(
            TechCorePhase::ConfirmedRecovering.programs(),
            [
                Blink {
                    color: White,
                    rate: Hz3
                },
                Blink {
                    color: Team,
                    rate: Hz3
                },
                Blink {
                    color: Team,
                    rate: Hz3
                },
            ]
        );
    }

    #[test]
    fn tech_core_debug_sequence_wraps() {
        let mut phase = TechCorePhase::MatchRunningIdle;
        for expected in TechCorePhase::DEBUG_SEQUENCE.into_iter().skip(1) {
            phase = phase.next_debug();
            assert_eq!(phase, expected);
        }

        assert_eq!(phase.next_debug(), TechCorePhase::MatchRunningIdle);
    }
}
