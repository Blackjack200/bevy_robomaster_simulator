use super::consts::{
    BLUE_LIGHT_NAMES, FIRST_LIGHT_SEGMENT_COUNT, FLOW_SEGMENT_HZ, RED_LIGHT_NAMES,
};
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
use serde_json::{Value, json};
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

    pub const fn number(self) -> u8 {
        self.index() as u8 + 1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LightColor {
    White,
    Team,
    Green,
}

impl LightColor {
    pub const fn as_str_for_team(self, team: Team) -> &'static str {
        match self {
            Self::White => "white",
            Self::Green => "green",
            Self::Team => match team {
                Team::Red => "red",
                Team::Blue => "blue",
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlinkRate {
    Hz1,
    Hz3,
}

impl BlinkRate {
    pub const fn hz(self) -> f64 {
        match self {
            Self::Hz1 => 1.0,
            Self::Hz3 => 3.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssemblyLightProgram {
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LightProgram {
    Off,
    Solid(LightColor),
    Blink { color: LightColor, rate: BlinkRate },
    Flow { color: LightColor },
    Assembly(AssemblyLightProgram),
}

impl LightProgram {
    /// Returns the active color only for programs with a uniform group color.
    /// Segmented assembly guidance is described by its active segments instead.
    pub fn active_color(self, elapsed_secs: f64) -> Option<LightColor> {
        match self {
            Self::Off => None,
            Self::Solid(color) => Some(color),
            Self::Blink { color, rate } => {
                ((elapsed_secs * rate.hz()).fract() < 0.5).then_some(color)
            }
            Self::Flow { color } => Some(color),
            Self::Assembly(AssemblyLightProgram::InProgress) => None,
            Self::Assembly(AssemblyLightProgram::Completed) => Some(LightColor::Green),
        }
    }

    fn json_value(self, team: Team) -> Value {
        match self {
            Self::Off => json!({ "mode": "off" }),
            Self::Solid(color) => json!({
                "mode": "solid",
                "color": color.as_str_for_team(team),
            }),
            Self::Blink { color, rate } => json!({
                "mode": "blink",
                "color": color.as_str_for_team(team),
                "hz": rate.hz(),
            }),
            Self::Flow { color } => json!({
                "mode": "flow",
                "color": color.as_str_for_team(team),
                "segment_hz": FLOW_SEGMENT_HZ,
            }),
            Self::Assembly(AssemblyLightProgram::InProgress) => json!({
                "mode": "step5_in_progress",
                "target_color": LightColor::Team.as_str_for_team(team),
                "energy_unit_color": LightColor::White.as_str_for_team(team),
            }),
            Self::Assembly(AssemblyLightProgram::Completed) => json!({
                "mode": "step5_completed",
                "target_color": LightColor::Green.as_str_for_team(team),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TechCoreFirstLightSegment(usize);

impl TechCoreFirstLightSegment {
    pub const MIN_NUMBER: usize = 1;
    pub const MAX_NUMBER: usize = FIRST_LIGHT_SEGMENT_COUNT;

    pub const fn from_zero_based(index: usize) -> Option<Self> {
        if index < FIRST_LIGHT_SEGMENT_COUNT {
            Some(Self(index))
        } else {
            None
        }
    }

    pub const fn from_number(number: usize) -> Option<Self> {
        if number >= Self::MIN_NUMBER && number <= Self::MAX_NUMBER {
            Some(Self(number - 1))
        } else {
            None
        }
    }

    pub fn from_angle_radians(radians: f64) -> Self {
        let normalized = radians.rem_euclid(std::f64::consts::TAU);
        let index = (normalized / std::f64::consts::TAU * FIRST_LIGHT_SEGMENT_COUNT as f64).floor()
            as usize;

        Self(index.min(FIRST_LIGHT_SEGMENT_COUNT - 1))
    }

    pub fn from_angle_degrees(degrees: f64) -> Self {
        Self::from_angle_radians(degrees.to_radians())
    }

    const fn index(self) -> usize {
        self.0
    }

    pub const fn number(self) -> usize {
        self.0 + 1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TechCoreStep5Lights {
    target: TechCoreFirstLightSegment,
    energy_unit: TechCoreFirstLightSegment,
}

impl TechCoreStep5Lights {
    pub const fn new(
        target: TechCoreFirstLightSegment,
        energy_unit: TechCoreFirstLightSegment,
    ) -> Self {
        Self {
            target,
            energy_unit,
        }
    }

    pub const fn target(self) -> TechCoreFirstLightSegment {
        self.target
    }

    pub const fn energy_unit(self) -> TechCoreFirstLightSegment {
        self.energy_unit
    }
}

impl Default for TechCoreStep5Lights {
    fn default() -> Self {
        Self {
            target: TechCoreFirstLightSegment::from_zero_based(0).unwrap(),
            energy_unit: TechCoreFirstLightSegment::from_zero_based(FIRST_LIGHT_SEGMENT_COUNT / 2)
                .unwrap(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FlowActivation {
    Segment(usize),
    All,
}

fn flow_activation(elapsed_secs: f64) -> FlowActivation {
    let elapsed_secs = elapsed_secs.max(0.0);
    let step = (elapsed_secs * FLOW_SEGMENT_HZ).floor() as usize;
    let forward_len = FIRST_LIGHT_SEGMENT_COUNT;
    let round_trip_len = forward_len * 2;

    if step < forward_len {
        FlowActivation::Segment(step)
    } else if step < round_trip_len {
        FlowActivation::Segment(round_trip_len - 1 - step)
    } else {
        FlowActivation::All
    }
}

fn flow_active_segments_json(elapsed_secs: f64) -> Value {
    fn segment_json(side: &'static str, index: usize) -> Value {
        json!({
            "side": side,
            "index": index + 1,
        })
    }

    match flow_activation(elapsed_secs) {
        FlowActivation::Segment(index) => {
            json!([segment_json("left", index), segment_json("right", index),])
        }
        FlowActivation::All => Value::Array(
            (0..FIRST_LIGHT_SEGMENT_COUNT)
                .flat_map(|index| [segment_json("left", index), segment_json("right", index)])
                .collect(),
        ),
    }
}

fn segment_pair_json(
    segment: TechCoreFirstLightSegment,
    color: &'static str,
    role: &'static str,
) -> [Value; 2] {
    [
        json!({
            "side": "left",
            "index": segment.number(),
            "color": color,
            "role": role,
        }),
        json!({
            "side": "right",
            "index": segment.number(),
            "color": color,
            "role": role,
        }),
    ]
}

fn step5_active_segments_json(
    team: Team,
    assembly: AssemblyLightProgram,
    step5_lights: TechCoreStep5Lights,
) -> Value {
    let mut segments = Vec::with_capacity(4);
    let target = step5_lights.target();

    match assembly {
        AssemblyLightProgram::InProgress => {
            segments.extend(segment_pair_json(
                target,
                LightColor::Team.as_str_for_team(team),
                "target",
            ));

            let energy_unit = step5_lights.energy_unit();
            if energy_unit != target {
                segments.extend(segment_pair_json(
                    energy_unit,
                    LightColor::White.as_str_for_team(team),
                    "energy_unit",
                ));
            }
        }
        AssemblyLightProgram::Completed => {
            segments.extend(segment_pair_json(
                target,
                LightColor::Green.as_str_for_team(team),
                "target",
            ));
        }
    }

    Value::Array(segments)
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
        use AssemblyLightProgram::{Completed, InProgress};
        use BlinkRate::{Hz1, Hz3};
        use LightColor::{Team, White};
        use LightProgram::{Assembly, Blink, Flow, Off, Solid};

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
            Self::Step5InProgress => [Assembly(InProgress), Solid(Team), Solid(Team)],
            Self::Step5Completed => [Assembly(Completed), Solid(Team), Solid(Team)],
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

    pub const fn id(self) -> u8 {
        match self {
            Self::MatchRunningIdle => 0,
            Self::DifficultySelectedArmNotReady => 1,
            Self::DifficultySelectedArmReady => 2,
            Self::Step2Completed => 3,
            Self::Step3Completed => 4,
            Self::Step4Completed => 5,
            Self::Step5InProgress => 6,
            Self::Step5Completed => 7,
            Self::ConfirmedRecovering => 8,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MatchRunningIdle => "match_running_idle",
            Self::DifficultySelectedArmNotReady => "difficulty_selected_arm_not_ready",
            Self::DifficultySelectedArmReady => "difficulty_selected_arm_ready",
            Self::Step2Completed => "step2_completed",
            Self::Step3Completed => "step3_completed",
            Self::Step4Completed => "step4_completed",
            Self::Step5InProgress => "step5_in_progress",
            Self::Step5Completed => "step5_completed",
            Self::ConfirmedRecovering => "confirmed_recovering",
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

fn team_name(team: Team) -> &'static str {
    match team {
        Team::Red => "red",
        Team::Blue => "blue",
    }
}

fn light_json_value(
    phase: TechCorePhase,
    team: Team,
    group: TechCoreLightGroup,
    elapsed_secs: f64,
    step5_lights: TechCoreStep5Lights,
) -> Value {
    let program = phase.programs()[group.index()];
    let active_color = match program {
        LightProgram::Assembly(AssemblyLightProgram::InProgress) => "mixed",
        LightProgram::Assembly(AssemblyLightProgram::Completed) => {
            LightColor::Green.as_str_for_team(team)
        }
        _ => program
            .active_color(elapsed_secs)
            .map(|color| color.as_str_for_team(team))
            .unwrap_or("off"),
    };

    let mut value = json!({
        "team": team_name(team),
        "group": group.number(),
        "program": program.json_value(team),
        "active_color": active_color,
    });

    if matches!(program, LightProgram::Flow { .. }) && group == TechCoreLightGroup::First {
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "active_segments".to_string(),
                flow_active_segments_json(elapsed_secs),
            );
        }
    }

    if let LightProgram::Assembly(assembly) = program {
        if group == TechCoreLightGroup::First {
            if let Some(object) = value.as_object_mut() {
                object.insert(
                    "active_segments".to_string(),
                    step5_active_segments_json(team, assembly, step5_lights),
                );
            }
        }
    }

    value
}

fn tech_core_phase_json_value(
    phase: TechCorePhase,
    elapsed_secs: f64,
    step5_lights: TechCoreStep5Lights,
) -> Value {
    let mut lights = Vec::with_capacity(6);
    for team in [Team::Red, Team::Blue] {
        for group in TechCoreLightGroup::ALL {
            lights.push(light_json_value(
                phase,
                team,
                group,
                elapsed_secs,
                step5_lights,
            ));
        }
    }

    json!({
        "phase": {
            "id": phase.id(),
            "name": phase.as_str(),
        },
        "lights": lights,
    })
}

pub fn tech_core_state_json_from_phases<I>(
    stamp_sec: i32,
    stamp_nanosec: u32,
    elapsed_secs: f64,
    phases: I,
) -> String
where
    I: IntoIterator<Item = TechCorePhase>,
{
    let cores = phases
        .into_iter()
        .map(|phase| {
            tech_core_phase_json_value(phase, elapsed_secs, TechCoreStep5Lights::default())
        })
        .collect::<Vec<_>>();

    json!({
        "stamp": {
            "sec": stamp_sec,
            "nanosec": stamp_nanosec,
        },
        "cores": cores,
    })
    .to_string()
}

pub fn tech_core_state_json<'a, I>(
    stamp_sec: i32,
    stamp_nanosec: u32,
    elapsed_secs: f64,
    cores: I,
) -> String
where
    I: IntoIterator<Item = &'a TechCore>,
{
    let cores = cores
        .into_iter()
        .map(|core| {
            tech_core_phase_json_value(
                core.phase(),
                core.phase_elapsed_secs(elapsed_secs),
                core.step5_lights(),
            )
        })
        .collect::<Vec<_>>();

    json!({
        "stamp": {
            "sec": stamp_sec,
            "nanosec": stamp_nanosec,
        },
        "cores": cores,
    })
    .to_string()
}

#[derive(Debug, Clone, Copy)]
struct FirstLightSet {
    whole: Option<Entity>,
    left: [Option<Entity>; FIRST_LIGHT_SEGMENT_COUNT],
    right: [Option<Entity>; FIRST_LIGHT_SEGMENT_COUNT],
}

impl FirstLightSet {
    fn new(
        whole: Option<Entity>,
        left: [Option<Entity>; FIRST_LIGHT_SEGMENT_COUNT],
        right: [Option<Entity>; FIRST_LIGHT_SEGMENT_COUNT],
    ) -> Self {
        Self { whole, left, right }
    }

    fn has_segments(&self) -> bool {
        self.left
            .iter()
            .chain(self.right.iter())
            .any(Option::is_some)
    }

    fn missing_segments(&self, prefix: &str) -> Vec<String> {
        let mut missing = Vec::new();

        for (side, segments) in [("L", &self.left), ("R", &self.right)] {
            for (index, entity) in segments.iter().enumerate() {
                if entity.is_none() {
                    missing.push(format!("{prefix}_{side}_{}", index + 1));
                }
            }
        }

        missing
    }

    fn segment_entities(&self) -> impl Iterator<Item = Entity> + '_ {
        self.left
            .iter()
            .chain(self.right.iter())
            .filter_map(|entity| *entity)
    }

    fn assign_all(
        &self,
        handle: Handle<StandardMaterial>,
        children: &Query<&Children>,
        mesh_materials: &mut Query<&mut MeshMaterial3d<StandardMaterial>>,
    ) {
        if self.has_segments() {
            for entity in self.segment_entities() {
                assign_material(entity, handle.clone(), children, mesh_materials);
            }
        } else if let Some(entity) = self.whole {
            assign_material(entity, handle, children, mesh_materials);
        }
    }

    fn assign_flow(
        &self,
        team: Team,
        color: LightColor,
        elapsed_secs: f64,
        handles: &TechCoreMaterialHandles,
        children: &Query<&Children>,
        mesh_materials: &mut Query<&mut MeshMaterial3d<StandardMaterial>>,
    ) {
        if !self.has_segments() {
            self.assign_all(handles.resolve_color(team, color), children, mesh_materials);
            return;
        }

        self.assign_all(handles.off.clone(), children, mesh_materials);
        let active_handle = handles.resolve_color(team, color);

        match flow_activation(elapsed_secs) {
            FlowActivation::Segment(index) => {
                for entity in [self.left[index], self.right[index]].into_iter().flatten() {
                    assign_material(entity, active_handle.clone(), children, mesh_materials);
                }
            }
            FlowActivation::All => {
                self.assign_all(active_handle, children, mesh_materials);
            }
        }
    }

    fn assign_segment_pair(
        &self,
        segment: TechCoreFirstLightSegment,
        handle: Handle<StandardMaterial>,
        children: &Query<&Children>,
        mesh_materials: &mut Query<&mut MeshMaterial3d<StandardMaterial>>,
    ) {
        let index = segment.index();
        for entity in [self.left[index], self.right[index]].into_iter().flatten() {
            assign_material(entity, handle.clone(), children, mesh_materials);
        }
    }

    fn assign_assembly(
        &self,
        team: Team,
        assembly: AssemblyLightProgram,
        step5_lights: TechCoreStep5Lights,
        handles: &TechCoreMaterialHandles,
        children: &Query<&Children>,
        mesh_materials: &mut Query<&mut MeshMaterial3d<StandardMaterial>>,
    ) {
        if !self.has_segments() {
            let fallback_color = match assembly {
                AssemblyLightProgram::InProgress => LightColor::Team,
                AssemblyLightProgram::Completed => LightColor::Green,
            };
            self.assign_all(
                handles.resolve_color(team, fallback_color),
                children,
                mesh_materials,
            );
            return;
        }

        self.assign_all(handles.off.clone(), children, mesh_materials);

        match assembly {
            AssemblyLightProgram::InProgress => {
                let target = step5_lights.target();
                let energy_unit = step5_lights.energy_unit();

                if energy_unit != target {
                    self.assign_segment_pair(
                        energy_unit,
                        handles.resolve_color(team, LightColor::White),
                        children,
                        mesh_materials,
                    );
                }

                self.assign_segment_pair(
                    target,
                    handles.resolve_color(team, LightColor::Team),
                    children,
                    mesh_materials,
                );
            }
            AssemblyLightProgram::Completed => {
                self.assign_segment_pair(
                    step5_lights.target(),
                    handles.resolve_color(team, LightColor::Green),
                    children,
                    mesh_materials,
                );
            }
        }
    }

    fn assign_program(
        &self,
        team: Team,
        program: LightProgram,
        elapsed_secs: f64,
        step5_lights: TechCoreStep5Lights,
        handles: &TechCoreMaterialHandles,
        children: &Query<&Children>,
        mesh_materials: &mut Query<&mut MeshMaterial3d<StandardMaterial>>,
    ) {
        match program {
            LightProgram::Flow { color } => {
                self.assign_flow(team, color, elapsed_secs, handles, children, mesh_materials);
            }
            LightProgram::Assembly(assembly) => {
                self.assign_assembly(
                    team,
                    assembly,
                    step5_lights,
                    handles,
                    children,
                    mesh_materials,
                );
            }
            _ => {
                let handle = handles.resolve(team, program, elapsed_secs);
                self.assign_all(handle, children, mesh_materials);
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct TeamCoreLights {
    team: Team,
    first: FirstLightSet,
    second: Entity,
    third: Entity,
}

impl TeamCoreLights {
    fn new(team: Team, first: FirstLightSet, second: Entity, third: Entity) -> Self {
        Self {
            team,
            first,
            second,
            third,
        }
    }
}

#[derive(Component, Debug)]
pub struct TechCore {
    phase: TechCorePhase,
    last_rendered_phase: TechCorePhase,
    phase_started_at_secs: f64,
    step5_lights: TechCoreStep5Lights,
    red: TeamCoreLights,
    blue: TeamCoreLights,
}

impl TechCore {
    fn new(red: TeamCoreLights, blue: TeamCoreLights) -> Self {
        Self {
            phase: TechCorePhase::MatchRunningIdle,
            last_rendered_phase: TechCorePhase::MatchRunningIdle,
            phase_started_at_secs: 0.0,
            step5_lights: TechCoreStep5Lights::default(),
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

    pub const fn step5_lights(&self) -> TechCoreStep5Lights {
        self.step5_lights
    }

    pub fn set_step5_lights(&mut self, step5_lights: TechCoreStep5Lights) {
        self.step5_lights = step5_lights;
    }

    pub fn set_step5_target_segment(&mut self, segment: TechCoreFirstLightSegment) {
        self.step5_lights.target = segment;
    }

    pub fn set_step5_energy_unit_segment(&mut self, segment: TechCoreFirstLightSegment) {
        self.step5_lights.energy_unit = segment;
    }

    pub fn set_step5_target_angle_radians(&mut self, radians: f64) {
        self.set_step5_target_segment(TechCoreFirstLightSegment::from_angle_radians(radians));
    }

    pub fn set_step5_energy_unit_angle_radians(&mut self, radians: f64) {
        self.set_step5_energy_unit_segment(TechCoreFirstLightSegment::from_angle_radians(radians));
    }

    pub fn set_step5_target_angle_degrees(&mut self, degrees: f64) {
        self.set_step5_target_segment(TechCoreFirstLightSegment::from_angle_degrees(degrees));
    }

    pub fn set_step5_energy_unit_angle_degrees(&mut self, degrees: f64) {
        self.set_step5_energy_unit_segment(TechCoreFirstLightSegment::from_angle_degrees(degrees));
    }

    pub fn advance_debug(&mut self) {
        self.phase = self.phase.next_debug();
    }

    fn phase_elapsed_secs(&self, elapsed_secs: f64) -> f64 {
        if self.phase == self.last_rendered_phase {
            (elapsed_secs - self.phase_started_at_secs).max(0.0)
        } else {
            0.0
        }
    }

    fn render_elapsed_secs(&mut self, elapsed_secs: f64) -> f64 {
        if self.phase != self.last_rendered_phase {
            self.last_rendered_phase = self.phase;
            self.phase_started_at_secs = elapsed_secs;
        }

        self.phase_elapsed_secs(elapsed_secs)
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

        self.resolve_color(team, color)
    }

    fn resolve_color(&self, team: Team, color: LightColor) -> Handle<StandardMaterial> {
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

fn find_first_light_set(name_map: &HashMap<String, Entity>, prefix: &str) -> FirstLightSet {
    let mut left = [None; FIRST_LIGHT_SEGMENT_COUNT];
    let mut right = [None; FIRST_LIGHT_SEGMENT_COUNT];

    for index in 0..FIRST_LIGHT_SEGMENT_COUNT {
        left[index] = name_map.get(&format!("{prefix}_L_{}", index + 1)).copied();
        right[index] = name_map.get(&format!("{prefix}_R_{}", index + 1)).copied();
    }

    FirstLightSet::new(name_map.get(prefix).copied(), left, right)
}

fn find_team_lights(
    name_map: &HashMap<String, Entity>,
    team: Team,
    names: [&str; 3],
) -> Option<TeamCoreLights> {
    let [first_name, second_name, third_name] = names;
    let first = find_first_light_set(name_map, first_name);
    let Some(second) = name_map.get(second_name).copied() else {
        warn!("TECH_CORE.glb is missing {second_name}");
        return None;
    };
    let Some(third) = name_map.get(third_name).copied() else {
        warn!("TECH_CORE.glb is missing {third_name}");
        return None;
    };

    Some(TeamCoreLights::new(team, first, second, third))
}

fn warn_incomplete_first_light_set(prefix: &str, lights: &FirstLightSet) {
    if !lights.has_segments() {
        if lights.whole.is_none() {
            warn!(
                "TECH_CORE.glb is missing {prefix} and segmented {prefix}_{{L,R}}_1..{FIRST_LIGHT_SEGMENT_COUNT}"
            );
        }
        return;
    }

    let missing = lights.missing_segments(prefix);
    if missing.is_empty() {
        return;
    }

    let preview = missing
        .iter()
        .take(6)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let suffix = if missing.len() > 6 {
        format!(" (+{} more)", missing.len() - 6)
    } else {
        String::new()
    };

    warn!("TECH_CORE.glb has incomplete {prefix} segments; missing {preview}{suffix}");
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

    let Some(red) = find_team_lights(&name_map, Team::Red, RED_LIGHT_NAMES) else {
        return;
    };
    let Some(blue) = find_team_lights(&name_map, Team::Blue, BLUE_LIGHT_NAMES) else {
        return;
    };

    warn_incomplete_first_light_set(RED_LIGHT_NAMES[0], &red.first);
    warn_incomplete_first_light_set(BLUE_LIGHT_NAMES[0], &blue.first);

    commands
        .entity(events.entity)
        .insert(TechCore::new(red, blue));
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
    mut cores: Query<&mut TechCore>,
) {
    let handles = handles.get_or_insert_with(|| TechCoreMaterialHandles::new(&mut materials));
    let elapsed_secs = time.elapsed_secs_f64();

    for mut core in &mut cores {
        let phase_elapsed_secs = core.render_elapsed_secs(elapsed_secs);
        let programs = core.phase.programs();
        let step5_lights = core.step5_lights();
        for team in core.teams() {
            for group in TechCoreLightGroup::ALL {
                let program = programs[group.index()];
                match group {
                    TechCoreLightGroup::First => team.first.assign_program(
                        team.team,
                        program,
                        phase_elapsed_secs,
                        step5_lights,
                        handles,
                        &children,
                        &mut mesh_materials,
                    ),
                    TechCoreLightGroup::Second => {
                        let handle = handles.resolve(team.team, program, phase_elapsed_secs);
                        assign_material(team.second, handle, &children, &mut mesh_materials);
                    }
                    TechCoreLightGroup::Third => {
                        let handle = handles.resolve(team.team, program, phase_elapsed_secs);
                        assign_material(team.third, handle, &children, &mut mesh_materials);
                    }
                }
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
        use AssemblyLightProgram::{Completed, InProgress};
        use BlinkRate::{Hz1, Hz3};
        use LightColor::{Team, White};
        use LightProgram::{Assembly, Blink, Flow, Off, Solid};

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
            [Assembly(InProgress), Solid(Team), Solid(Team)]
        );
        assert_eq!(
            TechCorePhase::Step5Completed.programs(),
            [Assembly(Completed), Solid(Team), Solid(Team)]
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

    #[test]
    fn tech_core_phase_ids_are_stable() {
        let ids = TechCorePhase::DEBUG_SEQUENCE.map(TechCorePhase::id);
        assert_eq!(ids, [0, 1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn light_program_resolves_blink_active_color() {
        let program = LightProgram::Blink {
            color: LightColor::White,
            rate: BlinkRate::Hz1,
        };

        assert_eq!(program.active_color(0.25), Some(LightColor::White));
        assert_eq!(program.active_color(0.75), None);
    }

    #[test]
    fn tech_core_segment_maps_angles_to_first_light_indices() {
        assert_eq!(
            TechCoreFirstLightSegment::from_angle_degrees(0.0).number(),
            1
        );
        assert_eq!(
            TechCoreFirstLightSegment::from_angle_degrees(20.0).number(),
            2
        );
        assert_eq!(
            TechCoreFirstLightSegment::from_angle_degrees(359.9).number(),
            FIRST_LIGHT_SEGMENT_COUNT
        );
        assert_eq!(
            TechCoreFirstLightSegment::from_angle_degrees(-1.0).number(),
            FIRST_LIGHT_SEGMENT_COUNT
        );
    }

    #[test]
    fn tech_core_flow_activation_runs_forward_back_then_all() {
        assert_eq!(flow_activation(0.0), FlowActivation::Segment(0));
        assert_eq!(
            flow_activation((FIRST_LIGHT_SEGMENT_COUNT as f64 - 1.0) / FLOW_SEGMENT_HZ),
            FlowActivation::Segment(FIRST_LIGHT_SEGMENT_COUNT - 1)
        );
        assert_eq!(
            flow_activation(FIRST_LIGHT_SEGMENT_COUNT as f64 / FLOW_SEGMENT_HZ),
            FlowActivation::Segment(FIRST_LIGHT_SEGMENT_COUNT - 1)
        );
        assert_eq!(
            flow_activation((FIRST_LIGHT_SEGMENT_COUNT as f64 * 2.0 - 1.0) / FLOW_SEGMENT_HZ),
            FlowActivation::Segment(0)
        );
        assert_eq!(
            flow_activation(FIRST_LIGHT_SEGMENT_COUNT as f64 * 2.0 / FLOW_SEGMENT_HZ),
            FlowActivation::All
        );
    }

    #[test]
    fn tech_core_state_json_contains_flow_segments() {
        let value: Value = serde_json::from_str(&tech_core_state_json_from_phases(
            0,
            0,
            0.0,
            [TechCorePhase::DifficultySelectedArmNotReady],
        ))
        .unwrap();
        let red_first = &value["cores"][0]["lights"][0];

        assert_eq!(red_first["program"]["mode"], "flow");
        assert_eq!(red_first["program"]["segment_hz"], FLOW_SEGMENT_HZ);
        assert_eq!(red_first["active_segments"][0]["side"], "left");
        assert_eq!(red_first["active_segments"][0]["index"], 1);
        assert_eq!(red_first["active_segments"][1]["side"], "right");
        assert_eq!(red_first["active_segments"][1]["index"], 1);

        let value: Value = serde_json::from_str(&tech_core_state_json_from_phases(
            0,
            0,
            FIRST_LIGHT_SEGMENT_COUNT as f64 * 2.0 / FLOW_SEGMENT_HZ,
            [TechCorePhase::DifficultySelectedArmNotReady],
        ))
        .unwrap();

        assert_eq!(
            value["cores"][0]["lights"][0]["active_segments"]
                .as_array()
                .unwrap()
                .len(),
            FIRST_LIGHT_SEGMENT_COUNT * 2
        );
    }

    #[test]
    fn tech_core_state_json_contains_resolved_light_state() {
        let value: Value = serde_json::from_str(&tech_core_state_json_from_phases(
            12,
            34,
            0.0,
            [TechCorePhase::Step5Completed],
        ))
        .unwrap();

        assert_eq!(value["stamp"]["sec"], 12);
        assert_eq!(value["stamp"]["nanosec"], 34);
        assert_eq!(value["cores"][0]["phase"]["id"], 7);
        assert_eq!(value["cores"][0]["phase"]["name"], "step5_completed");
        assert_eq!(value["cores"][0]["lights"][0]["team"], "red");
        assert_eq!(value["cores"][0]["lights"][0]["group"], 1);
        assert_eq!(
            value["cores"][0]["lights"][0]["program"]["mode"],
            "step5_completed"
        );
        assert_eq!(
            value["cores"][0]["lights"][0]["program"]["target_color"],
            "green"
        );
        assert_eq!(value["cores"][0]["lights"][0]["active_color"], "green");
        assert_eq!(
            value["cores"][0]["lights"][0]["active_segments"][0]["role"],
            "target"
        );
        assert_eq!(
            value["cores"][0]["lights"][0]["active_segments"][0]["color"],
            "green"
        );
        assert_eq!(value["cores"][0]["lights"][3]["team"], "blue");
        assert_eq!(
            value["cores"][0]["lights"][3]["program"]["target_color"],
            "green"
        );
    }

    #[test]
    fn tech_core_state_json_contains_step5_in_progress_segments() {
        let value: Value = serde_json::from_str(&tech_core_state_json_from_phases(
            0,
            0,
            0.0,
            [TechCorePhase::Step5InProgress],
        ))
        .unwrap();
        let red_first = &value["cores"][0]["lights"][0];

        assert_eq!(red_first["program"]["mode"], "step5_in_progress");
        assert_eq!(red_first["program"]["target_color"], "red");
        assert_eq!(red_first["program"]["energy_unit_color"], "white");
        assert_eq!(red_first["active_color"], "mixed");
        assert_eq!(red_first["active_segments"][0]["role"], "target");
        assert_eq!(red_first["active_segments"][0]["color"], "red");
        assert_eq!(red_first["active_segments"][2]["role"], "energy_unit");
        assert_eq!(red_first["active_segments"][2]["color"], "white");
    }

    #[test]
    fn tech_core_state_json_marks_blink_off_half() {
        let value: Value = serde_json::from_str(&tech_core_state_json_from_phases(
            0,
            0,
            0.75,
            [TechCorePhase::DifficultySelectedArmReady],
        ))
        .unwrap();

        assert_eq!(value["cores"][0]["lights"][0]["program"]["mode"], "blink");
        assert_eq!(value["cores"][0]["lights"][0]["program"]["hz"], 1.0);
        assert_eq!(value["cores"][0]["lights"][0]["active_color"], "off");
    }
}
