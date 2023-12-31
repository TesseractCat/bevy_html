use bevy::ecs::system::{RunSystemOnce, BoxedSystem, Command};
use bevy::prelude::*;
use bevy::ui::UiStack;
use bevy::{ecs::component::Component, reflect::ReflectDeserialize};
use bevy::ecs::reflect::ReflectComponent;
use bevy::reflect::std_traits::ReflectDefault;
use bevy::reflect::Reflect;
use serde::{Serialize, Deserialize};

use crate::{HTMLScene, named_system_registry, spawn_scene_system};
use crate::named_system_registry::NamedSystemRegistry;

#[derive(Component, Serialize, Deserialize, Default, Debug, Clone, Reflect)]
#[reflect(Component, Deserialize, Default)]
pub enum XSwap {
    #[default]
    Outer,
    Inner,
    Id(String),
    Root
}
#[derive(Component, Serialize, Deserialize, Default, Debug, Clone, Reflect)]
#[reflect(Component, Deserialize)]
pub struct XSwapOn(pub String);
#[derive(Component, Serialize, Deserialize, Default, Debug, Clone, Reflect)]
#[reflect(Component, Deserialize)]
pub struct XFunction(pub String);

fn button_swap_system(
    world: &mut World,
) {
    world.resource_scope(|world, mut named_system_registry: Mut<NamedSystemRegistry>| {
        world.resource_scope(|world, mut html_scenes: Mut<Assets<HTMLScene>>| {
            let mut interaction_query = world.query_filtered::<
                (
                    Entity,
                    &Interaction,
                    &Parent,
                    &XFunction,
                    Option<&XSwap>
                ),
                (Changed<Interaction>, With<Button>),
            >();
            let mut queue = bevy::ecs::system::CommandQueue::default();

            let mut to_apply: Vec<(Entity, Entity, XSwap, String)> = Vec::new();
            for (entity, interaction, parent, func, swap) in interaction_query.iter(world) {
                if matches!(interaction, Interaction::Pressed) {
                    to_apply.push((entity, parent.get(), swap.cloned().unwrap_or_default(), func.0.clone()));
                }
            }
            for (entity, parent, swap, func) in to_apply {
                let xs = named_system_registry.call::<(), HTMLScene>(world, func.as_str(), ()).unwrap();

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
                    _ => unimplemented!()
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
            .register_type_data::<XSwap, ReflectComponent>()
            .register_type_data::<XSwap, ReflectDeserialize>()

            .register_type::<XFunction>()
            .register_type_data::<XFunction, ReflectComponent>()
            .register_type_data::<XFunction, ReflectDeserialize>()
            
            .add_systems(PreUpdate, button_swap_system.before(spawn_scene_system));
    }
}