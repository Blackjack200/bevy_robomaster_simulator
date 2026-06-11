use crate::capture::CaptureCamera;
use crate::capture::driver::{
    CaptureConfig, CapturedFrame, CapturedFrameKind, GpuCaptureHandler, SnapshotAsync, SnapshotSync,
};
use crate::dataset::occlusion::{DEPTH_EPSILON_M, DepthSample, entity_fully_visible_in_depth};
use crate::dataset::writer::{ArmorColor, ArmorEntry, DatasetWriter};
use crate::robomaster::prelude::{
    Armor, ArmorLabel, ArmorParts, ArmorRoot, ArmorType, MarkerData, Side, Team, VertexData,
};
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use bevy::render::{Extract, RenderApp, RenderSystems};
use std::sync::{Arc, Mutex};

pub const DATASET_DEPTH_NEAR: f32 = 0.1;
pub const DATASET_DEPTH_FAR: f32 = 10_000.0;

#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemSet)]
pub enum ArmorOcclusionSystems {
    Propagate,
}

#[derive(Resource, Deref, DerefMut)]
pub struct DatasetHandle(pub Arc<Mutex<DatasetWriter>>);

#[derive(Resource, Deref, DerefMut)]
struct Cooldown(Mutex<Timer>);

#[derive(Debug, Clone)]
struct PendingArmorEntry {
    color: ArmorColor,
    typ: ArmorType,
    label: ArmorLabel,
    corners_px: [(u32, u32); 4],
    samples: Vec<DepthSample>,
}

#[derive(Debug, Clone)]
struct PendingFrame {
    frame_name: String,
    armors: Vec<PendingArmorEntry>,
    rgb_reserved: bool,
    depth_reserved: bool,
    rgb: Option<(u32, u32, Vec<u8>)>,
    depth: Option<(u32, u32, Vec<u8>)>,
}

#[derive(Default, Resource)]
pub(crate) struct Data(Arc<Mutex<Vec<PendingFrame>>>);

impl Data {
    fn queue(&self) -> Arc<Mutex<Vec<PendingFrame>>> {
        self.0.clone()
    }
}

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

    if clip.w <= 0.0 {
        return None;
    }

    let ndc = clip.xyz() / clip.w;
    if ndc.x < -1.0 || ndc.x > 1.0 || ndc.y < -1.0 || ndc.y > 1.0 {
        return None;
    }

    let screen_x = (ndc.x + 1.0) * 0.5 * (config.width as f32);
    let screen_y = (1.0 - ndc.y) * 0.5 * (config.height as f32);

    Some((screen_x as u32, screen_y as u32))
}

type CornerTuple = (Vec3, (u32, u32));

pub(crate) fn sort_screen_points(points: [CornerTuple; 4]) -> [CornerTuple; 4] {
    let points_with_vec: Vec<(CornerTuple, Vec2)> = points
        .iter()
        .map(|&value| (value, Vec2::new(value.1.0 as f32, value.1.1 as f32)))
        .collect();

    let center = points_with_vec
        .iter()
        .map(|(_, value)| *value)
        .fold(Vec2::ZERO, |acc, value| acc + value)
        / 4.0;

    let mut sorted: Vec<(CornerTuple, Vec2, f32)> = points_with_vec
        .into_iter()
        .map(|(tuple, vec)| {
            let dir = (vec - center).normalize();
            let angle = dir.angle_to(Vec2::X).to_degrees();
            (tuple, vec, angle)
        })
        .collect();

    sorted.sort_by(|lhs, rhs| rhs.2.partial_cmp(&lhs.2).unwrap());
    [sorted[0].0, sorted[3].0, sorted[2].0, sorted[1].0]
}

#[derive(Clone, Copy)]
enum SnapshotKind {
    Color,
    Depth,
}

pub struct DatasetSnapshotCreator {
    kind: SnapshotKind,
}

impl Default for DatasetSnapshotCreator {
    fn default() -> Self {
        Self {
            kind: SnapshotKind::Color,
        }
    }
}

impl DatasetSnapshotCreator {
    pub fn depth() -> Self {
        Self {
            kind: SnapshotKind::Depth,
        }
    }
}

impl GpuCaptureHandler for DatasetSnapshotCreator {
    fn captured(&self, world: &World) -> Option<Box<dyn SnapshotSync>> {
        let queue = world.resource::<Data>().queue();
        let mut guard = queue.lock().unwrap();
        let frame = guard.iter_mut().find(|frame| match self.kind {
            SnapshotKind::Color => !frame.rgb_reserved,
            SnapshotKind::Depth => !frame.depth_reserved,
        })?;

        match self.kind {
            SnapshotKind::Color => frame.rgb_reserved = true,
            SnapshotKind::Depth => frame.depth_reserved = true,
        }

        Some(Box::new(DatasetSnapshotSync {
            frame_name: frame.frame_name.clone(),
            kind: self.kind,
            queue: queue.clone(),
        }))
    }
}

struct DatasetSnapshotSync {
    frame_name: String,
    kind: SnapshotKind,
    queue: Arc<Mutex<Vec<PendingFrame>>>,
}

impl SnapshotSync for DatasetSnapshotSync {
    fn captured(
        self: Box<Self>,
        world: &mut DeferredWorld,
        _config: &CaptureConfig,
    ) -> Box<dyn SnapshotAsync> {
        Box::new(DatasetSnapshot {
            frame_name: self.frame_name,
            kind: self.kind,
            queue: self.queue.clone(),
            writer: world.resource::<DatasetHandle>().0.clone(),
        })
    }
}

struct DatasetSnapshot {
    frame_name: String,
    kind: SnapshotKind,
    queue: Arc<Mutex<Vec<PendingFrame>>>,
    writer: Arc<Mutex<DatasetWriter>>,
}

impl SnapshotAsync for DatasetSnapshot {
    fn captured(&mut self, frame: CapturedFrame<'_>) {
        let mut queue = self.queue.lock().unwrap();
        let Some(index) = queue
            .iter()
            .position(|item| item.frame_name == self.frame_name)
        else {
            return;
        };

        match self.kind {
            SnapshotKind::Color if frame.kind == CapturedFrameKind::Rgb8 => {
                queue[index].rgb = Some((frame.width, frame.height, frame.data.to_vec()));
            }
            SnapshotKind::Depth if frame.kind == CapturedFrameKind::Depth32F => {
                queue[index].depth = Some((frame.width, frame.height, frame.data.to_vec()));
            }
            _ => return,
        }

        let ready = queue[index].rgb.is_some() && queue[index].depth.is_some();
        if !ready {
            return;
        }

        let finished = queue.remove(index);
        drop(queue);
        finalize_pending_frame(&self.writer, finished).unwrap();
    }
}

fn finalize_pending_frame(
    writer: &Arc<Mutex<DatasetWriter>>,
    pending: PendingFrame,
) -> std::io::Result<()> {
    let (rgb_width, rgb_height, rgb_data) = pending.rgb.unwrap();
    let (depth_width, depth_height, depth_data) = pending.depth.unwrap();

    let visible_entries = pending
        .armors
        .into_iter()
        .filter(|entry| {
            entity_fully_visible_in_depth(
                depth_width,
                depth_height,
                depth_data.as_slice(),
                DATASET_DEPTH_NEAR,
                DEPTH_EPSILON_M,
                entry.samples.as_slice(),
            )
        })
        .map(|entry| ArmorEntry {
            color: entry.color,
            typ: entry.typ,
            label: entry.label,
            points: entry
                .corners_px
                .map(|(x, y)| Vec2::new(x as f32 / rgb_width as f32, y as f32 / rgb_height as f32)),
        })
        .collect::<Vec<_>>();

    let mut writer = writer.lock().unwrap();
    writer.write_color_entry(
        pending.frame_name.as_str(),
        rgb_height,
        rgb_width,
        rgb_data.as_slice(),
        visible_entries.as_slice(),
    )?;
    writer.write_depth_entry(
        pending.frame_name.as_str(),
        depth_width,
        depth_height,
        depth_data.as_slice(),
        DATASET_DEPTH_NEAR,
        DATASET_DEPTH_FAR,
    )
}

pub(crate) fn capture(
    root_data: Extract<Query<(Entity, &Armor, &ArmorRoot, &ArmorParts)>>,
    vertex_data: Extract<Query<(&GlobalTransform, &VertexData)>>,
    marker_data: Extract<Query<(&GlobalTransform, &MarkerData)>>,
    camera: Extract<Single<(&Projection, &GlobalTransform), With<CaptureCamera>>>,
    config: Res<CaptureConfig>,
    queue: Res<Data>,
    handle: Res<DatasetHandle>,
) {
    let (projection, camera_global_transform) = **camera;
    let mut armors = Vec::new();

    for (_entity, armor, _root, parts) in root_data.iter() {
        let project_points = |global_transform: &GlobalTransform,
                              points: &[Vec3]|
         -> Option<Vec<(Vec3, (u32, u32), f32)>> {
            let mut mapped = Vec::with_capacity(points.len());
            for point in points {
                let world = global_transform.transform_point(*point);
                let pixel = world_to_screen(world, camera_global_transform, projection, &config)?;
                let depth_m = camera_depth_m(camera_global_transform, world)?;
                mapped.push((world, pixel, depth_m));
            }
            Some(mapped)
        };

        let marker = parts.marker();
        let vertices = [parts.vertex(Side::Left), parts.vertex(Side::Right)];
        let mut samples = Vec::new();
        for vertex in vertices {
            let (tf, vertex_data) = vertex_data.get(vertex).unwrap();
            let Some(vertices) = project_points(tf, vertex_data.points.as_slice()) else {
                continue;
            };
            samples.extend(
                vertices
                    .into_iter()
                    .map(|(_, (x, y), depth_m)| DepthSample {
                        pixel: UVec2::new(x, y),
                        expected_depth_m: depth_m,
                    }),
            );
        }

        let (tf, MarkerData(markers)) = marker_data.get(marker).unwrap();
        let Some(markers) = project_points(tf, markers) else {
            continue;
        };
        samples.extend(markers.iter().map(|(_, (x, y), depth_m)| DepthSample {
            pixel: UVec2::new(*x, *y),
            expected_depth_m: *depth_m,
        }));

        let marker_ordered = sort_screen_points([
            (markers[0].0, markers[0].1),
            (markers[1].0, markers[1].1),
            (markers[2].0, markers[2].1),
            (markers[3].0, markers[3].1),
        ]);
        armors.push(PendingArmorEntry {
            color: match armor.team {
                Team::Red => ArmorColor::Red,
                Team::Blue => ArmorColor::Blue,
            },
            typ: armor.spec.armor_type(),
            label: armor.label,
            corners_px: marker_ordered.map(|value| value.1),
            samples,
        });
    }

    if armors.is_empty() {
        return;
    }

    let frame_name = handle.lock().unwrap().next_frame_name();
    queue.queue().lock().unwrap().push(PendingFrame {
        frame_name,
        armors,
        rgb_reserved: false,
        depth_reserved: false,
        rgb: None,
        depth: None,
    });
}

fn camera_depth_m(camera_transform: &GlobalTransform, world: Vec3) -> Option<f32> {
    let view = camera_transform.to_matrix().inverse();
    let point_view = view.transform_point3(world);
    let depth_m = -point_view.z;
    (depth_m > 0.0).then_some(depth_m)
}
