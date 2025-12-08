use crate::robomaster::common::Team;
use crate::robomaster::prelude::{ArmorLabel, ArmorType};
use bevy::prelude::Component;

#[derive(Component, Clone)]
pub struct Armor(pub Team, pub ArmorType, pub ArmorLabel);
