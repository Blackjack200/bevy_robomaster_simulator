use crate::util::bevy::set_visibility;
use bevy::app::App;
use bevy::asset::AssetId;
use bevy::color::LinearRgba;
use bevy::ecs::system::lifetimeless::{Read, Write};
use bevy::prelude::{Children, Plugin, Resource};
use bevy::{
    asset::{Assets, Handle},
    camera::visibility::Visibility,
    ecs::{
        entity::Entity,
        system::{Query, ResMut, SystemParam},
    },
    pbr::{MeshMaterial3d, StandardMaterial},
};
use std::collections::HashMap;
use std::hash::Hash;

#[derive(SystemParam)]
pub struct StatefulAppearance<'w, 's> {
    materials: ResMut<'w, Assets<StandardMaterial>>,
    cache: ResMut<'w, MaterialCache>,
    mesh_materials: Query<'w, 's, Write<MeshMaterial3d<StandardMaterial>>>,
    visibilities: Query<'w, 's, Write<Visibility>>,
}

pub trait Control {
    fn set(&mut self, state: Activation, param: &mut StatefulAppearance);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Activation {
    Deactivated = 0,
    Activating = 1,
    Activated = 2,
    Completed = 3,
}

pub enum Controller {
    Material(
        Entity,
        Handle<StandardMaterial>,
        Handle<StandardMaterial>,
        Handle<StandardMaterial>,
        Handle<StandardMaterial>,
    ),

    Visibility(
        Option<Entity>,
        Option<Entity>,
        Option<Entity>,
        Option<Entity>,
    ),

    Combined(Vec<Controller>),
}

impl Control for Controller {
    fn set(&mut self, state: Activation, param: &mut StatefulAppearance) {
        match self {
            Self::Material(entity, deactivated, activating, activated, completed) => {
                let apply = match state {
                    Activation::Deactivated => deactivated,
                    Activation::Activating => activating,
                    Activation::Activated => activated,
                    Activation::Completed => completed,
                };
                if let Ok(mut mesh_material) = param.mesh_materials.get_mut(*entity) {
                    mesh_material.0 = apply.clone()
                }
            }
            Self::Visibility(deactivated, activating, activated, completed) => {
                let (show, hide) = match state {
                    Activation::Deactivated => (deactivated, [activating, activated, completed]),
                    Activation::Activating => (activating, [deactivated, activated, completed]),
                    Activation::Activated => (activated, [deactivated, activating, completed]),
                    Activation::Completed => (completed, [deactivated, activating, activated]),
                };
                for entity in hide {
                    if let Some(entity) = entity {
                        set_visibility(*entity, Visibility::Hidden, &mut param.visibilities)
                            .unwrap();
                    }
                }
                if let Some(show) = show {
                    set_visibility(*show, Visibility::Visible, &mut param.visibilities).unwrap();
                }
            }
            Self::Combined(vec) => {
                for c in vec {
                    c.set(state, param);
                }
            }
        }
    }
}

impl Controller {
    pub fn new_visibility(
        deactivated: Option<Entity>,
        activating: Option<Entity>,
        activated: Option<Entity>,
        completed: Option<Entity>,
    ) -> Self {
        Self::Visibility(deactivated, activating, activated, completed)
    }

    pub fn new_material(
        entity: Entity,
        deactivated: Handle<StandardMaterial>,
        activating: Handle<StandardMaterial>,
        activated: Handle<StandardMaterial>,
        completed: Handle<StandardMaterial>,
    ) -> Self {
        Self::Material(entity, deactivated, activating, activated, completed)
    }

    pub fn new_combined(v: Vec<Controller>) -> Self {
        Self::Combined(v)
    }
}

#[derive(Resource, Default)]
struct MaterialCache {
    muted: HashMap<AssetId<StandardMaterial>, Handle<StandardMaterial>>,
}

impl MaterialCache {
    fn ensure_muted(
        &mut self,
        handle: &Handle<StandardMaterial>,
        materials: &mut Assets<StandardMaterial>,
    ) -> Handle<StandardMaterial> {
        let id = handle.id();
        if let Some(existing) = self.muted.get(&id) {
            return existing.clone();
        }
        let Some(original) = materials.get(handle) else {
            return handle.clone();
        };
        let mut clone = original.clone();
        clone.emissive = LinearRgba::BLACK;
        clone.emissive_exposure_weight = 0.0;
        let muted_handle = materials.add(clone);
        self.muted.insert(id, muted_handle.clone());
        muted_handle
    }
}

pub type ConstructData<'w, 's, 'g> = (Entity, &'g mut StatefulAppearance<'w, 's>);

impl<'w, 's> StatefulAppearance<'w, 's> {
    pub fn visible(&self, entity: Entity) -> bool {
        if let Ok(v) = self.visibilities.get(entity) {
            v != Visibility::Hidden
        } else {
            true
        }
    }
}

#[derive(SystemParam)]
pub struct StatefulAppearanceCreator<'w, 's> {
    pub appearance: StatefulAppearance<'w, 's>,
    children: Query<'w, 's, Read<Children>>,
}

impl<'w, 's> StatefulAppearanceCreator<'w, 's> {
    fn as_combined<F: for<'g> Fn(ConstructData<'w, 's, 'g>) -> Result<Controller, ()>>(
        &mut self,
        entity: Entity,
        f: &F,
    ) -> Controller {
        let mut swaps = vec![];
        if let Ok(v) = f((entity, &mut self.appearance)) {
            swaps.push(v);
        }
        for child in self.children.iter_descendants(entity) {
            if let Ok(v) = f((child, &mut self.appearance)) {
                swaps.push(v);
            }
        }
        Controller::new_combined(swaps)
    }

    pub fn create_controller<F: for<'g> Fn(ConstructData<'w, 's, 'g>) -> Result<Controller, ()>>(
        &mut self,
        entities: Vec<Entity>,
        f: F,
    ) -> Controller {
        let mut controllers = Vec::new();
        for entity in entities {
            controllers.push(self.as_combined(entity, &f));
        }
        Controller::new_combined(controllers)
    }
}

pub fn material_raw<F>(f: F) -> impl Fn(ConstructData) -> Result<Controller, ()>
where
    F: Fn(Entity, Handle<StandardMaterial>, Handle<StandardMaterial>) -> Controller,
{
    move |value: ConstructData| -> Result<Controller, ()> {
        let (entity, param) = value;
        if let Ok(mut mesh_material) = param.mesh_materials.get_mut(entity) {
            let off = param
                .cache
                .ensure_muted(&mesh_material.0, &mut param.materials);
            let on = std::mem::replace(&mut mesh_material.0, off.clone());
            Ok(f(entity, on, off))
        } else {
            Err(())
        }
    }
}

#[macro_export]
macro_rules! internal_assign_hack {
    // this is literally a hack
    (@internal deactivated, $value:expr, $d:ident, $a:ident, $ac:ident, $c:ident) => {
        $d = $value;
    };
    (@internal activating, $value:expr, $d:ident, $a:ident, $ac:ident, $c:ident) => {
        $a = $value;
    };
    (@internal activated, $value:expr, $d:ident, $a:ident, $ac:ident, $c:ident) => {
        $ac = $value;
    };
    (@internal completed, $value:expr, $d:ident, $a:ident, $ac:ident, $c:ident) => {
        $c = $value;
    };
}

#[macro_export]
macro_rules! visibility {
    ($($state:ident),* $(,)?) => {
        |value: $crate::robomaster::visibility::ConstructData| -> Result<$crate::robomaster::visibility::Controller, ()> {
            use ::std::option::Option::{Some, None};
            use $crate::internal_assign_hack;
            // 4 optional fields
            let mut _deactivated = None;
            let mut _activating = None;
            let mut _activated = None;
            let mut _completed = None;

            $(
                internal_assign_hack!(@internal $state, Some(value.0), _deactivated, _activating, _activated, _completed);
            )*

            Ok($crate::robomaster::visibility::Controller::new_visibility(_deactivated, _activating, _activated, _completed))
        }
    };
}

#[macro_export]
macro_rules! material {
    ( on = {$($on:ident),* $(,)?}) => {
         $crate::robomaster::visibility::material_raw(|entity, on, off| {
             use $crate::internal_assign_hack;

             let mut _deactivated = off.clone();
             let mut _activating = off.clone();
             let mut _activated = off.clone();
             let mut _completed = off.clone();

             $(
                internal_assign_hack!(@internal $on, on.clone(), _deactivated, _activating, _activated, _completed);
             )*
             $crate::robomaster::visibility::Controller::new_material(entity, _deactivated, _activating, _activated, _completed)
         })
    };
}

#[derive(Default)]
pub(super) struct StatefulAppearancePlugin;

impl Plugin for StatefulAppearancePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MaterialCache>();
    }
}
