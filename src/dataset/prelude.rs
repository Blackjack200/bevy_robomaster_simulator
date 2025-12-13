use crate::capture::driver::{CaptureConfig, GpuCaptureHandler, SnapshotAsync, SnapshotSync};
use crate::dataset::occlusion::Occlusion;
use crate::dataset::writer::{ArmorColor, ArmorEntry, DatasetWriter};
use crate::robomaster::prelude::{
    ArmorLabel, ArmorOwned, ArmorRoot, ArmorType, MarkerData, Side, Team, VertexData,
};
use crate::ros2::capture::CaptureCamera;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use bevy::render::{Extract, RenderApp, RenderSystems};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemSet)]
pub enum ArmorOcclusionSystems {
    Propagate,
}

#[derive(Resource, Deref, DerefMut)]
struct DatasetHandle(pub Arc<Mutex<DatasetWriter>>);

#[derive(Resource, Deref, DerefMut)]
struct Cooldown(Mutex<Timer>);

pub struct DatasetPlugin;
impl Plugin for DatasetPlugin {
    fn build(&self, app: &mut App) {
        app.sub_app_mut(RenderApp)
            .insert_resource(DatasetHandle(Arc::new(Mutex::new(
                DatasetWriter::new("dataset").unwrap(),
            ))))
            .insert_resource(Data::default())
            .insert_resource(Cooldown(Mutex::new(Timer::from_seconds(
                0.25,
                TimerMode::Once,
            ))))
            .add_systems(
                ExtractSchedule,
                capture
                    .in_set(ArmorOcclusionSystems::Propagate)
                    .run_if(
                        |time: Res<Time>,
                         cd: Res<Cooldown>,
                         key: Extract<Res<ButtonInput<KeyCode>>>| {
                            let mut guard = cd.lock().unwrap();
                            guard.tick(time.delta());
                            if guard.is_finished() {
                                guard.reset();
                                return key.pressed(KeyCode::Digit1);
                            }
                            false
                        },
                    )
                    .before(RenderSystems::Render),
            );
    }
}

pub fn world_to_screen(
    world: Vec3,
    camera_xform: &GlobalTransform,
    projection: &Projection,
    config: &CaptureConfig,
) -> Option<(u32, u32)> {
    let clip =
        projection.get_clip_from_view() * camera_xform.to_matrix().inverse() * world.extend(1.0);

    // point is behind camera
    if clip.w <= 0.0 {
        return None;
    }

    // clip -> ndc
    let ndc = clip.xyz() / clip.w;

    // outside of screen view (x,y out of range)
    if ndc.x < -1.0 || ndc.x > 1.0 || ndc.y < -1.0 || ndc.y > 1.0 {
        return None;
    }

    // ndc -> screen
    let screen_x = (ndc.x + 1.0) * 0.5 * (config.width as f32);
    let screen_y = (1.0 - ndc.y) * 0.5 * (config.height as f32);

    Some((screen_x as u32, screen_y as u32))
}

type CornerTuple = (Vec3, (u32, u32));

fn sort_screen_points(points: [CornerTuple; 4]) -> [CornerTuple; 4] {
    let points_with_vec: Vec<(CornerTuple, Vec2)> = points
        .iter()
        .map(|&v| (v, Vec2::new(v.1.0 as f32, v.1.1 as f32)))
        .collect();

    let center = points_with_vec
        .iter()
        .map(|(_, v)| *v)
        .fold(Vec2::ZERO, |acc, v| acc + v)
        / 4.0;

    let mut sorted: Vec<(CornerTuple, Vec2, f32)> = points_with_vec
        .into_iter()
        .map(|(tuple, vec)| {
            let dir = (vec - center).normalize();
            let angle = dir.angle_to(Vec2::X).to_degrees();
            (tuple, vec, angle)
        })
        .collect();

    // 角度 descending 排序
    sorted.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());

    [sorted[0].0, sorted[3].0, sorted[2].0, sorted[1].0]
}

type ArmorScreenData = (ArmorType, ArmorLabel, ArmorColor, [(u32, u32); 4]);

#[derive(Default)]
pub struct DatasetSnapshotCreator {}
#[derive(Default, Resource, Deref, DerefMut)]
struct Data(Mutex<Vec<ArmorEntry>>);

impl GpuCaptureHandler for DatasetSnapshotCreator {
    fn captured(&self, world: &World) -> Option<Box<dyn SnapshotSync>> {
        let mut guard = world.resource::<Data>().lock().unwrap();
        let data = guard.drain(..).collect::<Vec<_>>();
        if !data.is_empty() {
            println!("annie are you ok?");
            Some(Box::new(DatasetSnapshotSync { data }))
        } else {
            None
        }
    }
}

struct DatasetSnapshotSync {
    data: Vec<ArmorEntry>,
}

impl SnapshotSync for DatasetSnapshotSync {
    fn captured(
        self: Box<Self>,
        world: &mut DeferredWorld,
        _config: &CaptureConfig,
    ) -> Box<dyn SnapshotAsync> {
        Box::new(DatasetSnapshot {
            data: self.data,
            writer: world.resource::<DatasetHandle>().0.clone(),
        })
    }
}

struct DatasetSnapshot {
    data: Vec<ArmorEntry>,
    writer: Arc<Mutex<DatasetWriter>>,
}

impl SnapshotAsync for DatasetSnapshot {
    fn captured(&mut self, width: u32, height: u32, image: &[u8]) {
        self.writer
            .lock()
            .unwrap()
            .write_entry(height, width, image, &self.data)
            .unwrap();
    }
}

fn capture(
    root_data: Extract<Query<(Entity, &ArmorOwned, &ArmorRoot)>>,
    vertex_data: Extract<Query<(&GlobalTransform, &VertexData)>>,
    marker_data: Extract<Query<(&GlobalTransform, &MarkerData)>>,
    camera: Extract<Single<(&Projection, &GlobalTransform), With<CaptureCamera>>>,
    mut occlusion: Extract<Occlusion>,
    config: Res<CaptureConfig>,
    armor_r: Res<Data>,
) {
    let mut armor_screen: HashMap<Team, Vec<ArmorScreenData>> = HashMap::new();
    let (projection, camera_global_transform) = **camera;
    let camera_pos = camera_global_transform.translation();

    for (vertex_entity, &ArmorOwned(ref ident, team, typ, label), root) in root_data.iter() {
        let all_in_frustum = |global_transform: &GlobalTransform,
                              unmapped: &[Vec3]|
         -> Option<Vec<(Vec3, (u32, u32))>> {
            let mut mapped = Vec::with_capacity(unmapped.len());
            for elem in unmapped {
                let global = global_transform.transform_point(*elem);
                let pos = world_to_screen(global, camera_global_transform, projection, &config)?;
                mapped.push((global, pos))
            }
            Some(mapped)
        };
        let marker = root.marker();
        let vertices = [root.vertex(Side::Left), root.vertex(Side::Right)];
        let mut vert = Vec::with_capacity(vertices.len());
        for vertex in vertices {
            let (tf, VertexData(side, vertices)) = vertex_data.get(vertex).unwrap();
            let Some(vertices) = all_in_frustum(tf, vertices.as_slice()) else {
                continue;
            };
            vert.push((
                side,
                vertex,
                vertices.into_iter().map(|v| v.0).collect::<Vec<_>>(),
            ));
        }
        if vert.len() != vertices.len() {
            continue;
        }
        let (tf, MarkerData(markers)) = marker_data.get(marker).unwrap();
        let Some(markers) = all_in_frustum(tf, markers) else {
            continue;
        };
        let marker_ordered = sort_screen_points(markers.as_slice().try_into().unwrap());
        if !occlusion.visible(
            camera_pos,
            camera_global_transform.forward().as_vec3(),
            &marker_ordered,
            ident.identifier.as_str(),
            vertex_entity,
            vert.as_slice(),
        ) {
            continue;
        }
        armor_screen.entry(team).or_insert(default()).push((
            typ,
            label,
            match team {
                Team::Red => ArmorColor::Red,
                Team::Blue => ArmorColor::Blue,
            },
            marker_ordered.map(|v| v.1),
        ));
    }
    for (team, armor_screen) in armor_screen.iter() {
        println!(
            "Infantry from team {:?} armor count: {}",
            team,
            armor_screen.len()
        );
    }
    let mut rr = armor_r.lock().unwrap();
    armor_screen.drain().for_each(|(_, n)| {
        for (typ, label, color, pos) in n {
            rr.push(ArmorEntry {
                color,
                typ,
                label,
                points: pos.map(|v| {
                    Vec2::new(
                        (v.0 as f32) / (config.width as f32),
                        (v.1 as f32) / (config.height as f32),
                    )
                }),
            });
        }
    });
}
