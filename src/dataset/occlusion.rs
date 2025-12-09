use crate::Controlled;
use crate::robomaster::prelude::{Armor, ArmorComponentType};
use bevy::{
    ecs::system::{SystemParam, lifetimeless::Read},
    prelude::*,
};

#[derive(SystemParam)]
pub struct Occlusion<'w, 's> {
    child_of: Query<'w, 's, Read<ChildOf>>,
    armor: Query<'w, 's, Read<Armor>>,
    names: Query<'w, 's, Read<Name>>,
    controlled: Query<'w, 's, Entity, With<Controlled>>,
    global_transforms: Query<'w, 's, Read<GlobalTransform>>,
    ray_cast: MeshRayCast<'w, 's>,
}

enum OcclusionType {
    None,
    VehicleBody,
    Armor,
}

impl<'w, 's> Occlusion<'w, 's> {
    fn sample_occluded(
        &mut self,
        camera_pos: Vec3,
        ident: &str,
        armor_entity: Entity,
        side: &str,
        vertex_entity: Entity,
        sample_pos: Vec3,
    ) -> bool {
        let dir = camera_pos - sample_pos;
        let total_dist = dir.length();

        if total_dist < f32::EPSILON {
            return false;
        }

        let ray = Ray3d::new(sample_pos, Dir3::new(dir.normalize()).unwrap());
        let hits = self.ray_cast.cast_ray(
            ray,
            &MeshRayCastSettings {
                visibility: RayCastVisibility::VisibleInView,
                filter: &|e| -> bool {
                    if self
                        .child_of
                        .iter_ancestors(e)
                        .any(|parent| self.controlled.get(parent).is_ok())
                    {
                        return false;
                    }
                    if self.child_of.iter_ancestors(e).any(|parent| {
                        self.armor.get(parent).into_iter().any(|parent| {
                            if parent.0.identifier != ident {
                                return true;
                            }
                            let ArmorComponentType::Vertex(ref s) = parent.0.component_type else {
                                return true;
                            };
                            s != side
                        })
                    }) {
                        return true;
                    }
                    true
                },
                ..default()
            },
        );
        for (e, hit) in hits {
            println!(
                "{:?}@{:?} is occluded by body: {:?}, hit_dist: {}, total_dist: {}",
                self.names.get(armor_entity),
                side,
                self.names.get(*e),
                hit.distance,
                total_dist
            );

            let is_occluded = hit.distance < total_dist - f32::EPSILON;

            if is_occluded {
                println!(
                    "{:?} is occluded by: {:?}, hit_dist: {}, total_dist: {}",
                    self.names.get(armor_entity),
                    self.names.get(*e),
                    hit.distance,
                    total_dist
                );
                return true;
            }
        }
        false
    }

    pub fn visible(
        &mut self,
        camera_pos: Vec3,
        ident: &str,
        armor_entity: Entity,
        vertices: &[(&str, Entity, Vec<Vec3>)],
    ) -> bool {
        vertices
            .iter()
            .all(move |v| self.side_visible(camera_pos, ident, armor_entity, v))
    }

    fn side_visible(
        &mut self,
        camera_pos: Vec3,
        ident: &str,
        armor_entity: Entity,
        vertex_entity: &(&str, Entity, Vec<Vec3>),
    ) -> bool {
        let (side, vertex_entity, ref samples) = *vertex_entity;
        samples.iter().any(move |&sample| {
            !self.sample_occluded(camera_pos, ident, armor_entity, side, vertex_entity, sample)
        })
    }
}
