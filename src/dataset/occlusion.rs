use std::ops::{Add, Mul, Sub};

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
    (0..=count).map(|n| start + (step * (n as f32))).collect()
}

#[derive(SystemParam)]
pub struct Occlusion<'w, 's> {
    child_of: Query<'w, 's, Read<ChildOf>>,
    names: Query<'w, 's, Read<Name>>,
    global_transforms: Query<'w, 's, Read<GlobalTransform>>,
    raycast: MeshRayCast<'w, 's>,
    config: Res<'w, OcclusionConfig>,
}

impl<'w, 's> Occlusion<'w, 's> {
    fn sample_occluded(
        &mut self,
        camera_pos: Vec3,
        armor_entity: Entity,
        sample_pos: Vec3,
    ) -> bool {
        let dir = sample_pos - camera_pos;
        let total_dist = dir.length();

        if total_dist < f32::EPSILON {
            return false;
        }

        let ray = Ray3d::new(camera_pos, Dir3::new(dir.normalize()).unwrap());
        let hits = self.raycast.cast_ray(
            ray,
            &MeshRayCastSettings {
                visibility: RayCastVisibility::VisibleInView,
                filter: &|e| {
                    e != armor_entity
                        && !self.child_of.iter_ancestors(e).any(|parent| {
                            self.names.get(parent).is_ok_and(|v| {
                                (v.contains("ARMOR_") && v.contains("_L")) || v.ends_with("_P")
                            })
                        })
                },
                early_exit_test: &|hit| {
                    if let Ok(transform) = self.global_transforms.get(hit) {
                        return transform.translation().distance(camera_pos) < total_dist;
                    }
                    true
                },
            },
        );
        hits.iter()
            .any(|(_, hit)| total_dist - hit.distance > f32::EPSILON)
    }

    pub fn visible(
        &mut self,
        camera_pos: Vec3,
        armor_entity: Entity,
        corners_ordered: &[(Vec3, (u32, u32)); 4],
    ) -> bool {
        let mut samples = sample(
            self.config.samples_per_vertex,
            corners_ordered[0].0,
            corners_ordered[1].0,
        );
        samples.append(&mut sample(
            self.config.samples_per_vertex,
            corners_ordered[2].0,
            corners_ordered[3].0,
        ));

        let mut total_samples = 0;
        let mut visible_samples = 0;

        for &sample in &samples {
            total_samples += 1;
            if !self.sample_occluded(camera_pos, armor_entity, sample) {
                visible_samples += 1;
            }
        }

        let visibility_ratio = visible_samples as f32 / total_samples as f32;

        visibility_ratio >= self.config.visibility_threshold
    }
}
