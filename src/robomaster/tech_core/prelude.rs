use crate::robomaster::tech_core::construct::TechCorePlugin;
use bevy::app::plugin_group;

#[allow(unused_imports)]
pub use crate::robomaster::tech_core::construct::{
    AssemblyLightProgram, BlinkRate, LightColor, LightProgram, TechCore, TechCoreFirstLightSegment,
    TechCoreLightGroup, TechCorePhase, TechCoreRoot, TechCoreStep5Lights, tech_core_state_json,
    tech_core_state_json_from_phases,
};

plugin_group! {
    #[derive(Default)]
    pub struct TechCorePlugins {
        :TechCorePlugin,
    }
}
