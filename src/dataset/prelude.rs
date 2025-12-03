use crate::Armor;
use crate::capture::driver::{CaptureConfig, GpuCaptureHandler, SnapshotAsync, SnapshotSync};
use crate::dataset::occlusion::{Occlusion, OcclusionConfig};
use crate::dataset::writer::{ArmorColor, ArmorEntry, DatasetWriter};
use crate::robomaster::prelude::{ArmorLabel, ArmorType, Team};
use crate::ros2::capture::CaptureCamera;
use bevy::ecs::world::DeferredWorld;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::render::sync_world::SyncToRenderWorld;
use bevy::render::{Extract, RenderApp, RenderSystems};
use std::collections::HashMap;
use std::mem::swap;
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
        app.insert_resource(OcclusionConfig::default())
            .add_systems(Update, insert_corner_data);
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

pub fn extract_corners(mesh: &Mesh) -> Option<[Vec3; 4]> {
    let mut points: Vec<Vec3> = Vec::new();
    for (_attr, values) in mesh.attributes() {
        if let VertexAttributeValues::Float32x3(vec) = values {
            points.extend(vec.iter().map(|&p| Vec3::from(p)));
            break;
        }
    }

    if points.is_empty() {
        return None;
    }

    if points.len() != 4 {
        panic!("Expected 4 points but got {}", points.len());
    }
    Some(points.as_slice().try_into().unwrap())
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
    // 转为 Vec2 方便做浮点运算
    let n: [(CornerTuple, Vec2); 4] = points.map(|v| (v, Vec2::new(v.1.0 as f32, v.1.1 as f32)));

    let mut axis = 0.0;
    let mut diagonal = (0, 0);

    // 找出距离最大的两个点（矩形对角线）
    // points.cartesian_product().map().max() 总是对角线
    for i in 0..4 {
        for j in i + 1..4 {
            let d = (n[i].1 - n[j].1).length();
            if d > axis {
                axis = d;
                diagonal = (i, j);
            }
        }
    }

    // 第一根对角线的两个点
    let mut p0 = n[diagonal.0];
    let mut p2 = n[diagonal.1];
    if p0.1.x > p2.1.x {
        // 左上角总是 x 较小的那个
        swap(&mut p0, &mut p2);
    }
    let [mut p1, mut p3]: [(CornerTuple, Vec2); 2] = (0..4)
        .filter(|&i| i != diagonal.0 && i != diagonal.1)
        .map(|i| n[i])
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    if p1.1.x > p3.1.x {
        // 左上角总是 x 较小的那个
        swap(&mut p1, &mut p3);
    }
    /*
     * 记四个点为
     * | p0 p3 |
     * | p1 p2 |
     * <--x 减小
     * 现在保证：
     * - 左上列: p0.x <= p2.x
     * - 左下列: p1.x <= p3.x
     * 但是可能上下颠倒
     */

    // 同样的，根据 y 坐标调整顺序，让顺时针/逆时针正确
    if p0.1.y > p1.1.y {
        swap(&mut p0, &mut p1);
    }
    if p3.1.y > p2.1.y {
        swap(&mut p3, &mut p2);
    }

    [
        p0.0, // 左上
        p1.0, // 左下
        p2.0, // 右下
        p3.0, // 右上
    ]
}
type ArmorScreenData = (ArmorType, ArmorLabel, ArmorColor, [(u32, u32); 4]);

#[derive(Component, Deref, DerefMut, Clone)]
#[require(SyncToRenderWorld)]
pub struct CornerData(pub [Vec3; 4]);

fn insert_corner_data(
    mut commands: Commands,
    armor_query: Query<(Entity, &Mesh3d), (With<Armor>, Without<CornerData>)>,
    ass: Res<Assets<Mesh>>,
) {
    for (armor_entity, mesh_handle) in armor_query {
        let Some(corners) = extract_corners(ass.get(mesh_handle).unwrap()) else {
            continue;
        };
        commands
            .entity(armor_entity)
            .insert((CornerData(corners), Visibility::Hidden));
    }
}

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
        config: &CaptureConfig,
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
    armor_query: Extract<Query<(Entity, &GlobalTransform, &Armor, &CornerData)>>,
    camera: Extract<Single<(&Projection, &GlobalTransform), With<CaptureCamera>>>,
    mut occlusion: Extract<Occlusion>,
    config: Res<CaptureConfig>,
    armor_r: Res<Data>,
) {
    let mut armor_screen: HashMap<Team, Vec<ArmorScreenData>> = HashMap::new();
    let (projection, camera_global_transform) = **camera;
    let camera_pos = camera_global_transform.translation();

    for (armor_entity, global_transform, &Armor(team, typ, label), corners) in armor_query.iter() {
        // 屏幕投影
        let corners: Vec<_> = corners
            .into_iter()
            .map(|corner| global_transform.transform_point(corner))
            .filter_map(|corner| {
                let pos = world_to_screen(corner, camera_global_transform, projection, &config)?;
                Some((corner, pos))
            })
            .collect();
        if corners.len() != 4 {
            continue; // 四角没有完全在屏幕上
        }
        let corners_ordered = sort_screen_points([corners[0], corners[1], corners[2], corners[3]]);
        if !occlusion.visible(camera_pos, armor_entity, &corners_ordered) {
            continue;
        }
        armor_screen.entry(team).or_insert(default()).push((
            typ,
            label,
            match team {
                Team::Red => ArmorColor::Red,
                Team::Blue => ArmorColor::Blue,
            },
            corners_ordered.map(|v| v.1),
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
