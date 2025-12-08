use crate::robomaster::prelude::extract_vertices;
use bevy::math::Vec3;
use bevy::mesh::Mesh;
use bevy::prelude::{Component, Deref, DerefMut};

#[derive(Component, Deref, DerefMut, Clone)]
pub struct MarkerData(pub [Vec3; 4]);

pub fn extract_markers(mesh: &Mesh) -> Option<[Vec3; 4]> {
    let vertices = extract_vertices(mesh)?;
    if vertices.len() != 4 {
        panic!("Expected 4 vertices but got {}", vertices.len());
    }
    Some(vertices.as_slice().try_into().unwrap())
}
