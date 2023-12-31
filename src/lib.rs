use std::{thread::spawn, collections::HashMap};

use bevy::{prelude::*, render::render_resource::DynamicUniformBuffer, reflect::{TypeInfo, TypeRegistry, TypeRegistration, ReflectMut, DynamicEnum, DynamicVariant, DynamicTuple, DynamicStruct, DynamicTupleStruct}, scene::DynamicEntity, app::ScheduleRunnerPlugin, pbr::PBR_TYPES_SHADER_HANDLE, ecs::{reflect::ReflectCommandExt, system::{EntityCommands, SystemId}}, ui::FocusPolicy, a11y::Focus};
use html_parser::Dom;
use maud::{html, Markup};
use named_system_registry::{NamedSystemRegistryPlugin, NamedSystemRegistry};
use ron::{Options, extensions::Extensions, Value, Map, value::RawValue};
use serde::{Serialize, Deserialize};
use thiserror::Error;

mod htmx;
use htmx::*;
mod named_system_registry;
pub use named_system_registry::NamedSystemRegistryExt;

#[derive(Asset, Reflect, Debug, Clone)]
pub struct HTMLScene(#[reflect(ignore)] Dom);
#[reflect_trait]
pub trait Template {
    fn template(&self) -> HTMLScene;
}

impl From<Markup> for HTMLScene {
    fn from(value: Markup) -> Self {
        HTMLScene(Dom::parse(&value.into_string()).unwrap())
    }
}
impl TryFrom<&str> for HTMLScene {
    type Error = html_parser::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(HTMLScene(Dom::parse(value)?))
    }
}
impl TryFrom<String> for HTMLScene {
    type Error = html_parser::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(HTMLScene(Dom::parse(&value)?))
    }
}

#[derive(Reflect, Default)]
#[reflect(Template)]
#[reflect(Default)]
struct NodeTemplate;
impl Template for NodeTemplate {
    fn template(&self) -> HTMLScene {
        html! {
            Node
            Style BackgroundColor BorderColor
            FocusPolicy Transform GlobalTransform Visibility InheritedVisibility ViewVisibility ZIndex { }
        }.into()
    }
}
#[derive(Reflect, Default)]
#[reflect(Template)]
#[reflect(Default)]
struct TextTemplate;
impl Template for TextTemplate {
    fn template(&self) -> HTMLScene {
        html! {
            NodeTemplate
            Text TextLayoutInfo TextFlags ContentSize { }
        }.into()
    }
}

#[derive(Error, Debug)]
pub enum HTMLSceneSpawnError {
    #[error("Attribute name [{0}]: Failed to deserialize")]
    DeserializationFailed(String),
    #[error("Attribute name [{0}]: Invalid attribute associated type <{1}>")]
    InvalidAttributeAssociatedType(String, String),
    #[error("Attribute name [{0}]: Component doesn't implement/reflect Default")]
    NoDefault(String),
    #[error("Attribute name [{0}]: Component doesn't implement/reflect Deserialize, and you are trying to assign a value")]
    NoDeserialize(String),
    #[error("Attribute name [{0}]: Attempting to patch a non-struct component")]
    PatchNonStruct(String),
}

fn construct_instance(world: &mut World, type_registry: &TypeRegistry, named_system_registry: &NamedSystemRegistry, key_type: &TypeRegistration, value: Option<&str>) -> Result<Box<dyn Reflect>, HTMLSceneSpawnError> {
    let ron_options = Options::default().with_default_extension(Extensions::UNWRAP_NEWTYPES);

    let instance: Option<Box<dyn Reflect>> = if value.is_some() {
        let decoded_html_string = html_escape::decode_html_entities(value.unwrap());

        // Recreate our deserializer
        let mut ron_de = ron::Deserializer::from_str_with_options(
            &decoded_html_string, &ron_options
        ).expect("Couldn't construct RON deserializer");

        match key_type.type_info() {
            TypeInfo::Struct(info) => {
                // Deserialize to a HashMap of RawValue
                let values: HashMap<&RawValue, &RawValue> = HashMap::deserialize(&mut ron_de).unwrap();
                let mut ref_struct = DynamicStruct::default();
                ref_struct.set_represented_type(Some(key_type.type_info()));
    
                for (k, v) in values.into_iter() {
                    let field_name = k.get_ron().trim();
                    if let Some(field_info) = info.field(field_name) {
                        ref_struct.insert_boxed(
                            field_name,
                            construct_instance(world, type_registry, named_system_registry, type_registry.get(field_info.type_id()).unwrap(), Some(v.get_ron()))?
                        );
                    }
                }
    
                Some(Box::new(ref_struct))
            },
            TypeInfo::TupleStruct(info) => {
                // Deserialize to a vec of RawValue, or if we can't deserialize assume length one
                let values: Vec<&RawValue> = Vec::deserialize(&mut ron_de)
                    .unwrap_or_else(|_| {
                        vec![RawValue::from_ron(&decoded_html_string).unwrap()]
                    });
                let mut ref_tuple = DynamicTupleStruct::default();
                ref_tuple.set_represented_type(Some(key_type.type_info()));
    
                for (i, v) in values.into_iter().enumerate() {
                    if let Some(field_info) = info.field_at(i) {
                        ref_tuple.insert_boxed(
                            construct_instance(world, type_registry, named_system_registry, type_registry.get(field_info.type_id()).unwrap(), Some(v.get_ron()))?
                        );
                    }
                }
    
                Some(Box::new(ref_tuple))
            },
            _ => {
                let mut ron_de = ron::Deserializer::from_str_with_options(&decoded_html_string, &ron_options)
                    .map_err(|_| HTMLSceneSpawnError::DeserializationFailed(key_type.type_info().type_path().to_string()))?;

                let deserialized = type_registry.get_type_data::<ReflectDeserialize>(key_type.type_id())
                    .and_then(|deserializer| deserializer.deserialize(&mut ron_de).ok());

                if let Some(deserialized) = deserialized{
                    // If there was a deserializer and we could deserialize, go with that
                    Some(deserialized)
                } else {
                    // Otherwise, see if there's a constructor function referenced here
                    let (constructor_fun, val) = <(&RawValue, &RawValue)>::deserialize(&mut ron_de).expect("Failed to find constructor function");
                    let constructor_fun = constructor_fun.get_ron().trim().to_string();

                    if let Some((in_type, _)) = named_system_registry.get_type_ids(&constructor_fun) {
                        let mut ron_de = ron::Deserializer::from_str_with_options(val.get_ron(), &ron_options)
                            .map_err(|_| HTMLSceneSpawnError::DeserializationFailed(key_type.type_info().type_path().to_string()))?;
                        let deserializer = type_registry.get_type_data::<ReflectDeserialize>(in_type).unwrap();
                        let in_val = deserializer.deserialize(&mut ron_de).expect("Failed to deserialize constructor function input type");

                        named_system_registry.call_reflect(world, &constructor_fun, in_val)
                    } else {
                        panic!("Attempted to use invalid/unregistered constructor function")
                    }
                }
            }
        }
    } else {
        None
    };

    let instance: Box<dyn Reflect> = match (instance, type_registry.get_type_data::<ReflectDefault>(key_type.type_id())) {
        // Get the default instance... or
        (Some(instance), Some(default_impl)) => { let mut d = default_impl.default(); d.apply(&*instance); d },
        (None, Some(default_impl)) => default_impl.default(),
        (Some(instance), None) => instance,
        (None, None) => panic!(),
    };

    Ok(instance)
}

fn spawn_scene(
    scene: &HTMLScene, replace: Entity,
    type_registry: &TypeRegistry, named_system_registry: &NamedSystemRegistry,
    world: &mut World
) -> Result<(), HTMLSceneSpawnError> {
    fn helper(
        html_el: &html_parser::Element,
        type_registry: &TypeRegistry, named_system_registry: &NamedSystemRegistry,
        commands: &mut EntityWorldMut
    ) -> Result<(), HTMLSceneSpawnError> {
        let mut text_style = TextStyle::default();

        for (attribute, value) in std::iter::once((&html_el.name, None)).chain(html_el.attributes.iter().map(|x| (x.0, x.1.as_ref()))) {
            // FIXME: Hardcode some attributes which can't be constructed at runtime
            // Ideally these attributes should implement ReflectDefault and/or ReflectDeserialize
            match attribute.as_str() {
                "Entity" => { continue; }, // 'Null' attribute
                "ZIndex" => { commands.insert(ZIndex::default()); continue; },
                "FocusPolicy" if value.is_none() => { commands.insert(FocusPolicy::default()); continue; },
                "TextStyle" if value.is_some() => {
                    let (font_size, color) = <(f32, Color)>::deserialize(&mut ron::Deserializer::from_str(value.unwrap()).unwrap()).unwrap_or_default();
                    text_style.font_size = font_size;
                    text_style.color = color;
                    continue;
                },
                _ => ()
            }

            let attribute_reg: &TypeRegistration = type_registry
                .get_with_short_type_path(&attribute)
                .expect(&format!("Attribute name [{}]: Referred to undefined component", &attribute));

            let instance = commands.world_scope(|world| {
                construct_instance(world, type_registry, named_system_registry, attribute_reg, value.map(|x| x.as_str()))
            })?;

            let template: Option<&dyn Template> = type_registry.get_type_data::<ReflectTemplate>(attribute_reg.type_id())
                .and_then(|x| x.get(&*instance));

            // If this component is actually a template, instead of a component
            if let Some(template) = template {
                let template = template.template().0;
                // Recurse with the template's XML
                helper(&template.children.first().unwrap().element().unwrap(), type_registry, named_system_registry, commands)?;
            } else {
                // Otherwise insert our component
                let reflect_component = type_registry.get_with_type_path(instance.get_represented_type_info().unwrap().type_path())
                    .unwrap().data::<ReflectComponent>().unwrap();
                reflect_component.insert(commands, &*instance);
            }
        }
        if let Some(id) = html_el.id.as_ref() {
            commands.insert(Name::from(id.as_str()));
        }

        for child in &html_el.children {
            if let Some(text) = child.text() {
                commands.insert(Text::from_section(text, text_style));
                break;
            }
        }
        
        let children = commands.world_scope(|world| {
            let mut children = Vec::new();
            for child in &html_el.children {
                if let html_parser::Node::Element(child) = child {
                    let mut child_entity = world.spawn_empty();
                    children.push(child_entity.id());
                    helper(&child, type_registry, named_system_registry, &mut child_entity)?;
                }
            }
            Ok(children)
        })?;
        commands.push_children(children.as_slice());
        Ok(())
    }

    let mut child = world.entity_mut(replace);
    helper(
        &scene.0.children.first().expect("HTMLScene has no children").element().expect("HTMLScene first child is not an element"),
        type_registry, named_system_registry,
    &mut child
    )
}

pub(crate) fn spawn_scene_system(
    world: &mut World,
) {
    world.resource_scope(|world, type_registry: Mut<AppTypeRegistry>| {
        world.resource_scope(|world, html_scenes: Mut<Assets<HTMLScene>>| {
            world.resource_scope(|world, named_system_registry: Mut<NamedSystemRegistry>| {
                let mut to_spawn = world.query::<(Entity, Option<&Parent>, &Handle<HTMLScene>)>();
                let mut children = world.query::<&Children>();

                for (entity, parent, handle) in to_spawn
                    .iter(world)
                    .map(|(a,b,c)| (a, b.map(|p| p.get()), c.clone()))
                    .collect::<Vec<_>>()
                {
                    let scene = html_scenes.get(handle).unwrap();
                    let entity = if let Some(parent) = parent {
                        let idx = children.get(world, parent).unwrap().iter().position(|&x| x == entity).unwrap();
            
                        world.entity_mut(entity).despawn_recursive();
                        let entity = world.spawn_empty().id();
                        world.entity_mut(parent).insert_children(idx, &[entity]);

                        entity
                    } else {
                        world.entity_mut(entity).despawn();
                        world.spawn_empty().id()
                    };

                    spawn_scene(
                        scene, entity,
                        &type_registry.0.read(), &named_system_registry,
                        world
                    ).expect("Failed to spawn HTMLScene!");
                }
            });
        });
    });
}

fn hex_to_color(hex: In<String>) -> Color {
    Color::hex(hex.0).unwrap()
}
fn path_to_handle(i: In<(String, String)>, asset_server: Res<AssetServer>, type_registry: Res<AppTypeRegistry>) -> Handle<Image> {
    let (handle_type, path) = i.0;
    asset_server.load(path)
}

pub struct HTMLPlugin;
impl Plugin for HTMLPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(NamedSystemRegistryPlugin)
            .add_plugins(XPlugin)

            .register_named_system("hex_to_color", hex_to_color)
            .register_named_system("path_to_handle", path_to_handle)

            .init_asset::<HTMLScene>()

            .register_type::<NodeTemplate>()
            .register_type_data::<NodeTemplate, ReflectTemplate>()
            .register_type_data::<NodeTemplate, ReflectDefault>()
            .register_type::<TextTemplate>()
            .register_type_data::<TextTemplate, ReflectTemplate>()
            .register_type_data::<TextTemplate, ReflectDefault>()

            .register_type::<(String, String)>()
            .register_type_data::<(String, String), ReflectDeserialize>()

            .add_systems(PreUpdate, spawn_scene_system);
    }
}