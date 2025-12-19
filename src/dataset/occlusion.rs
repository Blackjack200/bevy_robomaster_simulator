use crate::Controlled;
use crate::robomaster::prelude::{ArmorOwned, LightStrip, Side, VertexData};
use bevy::{
    ecs::system::{SystemParam, lifetimeless::Read},
    prelude::*,
};

#[derive(SystemParam)]
pub struct Occlusion<'w, 's> {
    child_of: Query<'w, 's, Read<ChildOf>>,
    armor: Query<'w, 's, Read<ArmorOwned>>,
    vertex: Query<'w, 's, Read<VertexData>>,
    light_strip: Query<'w, 's, Read<LightStrip>>,
    names: Query<'w, 's, Read<Name>>,
    controlled: Query<'w, 's, Entity, With<Controlled>>,
    global_transforms: Query<'w, 's, Read<GlobalTransform>>,
    ray_cast: MeshRayCast<'w, 's>,
}

enum OcclusionType {
    None,
    Tolerated,
    Untolerated,
}

impl<'w, 's> Occlusion<'w, 's> {
    fn sample_occluded(
        &mut self,
        camera_pos: Vec3,
        ident: &str,
        armor_entity: Entity,
        side: &Side,
        _vertex_entity: Entity,
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
                filter: &|e| -> bool {
                    if self
                        .child_of
                        .iter_ancestors(e)
                        .any(|parent| self.controlled.get(parent).is_ok())
                    {
                        return false;
                    }
                    let is_vertex = self.child_of.iter_ancestors(e).any(|parent| {
                        let Ok(parent) = self.vertex.get(parent) else {
                            return false;
                        };
                        parent.0 != *side
                    });
                    if is_vertex {
                        return false;
                    }
                    let is_self = self
                        .child_of
                        .iter_ancestors(e)
                        .any(|parent| parent == armor_entity);
                    if is_self {
                        let is_light_strip_other_side =
                            self.child_of.iter_ancestors(e).any(|parent| {
                                let Ok(parent) = self.light_strip.get(parent) else {
                                    return false;
                                };
                                parent.0 != *side
                            });
                        return is_light_strip_other_side;
                    }
                    true
                },
                ..default()
            },
        );
        'h: for &(e, ref hit) in hits {
            'g: for ancestor in self.child_of.iter_ancestors(e) {
                let Ok(ancestor) = self.light_strip.get(ancestor) else {
                    continue 'g;
                };
                println!("{:?}!={:?}", ancestor.0, *side);
                if ancestor.0 != *side {
                    println!(
                        "{:?}@{:?} is occluded by light_strip: {:?}, hit_dist: {}, total_dist: {}",
                        self.names.get(armor_entity),
                        side,
                        self.names.get(e),
                        hit.distance,
                        total_dist
                    );
                    //untolerated
                    return OcclusionType::Untolerated;
                }
            }
            println!(
                "{:?}@{:?} is occluded by body: {:?}, hit_dist: {}, total_dist: {}",
                self.names.get(armor_entity),
                side,
                self.names.get(e),
                hit.distance,
                total_dist
            );

            let is_occluded = hit.distance < total_dist - f32::EPSILON;

            if is_occluded {
                println!(
                    "{:?} is occluded by: {:?}, hit_dist: {}, total_dist: {}",
                    self.names.get(armor_entity),
                    self.names.get(e),
                    hit.distance,
                    total_dist
                );
                return OcclusionType::Tolerated;
            }
        }
        OcclusionType::None
    }

    pub fn visible(
        &mut self,
        camera_pos: Vec3,
        _forward: Vec3,
        _markers: &[(Vec3, (u32, u32)); 4],
        ident: &str,
        armor_entity: Entity,
        vertices: &[(&Side, Entity, Vec<Vec3>)],
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
        vertex_entity: &(&Side, Entity, Vec<Vec3>),
    ) -> bool {
        let (side, vertex_entity, ref samples) = *vertex_entity;
        let iter = samples.iter().map(move |&sample| {
            self.sample_occluded(camera_pos, ident, armor_entity, side, vertex_entity, sample)
        });
        let mut visible = false;
        for result in iter {
            match result {
                OcclusionType::None => {
                    visible = true;
                }
                OcclusionType::Tolerated => {}
                OcclusionType::Untolerated => {
                    return false;
                }
            }
        }
        visible
    }
}
