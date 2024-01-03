use std::{borrow::Cow, fmt::Display};

use bevy::{prelude::*, reflect::{TypeInfo, TypeRegistry, TypeRegistration, FromType}, gltf::Gltf, asset::{AssetLoader, AsyncReadExt}};
use bevy::reflect::erased_serde;
use html_parser::Dom;
use maud::{html, Markup, PreEscaped};
use named_system_registry::NamedSystemRegistryPlugin;
use ron::Options;
use serde::{Deserialize, de::DeserializeSeed};
use thiserror::Error;

pub mod htmx;
use htmx::*;
mod named_system_registry;
pub use named_system_registry::{NamedSystemRegistryExt, NamedSystemRegistry};

mod typed_partial_reflect_deserializer;
use typed_partial_reflect_deserializer::*;

#[derive(Asset, Reflect, Debug, Clone)]
pub struct HTMLScene(#[reflect(ignore)] String, #[reflect(ignore)] Dom);
impl HTMLScene {
    fn dom(&self) -> &Dom {
        &self.1
    }
}
impl Display for HTMLScene {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl maud::Render for HTMLScene {
    fn render(&self) -> Markup {
        PreEscaped(self.0.clone())
    }
}

impl From<Markup> for HTMLScene {
    fn from(value: Markup) -> Self {
        HTMLScene(value.clone().into_string(), Dom::parse(&value.into_string()).unwrap())
    }
}
impl TryFrom<&str> for HTMLScene {
    type Error = html_parser::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(HTMLScene(value.to_string(), Dom::parse(value)?))
    }
}
impl TryFrom<String> for HTMLScene {
    type Error = html_parser::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(HTMLScene(value.clone(), Dom::parse(&value)?))
    }
}

#[derive(Default)]
pub struct HTMLSceneAssetLoader;
impl AssetLoader for HTMLSceneAssetLoader {
    type Asset = HTMLScene;
    type Settings = ();
    type Error = html_parser::Error;

    fn load<'a>(
            &'a self,
            reader: &'a mut bevy::asset::io::Reader,
            settings: &'a Self::Settings,
            load_context: &'a mut bevy::asset::LoadContext,
        ) -> bevy::utils::BoxedFuture<'a, Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).await?;
            let str = std::str::from_utf8(bytes.as_slice()).unwrap();
            HTMLScene::try_from(str)
        })
    }

    fn extensions(&self) -> &[&str] {
        &["html"]
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
            func: |world, deserializer: &mut dyn erased_serde::Deserializer| {
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
    #[error("Unrecognized tag name {0}")]
    UnrecognizedTagName(String)
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

#[derive(Default, Reflect)]
struct InterimTextStyle {
    size: f32, color: Color, font: Handle<Font>
}
fn spawn_scene(
    scene: &HTMLScene, replace: Entity, world: &mut World
) -> Result<(), HTMLSceneSpawnError> {
    fn helper(
        html_el: &html_parser::Element, commands: &mut EntityWorldMut
    ) -> Result<(), HTMLSceneSpawnError> {
        let mut text_style = TextStyle::default();

        // If there's a registered template function
        if let Some(template) = commands.world_scope(|world| {
            world.resource_scope(|world, named_system_registry: Mut<NamedSystemRegistry>| {
                named_system_registry.call::<(), HTMLScene>(world, &html_el.name, ())
            })
        }) {
            // Recurse with the template's XML
            helper(&template.dom().children.first().unwrap().element().unwrap(), commands)?;
        } else if html_el.name != "Entity" { // Null tag
            return Err(HTMLSceneSpawnError::UnrecognizedTagName(html_el.name.to_string()));
        }

        for (attribute, value) in html_el.attributes.iter().map(|x| (x.0, x.1.as_ref())) {
            let type_registry_arc = commands.world().resource::<AppTypeRegistry>().0.clone();
            let type_registry = type_registry_arc.read();

            match attribute.as_str() {
                "TextStyle" if value.is_some() => {
                    let wrapped_value = format!("({})", html_escape::decode_html_entities(value.unwrap()));
                    let mut ron_de = ron::Deserializer::from_str(&wrapped_value).unwrap();
                    let mut t = InterimTextStyle::default();
                    t.apply(&*commands.world_scope(|world| {
                        TypedPartialReflectDeserializer::new(world,
                            type_registry.get(std::any::TypeId::of::<InterimTextStyle>()).unwrap(),
                            &type_registry,
                            false
                        ).deserialize(&mut ron_de).unwrap()
                    }));
                    text_style.font_size = t.size;
                    text_style.color = t.color;
                    text_style.font = t.font;
                    continue;
                },
                _ => ()
            }

            // Allow for generic types
            let attribute = if let Some((attribute, attribute_type)) = attribute.split_once(":") {
                format!("{attribute}<{attribute_type}>")
            } else {
                attribute.to_string()
            };

            let attribute_reg: &TypeRegistration = type_registry
                .get_with_short_type_path(&attribute)
                .expect(&format!("Attribute name [{}]: Referred to undefined component", &attribute));

            let instance = commands.world_scope(|world| {
                construct_instance(world, &type_registry, attribute_reg, value.map(|x| x.as_str()))
            })?;

            // Insert our component
            let reflect_component = type_registry
                .get_with_type_path(instance.get_represented_type_info().unwrap().type_path())
                .expect(&format!("Attribute name [{attribute}]: Not registered in TypeRegistry"))
                .data::<ReflectComponent>().unwrap();
            reflect_component.insert(commands, &*instance);
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
        &scene.dom().children.first().expect("HTMLScene has no children").element().expect("HTMLScene first child is not an element"),
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
    fn construct(_world: &mut World, data: Self::In) -> Option<Self> {
        let c = csscolorparser::parse(&data).ok()?;
        Some(Color::Rgba {
            red: c.r as f32, green: c.g as f32, blue: c.b as f32, alpha: c.a as f32
        })
    }
}
impl Construct for UiRect {
    type In = (Val, Val, Val, Val);
    fn construct(_world: &mut World, data: Self::In) -> Option<Self> {
        Some(UiRect::new(data.0, data.1, data.2, data.3))
    }
}

fn node() -> HTMLScene {
    html! {
        Entity Node
        Style
        BackgroundColor="\"transparent\"" BorderColor
        Transform GlobalTransform
        Visibility InheritedVisibility ViewVisibility
        FocusPolicy="Pass" ZIndex="Local(0)" { }
    }.into()
}
fn text() -> HTMLScene {
    html! {
        Node ContentSize Text TextLayoutInfo TextFlags { }
    }.into()
}
fn image() -> HTMLScene {
    html! {
        Node ContentSize UiImage UiImageSize { }
    }.into()
}
fn button() -> HTMLScene {
    html! {
        Node Button Interaction="None" { }
    }.into()
}

pub struct HTMLPlugin;
impl Plugin for HTMLPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(NamedSystemRegistryPlugin)
            .add_plugins(XPlugin)

            .init_asset::<HTMLScene>()
            .init_asset_loader::<HTMLSceneAssetLoader>()

            .register_type::<InterimTextStyle>()
            .register_type::<(String, String)>()
            .register_type_data::<(String, String), ReflectDeserialize>()

            .register_type_data::<Handle<Image>, ReflectConstruct>()
            .register_type_data::<Handle<Font>, ReflectConstruct>()
            .register_type_data::<Handle<Gltf>, ReflectConstruct>()
            .register_type_data::<Handle<AudioSource>, ReflectConstruct>()
            .register_type_data::<Handle<Scene>, ReflectConstruct>()
            .register_type_data::<Handle<HTMLScene>, ReflectConstruct>()
            .register_type_data::<Color, ReflectConstruct>()
            .register_type_data::<UiRect, ReflectConstruct>()

            .register_named_system("Node", node)
            .register_named_system("Text", text)
            .register_named_system("Image", image)
            .register_named_system("Button", button)

            .add_systems(PreUpdate, spawn_scene_system);
    }
}