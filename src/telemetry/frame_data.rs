use bevy::prelude::*;
use std::collections::HashMap;

use crate::robomaster::prelude::{ArmorLabel, Team};

#[derive(Clone, Debug)]
pub struct ArmorAnnotation {
    pub team: Team,
    pub label: ArmorLabel,
    pub corners: [Vec2; 4],
    pub center_3d: Vec3,
    pub occluded: bool,
}

#[derive(Clone)]
pub struct FrameData {
    pub timestamp: f64,
    pub armors: Vec<ArmorAnnotation>,
    pub poses: HashMap<String, Transform>,
}
