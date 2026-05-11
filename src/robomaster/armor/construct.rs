use crate::query;
use crate::robomaster::prelude::{ArmorLabel, ArmorSpec, MarkerData, Team, extract_markers};
use crate::util::entity_query::HierarchyQuery;
use avian3d::prelude::{ColliderConstructor, ColliderConstructorHierarchy, TrimeshFlags};
use bevy::app::App;
use bevy::ecs::system::SystemParam;
use bevy::ecs::system::lifetimeless::Read;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::{
    Added, Assets, Changed, ChildOf, Children, Commands, Component, Entity, Mesh, Mesh3d, Name,
    Plugin, Query, Res, Update, Vec3, Visibility, With, info,
};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Component, Debug)]
pub struct ScanArmor {
    pub team: Team,
    pub spec: ArmorSpec,
}

impl ScanArmor {
    pub const fn new(team: Team, spec: ArmorSpec) -> Self {
        Self { team, spec }
    }
}

#[derive(Component, Clone, Debug)]
pub struct VertexData {
    pub side: Side,
    pub points: Vec<Vec3>,
}

#[derive(Component, Clone, Debug)]
pub struct LightStrip {
    pub side: Side,
}

#[derive(Component, Clone, Debug)]
pub struct Armor {
    pub name: String,
    pub team: Team,
    pub spec: ArmorSpec,
    pub label: ArmorLabel,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct ArmorSticker {
    pub root: Entity,
    pub label: ArmorLabel,
}

#[derive(Component, Clone, Debug)]
pub struct ArmorStickerSelection {
    pub label: ArmorLabel,
    pub sequence_index: usize,
}

impl ArmorStickerSelection {
    pub fn new(label: ArmorLabel) -> Self {
        Self {
            label,
            sequence_index: ArmorLabel::index_from_small(label),
        }
    }

    pub fn advance_debug_sequence(&mut self) -> ArmorLabel {
        let sequence = ArmorLabel::sequence_small();
        self.sequence_index += 1;
        self.sequence_index %= sequence.len();
        self.label = sequence[self.sequence_index];
        self.label
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub const fn index(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Right => 1,
        }
    }
}

#[derive(SystemParam)]
pub struct ArmorConstructor<'w, 's> {
    commands: Commands<'w, 's>,
    children: Query<'w, 's, Read<Children>>,
    child_of: Query<'w, 's, Read<ChildOf>>,
    name: Query<'w, 's, Read<Name>, With<ChildOf>>,
    mesh_query: Query<'w, 's, Read<Mesh3d>>,
    mesh_assets: Res<'w, Assets<Mesh>>,
}

#[derive(Component, Clone)]
pub struct ArmorRoot {
    pub id: ArmorId,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct ArmorId(usize);

impl ArmorId {
    pub const fn as_usize(self) -> usize {
        self.0
    }
}

#[derive(Component, Clone)]
pub struct ArmorParts {
    marker: Entity,
    lights: [Entity; 2],
    vertices: [Entity; 2],
}

macro_rules! impl_side {
    ($method_name:ident, $field:ident) => {
        #[inline]
        #[must_use]
        pub fn $method_name(&self, side: Side) -> Entity {
            self.$field[side.index()]
        }
    };
}

impl ArmorParts {
    impl_side!(light, lights);
    impl_side!(vertex, vertices);

    #[inline]
    #[must_use]
    pub fn marker(&self) -> Entity {
        self.marker
    }
}

impl ArmorConstructor<'_, '_> {
    fn get_mesh(&self, entity: Entity) -> Option<&Mesh> {
        let mesh_handle = self.mesh_query.get(entity).ok()?;
        self.mesh_assets.get(mesh_handle)
    }

    fn process_marker(
        &mut self,
        entity: Entity,
        name: &str,
        armor_data: &ScanArmor,
    ) -> Option<MarkerData> {
        let mesh = self.get_mesh(entity)?;
        let vertices = extract_markers(mesh)?;

        info!(
            "Armor {:?}_{:?}_{:?}@'{}': Added marker with {} points",
            armor_data.team,
            armor_data.spec.armor_type(),
            armor_data.spec.label(),
            name,
            vertices.len()
        );

        self.commands
            .entity(entity)
            .insert((MarkerData(vertices), Visibility::Hidden));
        Some(MarkerData(vertices))
    }

    fn extract_vertex(
        &mut self,
        entity: Entity,
        name: &str,
        armor_data: &ScanArmor,
    ) -> Option<Vec<Vec3>> {
        let mesh = self.get_mesh(entity)?;

        let vertices = extract_vertices(mesh)?;

        info!(
            "Armor {:?}_{:?}_{:?}@'{}': Extracted {} vertices",
            armor_data.team,
            armor_data.spec.armor_type(),
            armor_data.spec.label(),
            name,
            vertices.len()
        );

        Some(vertices)
    }

    fn process_armor_root(
        &mut self,
        root: Entity,
        armor_name: String,
        armor_data: &ScanArmor,
    ) -> Option<ArmorRoot> {
        let query = HierarchyQuery::new(self.child_of, self.children, self.name);
        let root_query = query.of(root).flatten();
        {
            self.commands.entity(query!(root_query, .."ARMOR")?).insert(
                ColliderConstructorHierarchy::new(ColliderConstructor::TrimeshFromMeshWithConfig(
                    TrimeshFlags::MERGE_DUPLICATE_VERTICES,
                )),
            );
        }
        {
            let children = self.children;

            let name = self.name;
            children
                .iter_descendants(root)
                .filter_map(|v| name.get(v).ok().map(|name| (name, v)))
                .for_each(|(elem_name, armor_elem)| {
                    self.commands.entity(armor_elem).insert(Armor {
                        name: elem_name.to_string(),
                        team: armor_data.team,
                        spec: armor_data.spec,
                        label: armor_data.spec.label(),
                    });
                });
        }
        //let _base = query!(root_query, .."BASE")?;
        let lights = [
            [query!(root_query, .."L_L")?, query!(root_query, .."L_R")?],
            [
                query!(root_query, .."L_L_RED")?,
                query!(root_query, .."L_R_RED")?,
            ],
        ];
        let (lights, hide) = match armor_data.team {
            Team::Red => (lights[1], lights[0]),
            Team::Blue => (lights[0], lights[1]),
        };
        for hide in hide {
            self.commands.entity(hide).despawn();
        }

        self.commands
            .entity(lights[0])
            .insert(LightStrip { side: Side::Left });
        self.commands
            .entity(lights[1])
            .insert(LightStrip { side: Side::Right });

        let marker = query!(root_query, .."MARKER", ...)?;
        self.process_marker(marker, &armor_name, armor_data)?;

        let vertex = [
            (Side::Left, query!(root_query, .."VERTEX_L", ...)?),
            (Side::Right, query!(root_query, .."VERTEX_R", ...)?),
        ];
        let vertices = vertex.map(|(side, vertex)| {
            let v = self
                .extract_vertex(vertex, &armor_name, armor_data)
                .unwrap();
            self.commands.entity(vertex).insert((
                VertexData {
                    side,
                    points: v.clone(),
                },
                Visibility::Hidden,
            ));
            vertex
        });
        {
            let c_query = query!(root_query, .."_C", ref).flatten();
            c_query.clone().any().into_iter().for_each(|e| {
                self.commands.entity(e).insert(Visibility::Hidden);
            });
            for slot in armor_data.spec.sticker_slots() {
                let sticker = c_query.clone().suffix(slot.name_suffix).one()?;
                self.commands.entity(sticker).insert((
                    ArmorSticker {
                        root,
                        label: slot.label,
                    },
                    match slot.label == armor_data.spec.label() {
                        true => Visibility::Visible,
                        false => Visibility::Hidden,
                    },
                ));
            }
        }

        self.commands.entity(root).insert(Armor {
            name: armor_name.clone(),
            team: armor_data.team,
            spec: armor_data.spec,
            label: armor_data.spec.label(),
        });

        static ID: AtomicUsize = AtomicUsize::new(0);

        let ar = ArmorRoot {
            id: ArmorId(ID.fetch_add(1, Ordering::SeqCst)),
        };
        let parts = ArmorParts {
            marker,
            lights,
            vertices,
        };
        self.commands.entity(root).insert((
            ar.clone(),
            parts,
            ArmorStickerSelection::new(armor_data.spec.label()),
        ));
        Some(ar)
    }
}

/// 从Mesh中提取所有顶点
pub fn extract_vertices(mesh: &Mesh) -> Option<Vec<Vec3>> {
    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        .and_then(|values| {
            if let VertexAttributeValues::Float32x3(vec) = values {
                Some(vec.iter().map(|&p| Vec3::from(p)).collect())
            } else {
                None
            }
        })
        .filter(|points: &Vec<Vec3>| !points.is_empty())
}

fn insert(
    root: Query<(Entity, Read<ScanArmor>), Added<ScanArmor>>,
    mut constructor: ArmorConstructor,
) {
    for (root_entity, armor_data) in root.iter() {
        let children = constructor.children;
        let name = constructor.name;
        children
            .iter_descendants(root_entity)
            .filter_map(|child| {
                name.get(child)
                    .ok()
                    .filter(|name| name.contains("ARMOR_ROOT"))
                    .map(|name| (child, name))
            })
            .for_each(|(ent, name)| {
                constructor.process_armor_root(ent, name.to_string(), armor_data);
            })
    }
}

fn sync_armor_stickers(
    mut commands: Commands,
    selections: Query<(Entity, &ArmorStickerSelection), Changed<ArmorStickerSelection>>,
    stickers: Query<(Entity, &ArmorSticker)>,
) {
    for (root, selection) in &selections {
        for (entity, sticker) in &stickers {
            if sticker.root != root {
                continue;
            }
            commands
                .entity(entity)
                .insert(match sticker.label == selection.label {
                    true => Visibility::Visible,
                    false => Visibility::Hidden,
                });
        }
    }
}

#[derive(Default)]
pub(super) struct ArmorConstructorPlugin;

impl Plugin for ArmorConstructorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (insert, sync_armor_stickers));
    }
}
