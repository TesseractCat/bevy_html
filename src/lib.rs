use std::{thread::spawn, collections::HashMap, borrow::Cow};

use bevy::{prelude::*, render::render_resource::DynamicUniformBuffer, reflect::{TypeInfo, TypeRegistry, TypeRegistration, ReflectMut, DynamicEnum, DynamicVariant, DynamicTuple, DynamicStruct, DynamicTupleStruct, TypeRegistryArc, FromType, EnumInfo, VariantInfo, serde::{UntypedReflectDeserializer, TypedReflectDeserializer}}, scene::DynamicEntity, app::ScheduleRunnerPlugin, pbr::PBR_TYPES_SHADER_HANDLE, ecs::{reflect::ReflectCommandExt, system::{EntityCommands, SystemId}}, ui::FocusPolicy, a11y::Focus};
use bevy::reflect::erased_serde;
use html_parser::Dom;
use maud::{html, Markup};
use named_system_registry::{NamedSystemRegistryPlugin, NamedSystemRegistry};
use ron::{Options, extensions::Extensions, Value, Map, value::RawValue};
use serde::{Serialize, Deserialize, Deserializer, de::{Visitor, DeserializeSeed}};
use thiserror::Error;

mod htmx;
use htmx::*;
mod named_system_registry;
pub use named_system_registry::NamedSystemRegistryExt;

mod typed_partial_reflect_deserializer;
use typed_partial_reflect_deserializer::*;

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
#[reflect(Template, Default)]
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
#[reflect(Template, Default)]
struct TextTemplate;
impl Template for TextTemplate {
    fn template(&self) -> HTMLScene {
        html! {
            NodeTemplate
            Text TextLayoutInfo TextFlags ContentSize { }
        }.into()
    }
}

pub trait Construct
    where Self::In: Reflect + for<'de> Deserialize<'de> + 'static {
    type In;
    fn construct(world: &mut World, data: Self::In) -> Option<Self>
        where Self: Sized;
}
#[derive(Clone)]
pub struct ReflectConstruct {
    pub func: fn(
        world: &mut World, deserializer: &mut dyn erased_serde::Deserializer
    ) -> Option<Box<dyn Reflect>>
}
impl<T: Construct + Reflect> FromType<T> for ReflectConstruct {
    fn from_type() -> Self {
        ReflectConstruct {
            func: |world, mut deserializer: &mut dyn erased_serde::Deserializer| {
                let data = T::In::deserialize(deserializer).ok()?;
                let constructed = T::construct(world, data)?;
                Some(Box::new(constructed))
            }
        }
    }
}
impl ReflectConstruct {
    pub fn construct(&self, world: &mut World, deserializer: &mut dyn erased_serde::Deserializer) -> Option<Box<dyn Reflect>> {
        (self.func)(world, deserializer)
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

fn construct_instance(world: &mut World, type_registry: &TypeRegistry, key_type: &TypeRegistration, value: Option<&str>) -> Result<Box<dyn Reflect>, HTMLSceneSpawnError> {
    let ron_options = Options::default();//.with_default_extension(Extensions::UNWRAP_NEWTYPES);

    let default_impl = type_registry.get_type_data::<ReflectDefault>(key_type.type_id());

    let instance: Option<Box<dyn Reflect>> = if value.is_some() {
        let decoded_html_string = html_escape::decode_html_entities(value.unwrap());

        // Wrap structs in parens for convenience
        let decoded_html_string = match key_type.type_info() {
            TypeInfo::Struct(_) | TypeInfo::TupleStruct(_) => {
                Cow::Owned(format!("({})", decoded_html_string))
            },
            _ => decoded_html_string
        };

        let mut ron_de = ron::Deserializer::from_str_with_options(
            &decoded_html_string, &ron_options
        ).expect("Couldn't construct RON deserializer");

        let deserialized: Box<dyn Reflect> = DeserializeSeed::deserialize(
            TypedPartialReflectDeserializer::new(world, key_type, type_registry, default_impl.is_none()),
            &mut ron_de
        ).unwrap();
        Some(deserialized)
    } else {
        None
    };

    let instance: Box<dyn Reflect> = match (instance, default_impl) {
        // Get the default instance... or
        (Some(instance), Some(default_impl)) => { let mut d = default_impl.default(); d.apply(&*instance); d },
        (None, Some(default_impl)) => default_impl.default(),
        (Some(instance), None) => instance,
        (None, None) => panic!(),
    };

    Ok(instance)
}

fn spawn_scene(
    scene: &HTMLScene, replace: Entity, world: &mut World
) -> Result<(), HTMLSceneSpawnError> {
    fn helper(
        html_el: &html_parser::Element, commands: &mut EntityWorldMut
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

            let type_registry_arc = commands.world().resource::<AppTypeRegistry>().0.clone();
            let type_registry = type_registry_arc.read();
            let attribute_reg: &TypeRegistration = type_registry
                .get_with_short_type_path(&attribute)
                .expect(&format!("Attribute name [{}]: Referred to undefined component", &attribute));

            let instance = commands.world_scope(|world| {
                construct_instance(world, &type_registry, attribute_reg, value.map(|x| x.as_str()))
            })?;

            let template: Option<&dyn Template> = commands.world().resource::<AppTypeRegistry>().0.read()
                .get_type_data::<ReflectTemplate>(attribute_reg.type_id())
                .and_then(|x| x.get(&*instance));

            // If this component is actually a template, instead of a component
            if let Some(template) = template {
                let template = template.template().0;
                // Recurse with the template's XML
                helper(&template.children.first().unwrap().element().unwrap(), commands)?;
            } else {
                // Otherwise insert our component
                let reflect_component = type_registry
                    .get_with_type_path(instance.get_represented_type_info().unwrap().type_path())
                    .expect(&format!("Attribute name [{attribute}]: Not registered in TypeRegistry"))
                    .data::<ReflectComponent>().unwrap();
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
                    helper(&child, &mut child_entity)?;
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
        &mut child
    )
}

pub(crate) fn spawn_scene_system(
    world: &mut World,
) {
    world.resource_scope(|world, html_scenes: Mut<Assets<HTMLScene>>| {
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

            spawn_scene(scene, entity, world).expect("Failed to spawn HTMLScene!");
        }
    });
}

impl<T: Asset> Construct for Handle<T> {
    type In = String;
    fn construct(world: &mut World, data: Self::In) -> Option<Self> {
        let asset_server = world.resource_mut::<AssetServer>();
        Some(asset_server.load(data.to_string()))
    }
}
impl Construct for Color {
    type In = String;
    fn construct(world: &mut World, data: Self::In) -> Option<Self> {
        Color::hex(&data).ok()
    }
}
impl Construct for UiRect {
    type In = (Val, Val, Val, Val);
    fn construct(world: &mut World, data: Self::In) -> Option<Self> {
        Some(UiRect::new(data.0, data.1, data.2, data.3))
    }
}

pub struct HTMLPlugin;
impl Plugin for HTMLPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(NamedSystemRegistryPlugin)
            .add_plugins(XPlugin)

            .init_asset::<HTMLScene>()

            .register_type::<NodeTemplate>()
            .register_type::<TextTemplate>()

            .register_type::<(String, String)>()
            .register_type_data::<(String, String), ReflectDeserialize>()

            .register_type_data::<Handle<Image>, ReflectConstruct>()
            .register_type_data::<Color, ReflectConstruct>()
            .register_type_data::<UiRect, ReflectConstruct>()

            .add_systems(PreUpdate, spawn_scene_system);
    }
}