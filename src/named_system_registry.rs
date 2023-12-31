use std::{collections::HashMap, any::TypeId};

use bevy::{ecs::{system::{Resource, IntoSystem, System}, world::World, entity::Entity, component::Component}, app::{Plugin, App}, reflect::Reflect};

pub trait NamedSystemRegistryExt {
    fn register_named_system<M, S, In, Out>(&mut self, name: impl AsRef<str>, system: S) -> &mut Self
    where S: IntoSystem<In, Out, M> + 'static, In: Reflect + 'static, Out: Reflect + 'static;
}
impl NamedSystemRegistryExt for App {
    fn register_named_system<M, S, In, Out>(&mut self, name: impl AsRef<str>, system: S) -> &mut Self 
        where S: IntoSystem<In, Out, M> + 'static, In: Reflect + 'static, Out: Reflect + 'static {

        let mut system = IntoSystem::into_system(system);
        system.initialize(&mut self.world);
        let entity = self.world.spawn_empty()
            .insert(NamedSystem::<In, Out>(Some(Box::new(system))))
            .insert(CallNamedSystem(
                Some(Box::new(
                    |world, entity, in_ref| {
                        // Take system out
                        let mut sys = world.entity_mut(entity)
                            .get_mut::<NamedSystem<In, Out>>()
                            .unwrap().0.take().unwrap();
                        // Call system with world access
                        let res_ref = Box::new(sys.run(*in_ref.downcast().unwrap(), world));
                        // Put system back in
                        world.entity_mut(entity)
                            .get_mut::<NamedSystem<In, Out>>()
                            .unwrap().0 = Some(sys);

                        res_ref
                    }
                ))
            )).id();

        let mut named_system_reg = self.world.resource_mut::<NamedSystemRegistry>();
        (*named_system_reg).systems.insert(
            name.as_ref().to_string(),
            (entity, TypeId::of::<In>(), TypeId::of::<Out>())
        );

        self
    }
}

#[derive(Component)]
struct NamedSystem<In, Out>(Option<Box<dyn System<In = In, Out = Out>>>);
#[derive(Component)]
struct CallNamedSystem(Option<Box<dyn Fn(&mut World, Entity, Box<dyn Reflect>) -> Box<dyn Reflect> + Send + Sync>>);

#[derive(Resource, Default)]
pub struct NamedSystemRegistry {
    systems: HashMap<String, (Entity, TypeId, TypeId)>
}
impl NamedSystemRegistry {
    pub fn call_reflect(&self, world: &mut World, name: &str, in_ref: Box<dyn Reflect>) -> Option<Box<dyn Reflect>> {
        let entity = self.systems.get(name)?.0;

        // Take function out
        let f = world.entity_mut(entity).get_mut::<CallNamedSystem>()?.0.take().unwrap();

        let res_ref = f(world, entity, in_ref);

        // Put function back in
        world.entity_mut(entity).get_mut::<CallNamedSystem>()?.0 = Some(f);

        Some(res_ref)
    }
    pub fn call<In: Reflect, Out: Reflect>(&self, world: &mut World, name: &str, in_val: In) -> Option<Out> {
        let in_ref: Box<dyn Reflect> = Box::new(in_val);
        let res_ref = self.call_reflect(world, name, in_ref)?;
        res_ref.downcast().ok().map(|x| *x)
    }
    pub fn get_type_ids(&self, name: &str) -> Option<(TypeId, TypeId)> {
        let x = self.systems.get(name)?;
        Some((x.1, x.2))
    }
}

pub struct NamedSystemRegistryPlugin;
impl Plugin for NamedSystemRegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NamedSystemRegistry>();
    }
}