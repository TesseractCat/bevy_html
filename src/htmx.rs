use bevy::prelude::*;
use bevy::{ecs::component::Component, reflect::ReflectDeserialize};
use bevy::ecs::reflect::ReflectComponent;
use bevy::reflect::std_traits::ReflectDefault;
use bevy::reflect::Reflect;
use serde::{Serialize, Deserialize};

use crate::{HTMLScene, spawn_scene_system};
use crate::named_system_registry::NamedSystemRegistry;

#[derive(Component, Serialize, Deserialize, Default, Debug, Clone, Reflect)]
#[reflect(Component, Deserialize, Default)]
pub enum XSwap {
    #[default]
    Outer,
    Inner,
    Front,
    Back
}
#[derive(Component, Serialize, Deserialize, Default, Debug, Clone, Reflect)]
#[reflect(Component, Deserialize, Default)]
pub enum XTarget { // TODO: Some equivalent to CSS selectors (dynamic queries?)
    #[default]
    This,
    NextSibling,
    PreviousSibling,
    Root,
    Name(String),
    ChildName(String),
    Entity(Entity)
}
#[derive(Component, Serialize, Deserialize, Default, Debug, Clone, Reflect)]
#[reflect(Component, Deserialize)]
pub enum XOn {
    #[default]
    Create,
    Update,
    Fixed(f32),
    Click,
    Event(String)
}
#[derive(Component, Serialize, Deserialize, Default, Debug, Clone, Reflect)]
#[reflect(Component, Deserialize)]
pub struct XFunction(pub String);

type ToRun = (Entity, XFunction, XOn, XSwap, XTarget);

fn find_to_run(
    created_entities: Query<(), Added<Transform>>,
    interactions: Query<&Interaction, Changed<Interaction>>,
    x_entities: Query<(Entity, &XFunction, Option<&XOn>, Option<&XSwap>, Option<&XTarget>)>
) -> Vec<ToRun> {
    let mut to_run = Vec::new();

    for (entity, func, on, swap, target) in &x_entities {
        let func = func.clone();
        let on = on.cloned().unwrap_or_default();
        let swap = swap.cloned().unwrap_or_default();
        let target = target.cloned().unwrap_or_default();

        if match on {
            XOn::Create => created_entities.contains(entity),
            XOn::Click => interactions.get(entity)
                            .map(|i| matches!(i, Interaction::Pressed))
                            .unwrap_or(false),
            XOn::Update => true,
            _ => unimplemented!()
        } {
            to_run.push((entity, func, on, swap, target))
        }
    }

    to_run
}

fn run_x_funcs(
    to_run: In<Vec<ToRun>>, world: &mut World
) -> Vec<(ToRun, HTMLScene)> {
    let ran = world.resource_scope(|world, named_system_registry: Mut<NamedSystemRegistry>| {
        to_run.0.iter().map(|(_, func, _, _, _)|
            named_system_registry.call::<(), HTMLScene>(world, func.0.as_str(), ()).unwrap()
        ).collect::<Vec<_>>()
    });
    to_run.0.into_iter().zip(ran.into_iter()).collect()
}

fn swap_system(
    to_run: In<Vec<(ToRun, HTMLScene)>>,
    mut html_scenes: ResMut<Assets<HTMLScene>>,
    name_query: Query<(Entity, &Name)>,
    children: Query<&Children>,
    mut commands: Commands
) {
    for ((entity, _, _, swap, target), xs) in to_run.0.into_iter() {
        let entity = match target {
            XTarget::This => entity,
            XTarget::Name(name) => name_query.iter().find(|(_, n)| n.as_str() == name).unwrap().0,
            XTarget::ChildName(name) => children.iter_descendants(entity)
                                            .find(|d| name_query.get(*d).map(|(_, n)| n.as_str() == name).unwrap_or(false)).unwrap(),
            _ => unimplemented!()
        };
        match swap {
            XSwap::Outer => {
                commands.entity(entity)
                    .despawn_descendants()
                    .insert(html_scenes.add(xs));
            },
            XSwap::Inner => {
                let child = commands.spawn_empty()
                    .insert(html_scenes.add(xs))
                    .id();
                commands.entity(entity)
                    .despawn_descendants()
                    .add_child(child);
            },
            XSwap::Back => {
                let child = commands.spawn_empty()
                    .insert(html_scenes.add(xs))
                    .id();
                commands.entity(entity)
                    .push_children(&[child]);
            },
            XSwap::Front => {
                let child = commands.spawn_empty()
                    .insert(html_scenes.add(xs))
                    .id();
                commands.entity(entity)
                    .insert_children(0, &[child]);
            }
        }
    }
}

pub struct XPlugin;
impl Plugin for XPlugin {
    fn build(&self, app: &mut App) {
        app
            .register_type::<XSwap>()
            .register_type::<XTarget>()
            .register_type::<XFunction>()
            .register_type::<XOn>()
            
            .add_systems(PreUpdate,
                (find_to_run.pipe(run_x_funcs).pipe(swap_system), apply_deferred).before(spawn_scene_system)
            );
    }
}