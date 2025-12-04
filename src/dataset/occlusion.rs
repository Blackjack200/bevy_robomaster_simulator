use std::ops::{Add, Mul, Sub};

use crate::Controlled;
use bevy::{
    ecs::system::{SystemParam, lifetimeless::Read},
    prelude::*,
};

#[derive(Resource)]
pub struct OcclusionConfig {
    /// 每个边之间额外采样的点数
    pub samples_per_edge: usize,
    /// 灯条宽度方向的采样点数（每侧）
    pub samples_per_width: usize,
    /// 可见性阈值：至少多少比例的射线未被遮挡才算可见
    pub visibility_threshold: f32,
    /// 车体遮挡容忍度：允许多少比例的点被车体遮挡
    pub body_occlusion_tolerance: f32,
}

impl Default for OcclusionConfig {
    fn default() -> Self {
        Self {
            samples_per_edge: 5,
            samples_per_width: 2, // 中心 + 每侧2个点 = 5个点
            visibility_threshold: 0.5,
            body_occlusion_tolerance: 0.25,
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
        corners_ordered: &[(Vec3, (u32, u32)); 4],
        light_bar_width_inner: f32,
        light_bar_width_outer: f32,
    ) -> bool {
        let v1 = corners_ordered[1].0 - corners_ordered[0].0;
        let v2 = corners_ordered[3].0 - corners_ordered[0].0;
        let armor_normal = v1.cross(v2).normalize();
        // 左边灯条：左上[0] -> 左下[1]
        let left_samples = self.sample_light_bar(
            corners_ordered[0].0,
            corners_ordered[1].0,
            armor_normal,
            light_bar_width_inner,
            light_bar_width_outer,
        );

        // 右边灯条：右上[2] -> 右下[3]
        let right_samples = self.sample_light_bar(
            corners_ordered[2].0,
            corners_ordered[3].0,
            -armor_normal,
            light_bar_width_inner,
            light_bar_width_outer,
        );

        let left_visible = self.bar_visible(camera_pos, armor_entity, left_samples);
        let right_visible = self.bar_visible(camera_pos, armor_entity, right_samples);

        println!(
            "Left bar: {}, Right bar: {}",
            if left_visible { "VISIBLE" } else { "OCCLUDED" },
            if right_visible { "VISIBLE" } else { "OCCLUDED" }
        );

        left_visible && right_visible
    }

    fn bar_visible(
        &mut self,
        camera_pos: Vec3,
        armor_entity: Entity,
        samples: Vec<Vec<Vec3>>,
    ) -> bool {
        let mut pass = 0;
        let l = samples.len();
        for sample in samples {
            if self.side_visible(camera_pos, armor_entity, sample)
                > self.config.visibility_threshold
            {
                pass += 1;
            }
        }
        let confidence = pass as f32 / l as f32;
        println!("bar confidence={:.2}", confidence);
        confidence > self.config.visibility_threshold
    }

    fn side_visible(&mut self, camera_pos: Vec3, armor_entity: Entity, samples: Vec<Vec3>) -> f32 {
        let mut total_samples = 0;
        let mut visible_samples = 0;
        let mut body_occluded_samples = 0;

        for sample in samples {
            total_samples += 1;
            match self.sample_occluded(camera_pos, armor_entity, sample) {
                OcclusionType::None => {
                    visible_samples += 1;
                }
                OcclusionType::VehicleBody => {
                    return 0.0;
                }
                OcclusionType::Armor => {
                    // 被其他装甲板遮挡，不计入visible
                    body_occluded_samples += 1;
                }
            }
        }

        let visibility_ratio = visible_samples as f32 / total_samples as f32;
        let body_occlusion_ratio = body_occluded_samples as f32 / total_samples as f32;

        println!(
            "visibility={:.2}, body_occlusion={:.2}",
            visibility_ratio, body_occlusion_ratio
        );

        // 如果超过一半被车体遮挡，判定为不可见
        if body_occlusion_ratio > self.config.body_occlusion_tolerance {
            return 0.0;
        }

        visibility_ratio
    }

    fn sample_light_bar(
        &self,
        start: Vec3,
        end: Vec3,
        armor_normal: Vec3,
        light_bar_width_inner: f32,
        light_bar_width_outer: f32,
    ) -> Vec<Vec<Vec3>> {
        let edge_dir = (end - start).normalize();
        let width_inner = armor_normal.cross(edge_dir).normalize();

        let s = self.config.samples_per_width;
        let starts_inner = sample(s, start, start + width_inner * light_bar_width_inner);
        let ends_inner = sample(s, end, end + width_inner * light_bar_width_inner);
        let width_inner_samples: Vec<Vec<Vec3>> = starts_inner
            .into_iter()
            .zip(ends_inner)
            .map(|(start, end)| sample(self.config.samples_per_edge, start, end))
            .collect();

        let starts_outer = sample(s, start, start - width_inner * light_bar_width_outer);
        let ends_outer = sample(s, end, end - width_inner * light_bar_width_outer);
        starts_outer
            .into_iter()
            .zip(ends_outer)
            .fold(width_inner_samples, |mut w, (start, end)| {
                w.push(sample(self.config.samples_per_edge, start, end));
                w
            })
    }
}
