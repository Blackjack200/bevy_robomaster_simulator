use crate::robomaster::tech_core::construct::TechCorePlugin;
use bevy::app::plugin_group;

#[allow(unused_imports)]
pub use crate::robomaster::tech_core::construct::{
    BlinkRate, LightColor, LightProgram, TechCore, TechCoreLightGroup, TechCorePhase, TechCoreRoot,
};

plugin_group! {
    #[derive(Default)]
    pub struct TechCorePlugins {
        :TechCorePlugin,
    }
}
