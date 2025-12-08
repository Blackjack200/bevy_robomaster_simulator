use crate::Controlled;
use bevy::{
    ecs::system::{SystemParam, lifetimeless::Read},
    prelude::*,
};

#[derive(SystemParam)]
pub struct Occlusion<'w, 's> {
    child_of: Query<'w, 's, Read<ChildOf>>,
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
        armor_entity: Entity,
        sample_pos: Vec3,
    ) -> OcclusionType {
        let dir = camera_pos - sample_pos;
        let total_dist = dir.length();

        if total_dist < f32::EPSILON {
            return OcclusionType::None;
        }

        let ray = Ray3d::new(sample_pos, Dir3::new(dir.normalize()).unwrap());
        let hits = self.ray_cast.cast_ray(
            ray,
            &MeshRayCastSettings {
                visibility: RayCastVisibility::VisibleInView,
                ..default()
            },
        );
        for (e, hit) in hits.into_iter() {
            let is_controlled = self
                .child_of
                .iter_ancestors(*e)
                .any(|parent| self.controlled.get(parent).is_ok());
            if is_controlled {
                return OcclusionType::None;
            }
            let is_armor = self.child_of.iter_ancestors(*e).any(|parent| {
                self.names.get(parent).is_ok_and(|v| {
                    v.contains("ARMOR")
                        && (v.contains("_L") || v.contains("VERTEX") || v.contains("MARKER"))
                })
            });
            if !is_armor {
                println!(
                    "{:?} is occluded by body: {:?}, hit_dist: {}, total_dist: {}",
                    self.names.get(armor_entity),
                    self.names.get(*e),
                    hit.distance,
                    total_dist
                );
                return OcclusionType::VehicleBody;
            }

            let is_occluded = hit.distance < total_dist - f32::EPSILON;

            if is_occluded {
                println!(
                    "{:?} is occluded by: {:?}, hit_dist: {}, total_dist: {}",
                    self.names.get(armor_entity),
                    self.names.get(*e),
                    hit.distance,
                    total_dist
                );
                return OcclusionType::Armor;
            }
        }
        OcclusionType::None
    }

    pub fn visible(
        &mut self,
        camera_pos: Vec3,
        armor_entity: Entity,
        vertices: &[Vec<Vec3>],
    ) -> bool {
        vertices
            .iter()
            .any(move |vertex| self.side_visible(camera_pos, armor_entity, vertex))
    }

    fn side_visible(&mut self, camera_pos: Vec3, armor_entity: Entity, samples: &[Vec3]) -> bool {
        samples.iter().any(move |sample| {
            matches!(
                self.sample_occluded(camera_pos, armor_entity, *sample),
                OcclusionType::None
            )
        })
    }
}
