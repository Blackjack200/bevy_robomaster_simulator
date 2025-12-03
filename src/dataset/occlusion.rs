use std::ops::{Add, Mul, Sub};

use crate::Controlled;
use bevy::{
    ecs::system::{SystemParam, lifetimeless::Read},
    prelude::*,
};

#[derive(Resource)]
pub struct OcclusionConfig {
    /// 每个边之间额外采样的点数
    pub samples_per_vertex: usize,
    /// 可见性阈值：至少多少比例的射线未被遮挡才算可见
    pub visibility_threshold: f32,
}

impl Default for OcclusionConfig {
    fn default() -> Self {
        Self {
            samples_per_vertex: 8,
            visibility_threshold: 0.5,
        }
    }
}

fn sample<T: Copy + Add<T, Output = T> + Mul<f32, Output = T> + Sub<T, Output = T>>(
    count: usize,
    start: T,
    end: T,
) -> Vec<T> {
    if count == 0 {
        return vec![];
    }
    let step = (end - start) * (1.0 / count as f32);
    (0..count).map(|n| start + (step * (n as f32))).collect()
}

#[derive(SystemParam)]
pub struct Occlusion<'w, 's> {
    child_of: Query<'w, 's, Read<ChildOf>>,
    names: Query<'w, 's, Read<Name>>,
    controlled: Query<'w, 's, Entity, With<Controlled>>,
    global_transforms: Query<'w, 's, Read<GlobalTransform>>,
    ray_cast: MeshRayCast<'w, 's>,
    config: Res<'w, OcclusionConfig>,
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
            let is_armor = self
                .child_of
                .iter_ancestors(*e)
                .any(|parent| self.names.get(parent).is_ok_and(|v| v.contains("ARMOR_")));
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

            let is_occluded = hit.distance > 0.00001 && hit.distance < total_dist - f32::EPSILON;

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
        corners_ordered: &[(Vec3, (u32, u32)); 4],
    ) -> bool {
        self.side_visible(
            camera_pos,
            armor_entity,
            sample(
                self.config.samples_per_vertex,
                corners_ordered[0].0,
                corners_ordered[1].0,
            ),
        ) && self.side_visible(
            camera_pos,
            armor_entity,
            sample(
                self.config.samples_per_vertex,
                corners_ordered[2].0,
                corners_ordered[3].0,
            ),
        )
    }

    fn side_visible(&mut self, camera_pos: Vec3, armor_entity: Entity, samples: Vec<Vec3>) -> bool {
        let mut total_samples = 0;
        let mut visible_samples = 0;

        for sample in samples {
            total_samples += 1;
            match self.sample_occluded(camera_pos, armor_entity, sample) {
                OcclusionType::None => {
                    visible_samples += 1;
                }
                OcclusionType::VehicleBody => {
                    return false;
                }
                OcclusionType::Armor => {}
            }
        }

        let visibility_ratio = visible_samples as f32 / total_samples as f32;

        visibility_ratio >= self.config.visibility_threshold
    }
}
