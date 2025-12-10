use crate::robomaster::prelude::{ArmorLabel, ArmorType, MarkerData, Team, extract_markers};
use avian3d::prelude::{ColliderConstructor, ColliderConstructorHierarchy, TrimeshFlags};
use bevy::app::App;
use bevy::ecs::system::SystemParam;
use bevy::ecs::system::lifetimeless::Read;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::{
    Added, Assets, ChildOf, Children, Commands, Component, Entity, Mesh, Mesh3d, Name, Plugin,
    Query, Res, Update, Vec3, Visibility, With, error, info,
};

#[derive(Component)]
pub struct ScanArmor(pub Team, pub ArmorType, pub ArmorLabel);

#[derive(Component, Clone, Debug)]
pub struct VertexData(pub Side, pub Vec<Vec3>);

#[derive(Component, Clone)]
pub struct Armor(pub ArmorIdentifier, pub Team, pub ArmorType, pub ArmorLabel);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Unknown(String),
}

impl<T: ToString> From<T> for Side {
    fn from(value: T) -> Self {
        let str = value.to_string();
        match str.to_lowercase().as_str() {
            "left" | "l" => Side::Left,
            "right" | "r" => Side::Right,
            _ => Side::Unknown(str),
        }
    }
}

/// 装甲组件类型枚举
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArmorComponentType {
    Root,            // 根实体
    Character,       // 字符标识
    LightBar(Side),  // 灯条
    Marker,          // 标记点
    Vertex(Side),    // 顶点数据
    Collider,        // 碰撞体（基础装甲）
    Unknown(String), // 未知类型
}

impl ArmorComponentType {
    fn from_name_parts(parts: Vec<&str>) -> Self {
        if parts.is_empty() {
            return Self::Collider;
        }
        match parts[0] {
            "ROOT" => Self::Root,
            "C" => Self::Character,
            "L" => Self::LightBar(parts.get(1).unwrap_or(&"unknown").into()),
            "MARKER" => Self::Marker,
            "VERTEX" => Self::Vertex(parts.get(1).unwrap_or(&"unknown").into()),
            other => Self::Unknown(other.to_string()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArmorIdentifier {
    pub identifier: String,
    pub component_type: ArmorComponentType,
}

impl ArmorIdentifier {
    fn parse(name: &str) -> Option<Self> {
        let name = &name[..name.rfind('.').unwrap_or(name.len())];
        let pos = name.find("ARMOR")?;
        let identifier = name[..pos].to_string();
        let armor_parts = &name[pos..];
        let mut parts: Vec<&str> = armor_parts.split('_').collect();
        if parts.first() != Some(&"ARMOR") {
            return None;
        }
        parts.remove(0);
        let component_type = ArmorComponentType::from_name_parts(parts);
        Some(Self {
            identifier,
            component_type,
        })
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
#[derive(Component)]
pub struct ArmorRoot {
    pub marker: Entity,
    pub vertices: Vec<Entity>,
}

impl ArmorConstructor<'_, '_> {
    fn get_mesh(&self, entity: Entity) -> Option<&Mesh> {
        let mesh_handle = self.mesh_query.get(entity).ok()?;
        self.mesh_assets.get(mesh_handle)
    }

    fn process_marker(
        &mut self,
        entity: Entity,
        info: &ArmorIdentifier,
        armor_data: &ScanArmor,
    ) -> Option<MarkerData> {
        let mesh = self.get_mesh(entity)?;
        let vertices = extract_markers(mesh)?;

        info!(
            "Armor {:?}_{:?}_{:?}@'{}': Added marker with {} points",
            armor_data.0,
            armor_data.1,
            armor_data.2,
            info.identifier,
            vertices.len()
        );

        self.commands
            .entity(entity)
            .insert((MarkerData(vertices), Visibility::Hidden));
        Some(MarkerData(vertices))
    }

    fn process_vertex(
        &mut self,
        entity: Entity,
        info: &ArmorIdentifier,
        armor_data: &ScanArmor,
    ) -> Option<Vec<Vec3>> {
        let mesh = self.get_mesh(entity)?;

        let vertices = extract_vertices(mesh)?;

        info!(
            "Armor {:?}_{:?}_{:?}@'{}': Extracted {} vertices",
            armor_data.0,
            armor_data.1,
            armor_data.2,
            info.identifier,
            vertices.len()
        );

        Some(vertices)
    }

    fn process_collider(&mut self, entity: Entity) {
        self.commands
            .entity(entity)
            .insert(ColliderConstructorHierarchy::new(
                ColliderConstructor::TrimeshFromMeshWithConfig(
                    TrimeshFlags::MERGE_DUPLICATE_VERTICES,
                ),
            ));
    }

    fn process_armor_root(&mut self, root: Entity, armor_data: &ScanArmor) {
        let name = self.name.get(root).unwrap();
        let Some(info) = ArmorIdentifier::parse(name) else {
            info!("Failed to parse armor name: {}", name);
            return;
        };
        self.commands.entity(root).insert(Armor(
            info.clone(),
            armor_data.0,
            armor_data.1,
            armor_data.2,
        ));

        // 为所有子节点添加 Armor 组件
        let children = self.children;

        let name = self.name;
        let mut vertices = vec![];
        let mut marker = None;
        children
            .iter_descendants(root)
            .filter_map(|v| name.get(v).ok().map(|name| (name, v)))
            .for_each(|(name, armor_elem)| {
                let Some(info) = ArmorIdentifier::parse(name) else {
                    info!("Failed to parse armor name: {}", name);
                    return;
                };
                self.commands.entity(armor_elem).insert(Armor(
                    info.clone(),
                    armor_data.0,
                    armor_data.1,
                    armor_data.2,
                ));
                // 根据组件类型执行不同的处理
                match info.component_type {
                    ArmorComponentType::Character
                    | ArmorComponentType::LightBar(_)
                    | ArmorComponentType::Root => {
                        // 忽略这些类型
                    }
                    ArmorComponentType::Marker => {
                        let Some(_) = self.process_marker(armor_elem, &info, armor_data) else {
                            return;
                        };
                        marker = Some(armor_elem);
                    }
                    ArmorComponentType::Vertex(ref side) => {
                        let Some(v) = self.process_vertex(armor_elem, &info, armor_data) else {
                            return;
                        };
                        self.commands
                            .entity(armor_elem)
                            .insert((VertexData(side.clone(), v.clone()), Visibility::Hidden));
                        vertices.push(armor_elem);
                    }
                    ArmorComponentType::Collider => {
                        self.process_collider(armor_elem);
                    }
                    ArmorComponentType::Unknown(ref type_name) => {
                        info!("Unknown armor component type: {} in {}", type_name, name);
                    }
                }
            });
        let Some(marker) = marker else {
            error!("{} has no marker data", root);
            return;
        };
        self.commands
            .entity(root)
            .insert(ArmorRoot { marker, vertices });
    }
}

/// 从Mesh中提取所有顶点
pub fn extract_vertices(mesh: &Mesh) -> Option<Vec<Vec3>> {
    mesh.attributes()
        .find_map(|(_, values)| {
            if let VertexAttributeValues::Float32x3(vec) = values {
                Some(vec.iter().map(|&p| Vec3::from(p)).collect())
            } else {
                None
            }
        })
        .filter(|points: &Vec<Vec3>| !points.is_empty())
}

#[macro_export]
macro_rules! entity_root {
    (
        super $child_of:expr => $children:expr;
        name $name:expr;
        $root:ident {
            $($expr:tt)*
        }
    ) => {{
        let _child_of = &$child_of;
        let _children = &$children;
        let _name = &$name;
        let _root = $root;
        $crate::entity_root!(@internal _root, _name, _child_of, _children, { $($expr)* });
    }};

    (@internal $root:expr, $name:ident, $child_of:ident, $children:ident, {
        match {
            $($label:literal => $body:ident $tt:tt);* $(;)?
        }
    }) => {{
        if let Ok(children) = $children.get($root) {
            for &child in children.iter() {
                let Ok(name) = $name.get(child) else { continue; };
                let name_str = name.as_str();

                $(
                    if name_str == $label {
                        let $body = child;
                        $crate::entity_root!(@internal $body, $name, $child_of, $children, $tt);
                    }
                )*
            }
        }
    }};

    (@internal $root:expr, $name:ident, $child_of:ident, $children:ident, {
        $($stmt:stmt);* $(;)?
    }) => {{
        let _ = $root;
        $($stmt)*
    }};

    (@internal $root:expr, $name:ident, $child_of:ident, $children:ident, {}) => {{}};
}

fn insert(
    root: Query<(Entity, Read<ScanArmor>), Added<ScanArmor>>,
    mut constructor: ArmorConstructor,
) {
    for (root_entity, armor_data) in root.iter() {
        let children = constructor.children;
        let name = constructor.name;
        let armor_root: Vec<_> = children
            .iter_descendants(root_entity)
            .filter(|child| {
                name.get(*child)
                    .is_ok_and(|name| name.contains("ARMOR_ROOT"))
            })
            .collect();
        // 处理每个装甲子节点
        for root in armor_root {
            constructor.process_armor_root(root, armor_data);
        }
    }
}

#[derive(Default)]
pub(super) struct ArmorConstructorPlugin;

impl Plugin for ArmorConstructorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, insert);
    }
}
