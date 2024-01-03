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
    Entity(Entity)
}
#[derive(Component, Serialize, Deserialize, Default, Debug, Clone, Reflect)]
#[reflect(Component, Deserialize)]
pub struct XOn(pub String);
#[derive(Component, Serialize, Deserialize, Default, Debug, Clone, Reflect)]
#[reflect(Component, Deserialize)]
pub struct XFunction(pub String);

fn button_swap_system(
    world: &mut World,
) {
    world.resource_scope(|world, named_system_registry: Mut<NamedSystemRegistry>| {
        world.resource_scope(|world, mut html_scenes: Mut<Assets<HTMLScene>>| {
            let mut interaction_query = world.query_filtered::<
                (
                    Entity,
                    &Interaction,
                    &Parent,
                    &XFunction,
                    Option<&XSwap>,
                    Option<&XTarget>
                ),
                (Changed<Interaction>, With<Button>),
            >();
            let mut name_query = world.query::<(Entity, &Name)>();

            let mut to_apply: Vec<(Entity, Entity, XSwap, XTarget, String)> = Vec::new();
            for (entity, interaction, parent, func, swap, target) in interaction_query.iter(world) {
                if matches!(interaction, Interaction::Pressed) {
                    to_apply.push((entity, parent.get(), swap.cloned().unwrap_or_default(), target.cloned().unwrap_or_default(), func.0.clone()));
                }
            }
            for (entity, _parent, swap, target, func) in to_apply {
                let xs = named_system_registry.call::<(), HTMLScene>(world, func.as_str(), ()).unwrap();

                let entity = match target {
                    XTarget::This => entity,
                    XTarget::Name(name) => name_query.iter(world).find(|(_, n)| n.as_str() == name).unwrap().0,
                    _ => unimplemented!()
                };
                match swap {
                    XSwap::Outer => {
                        world.entity_mut(entity)
                            .despawn_descendants()
                            .insert(html_scenes.add(xs));
                    },
                    XSwap::Inner => {
                        let child = world.spawn_empty()
                            .insert(html_scenes.add(xs))
                            .id();
                        world.entity_mut(entity)
                            .despawn_descendants()
                            .add_child(child);
                    },
                    XSwap::Back => {
                        let child = world.spawn_empty()
                            .insert(html_scenes.add(xs))
                            .id();
                        world.entity_mut(entity)
                            .push_children(&[child]);
                    },
                    XSwap::Front => {
                        let child = world.spawn_empty()
                            .insert(html_scenes.add(xs))
                            .id();
                        world.entity_mut(entity)
                            .insert_children(0, &[child]);
                    }
                }
            }
        });
    });
}

pub struct XPlugin;
impl Plugin for XPlugin {
    fn build(&self, app: &mut App) {
        app
            .register_type::<XSwap>()
            .register_type::<XTarget>()
            .register_type::<XFunction>()
            
            .add_systems(PreUpdate, button_swap_system.before(spawn_scene_system));
    }
}