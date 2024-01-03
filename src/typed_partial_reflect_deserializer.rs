// A variant of TypedReflectDeserializer that deserializes to entirely dynamic reflect types which can then be applied, also checks for ReflectConstruct impls

use std::{marker::PhantomData, fmt::{self, Formatter}};

use bevy::{reflect::{TypeRegistration, TypeRegistry, Reflect, TypeInfo, DynamicStruct, StructInfo, ReflectDeserialize, DynamicTupleStruct, Enum, TupleStructInfo, DynamicEnum, EnumInfo, VariantInfo, DynamicVariant, DynamicTuple, StructVariantInfo, UnnamedField, TupleVariantInfo, TupleInfo, NamedField, Tuple}, scene::DynamicEntity, ecs::world::World};
use bevy::reflect::erased_serde;
use serde::{de::{Visitor, SeqAccess, MapAccess, DeserializeSeed, Error, EnumAccess, VariantAccess, IntoDeserializer}, Deserialize, Deserializer};
use std::collections::HashMap;

use crate::ReflectConstruct;

trait StructLikeInfo {
    fn get_path(&self) -> &str;
    fn get_field(&self, name: &str) -> Option<&NamedField>;
    fn field_at(&self, index: usize) -> Option<&NamedField>;
    fn get_field_len(&self) -> usize;
    fn iter_fields(&self) -> std::slice::Iter<'_, NamedField>;
}

trait TupleLikeInfo {
    fn get_path(&self) -> &str;
    fn get_field(&self, index: usize) -> Option<&UnnamedField>;
    fn get_field_len(&self) -> usize;
}

impl StructLikeInfo for StructInfo {
    fn get_path(&self) -> &str {
        self.type_path()
    }

    fn field_at(&self, index: usize) -> Option<&NamedField> {
        self.field_at(index)
    }

    fn get_field(&self, name: &str) -> Option<&NamedField> {
        self.field(name)
    }

    fn get_field_len(&self) -> usize {
        self.field_len()
    }

    fn iter_fields(&self) -> std::slice::Iter<'_, NamedField> {
        self.iter()
    }
}

impl StructLikeInfo for StructVariantInfo {
    fn get_path(&self) -> &str {
        self.name()
    }

    fn field_at(&self, index: usize) -> Option<&NamedField> {
        self.field_at(index)
    }

    fn get_field(&self, name: &str) -> Option<&NamedField> {
        self.field(name)
    }

    fn get_field_len(&self) -> usize {
        self.field_len()
    }

    fn iter_fields(&self) -> std::slice::Iter<'_, NamedField> {
        self.iter()
    }
}

impl TupleLikeInfo for TupleInfo {
    fn get_path(&self) -> &str {
        self.type_path()
    }

    fn get_field(&self, index: usize) -> Option<&UnnamedField> {
        self.field_at(index)
    }

    fn get_field_len(&self) -> usize {
        self.field_len()
    }
}

impl TupleLikeInfo for TupleStructInfo {
    fn get_path(&self) -> &str {
        self.type_path()
    }

    fn get_field(&self, index: usize) -> Option<&UnnamedField> {
        self.field_at(index)
    }

    fn get_field_len(&self) -> usize {
        self.field_len()
    }
}

impl TupleLikeInfo for TupleVariantInfo {
    fn get_path(&self) -> &str {
        self.name()
    }

    fn get_field(&self, index: usize) -> Option<&UnnamedField> {
        self.field_at(index)
    }

    fn get_field_len(&self) -> usize {
        self.field_len()
    }
}

pub struct TypedPartialReflectDeserializer<'a> {
    set_represented_type: bool,
    registration: &'a TypeRegistration,
    registry: &'a TypeRegistry,
    world: &'a mut World,
}
impl<'a> TypedPartialReflectDeserializer<'a> {
    pub fn new(world: &'a mut World, registration: &'a TypeRegistration, registry: &'a TypeRegistry, set_represented_type: bool) -> Self {
        Self {
            set_represented_type,
            registration,
            registry,
            world,
        }
    }
}
impl<'a, 'de> DeserializeSeed<'de> for TypedPartialReflectDeserializer<'a> {
    type Value = Box<dyn Reflect>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de> {
        
        // Deserialize into intermediary value so we can attempt using the constructor
        // HACK: Really I should use serde-value for this, but for some reason Ron explicitly detects if
        //       deserializing into a serde::__private::de::Content and only then deserializes enums correctly.
        //       Also this is how serde does untagged enums, so should be the correct approach.
        let v = serde::__private::de::Content::deserialize(deserializer).unwrap();
        let deserializer: serde::__private::de::ContentDeserializer<'de, D::Error> = v.clone().into_deserializer();

        if let Some(construct_reflect) = self.registration.data::<ReflectConstruct>() {
            if let Some(value) = construct_reflect.construct(self.world, &mut <dyn erased_serde::Deserializer>::erase(deserializer)) {
                return Ok(value);
            }
            // If the constructor fails, fall through and try deserializing the regular way
        }
        
        let deserializer: serde::__private::de::ContentDeserializer<'de, D::Error> = v.clone().into_deserializer();
        match self.registration.type_info() {
            TypeInfo::Struct(info) => {
                let mut dynamic_struct = deserializer.deserialize_struct(
                    info.type_path_table().ident().unwrap(),
                    info.field_names(),
                    StructVisitor {
                        info,

                        set_represented_type: self.set_represented_type,
                        world: self.world,
                        registration: self.registration,
                        registry: self.registry,
                    },
                ).unwrap();
                if self.set_represented_type { dynamic_struct.set_represented_type(Some(self.registration.type_info())); }
                Ok(Box::new(dynamic_struct))
            },
            TypeInfo::TupleStruct(info) => {
                let mut dynamic_tuple_struct: DynamicTupleStruct = deserializer.deserialize_tuple_struct(
                    info.type_path_table().ident().unwrap(),
                    info.field_len(),
                    TupleVisitor {
                        info,

                        set_represented_type: self.set_represented_type,
                        world: self.world,
                        registration: self.registration,
                        registry: self.registry,
                    },
                ).unwrap().into();
                if self.set_represented_type { dynamic_tuple_struct.set_represented_type(Some(self.registration.type_info())); }
                Ok(Box::new(dynamic_tuple_struct))
            },
            TypeInfo::Enum(info) => {
                let mut dynamic_enum = match v {
                    serde::__private::de::Content::None => DynamicEnum::new("None", DynamicVariant::Unit),
                    _ => deserializer.deserialize_enum(
                        info.type_path_table().ident().unwrap(),
                        info.variant_names(),
                        EnumVisitor {
                            info,

                            set_represented_type: self.set_represented_type,
                            world: self.world,
                            registration: self.registration,
                            registry: self.registry,
                        }
                    ).unwrap()
                };
                if self.set_represented_type { dynamic_enum.set_represented_type(Some(self.registration.type_info())); }
                Ok(Box::new(dynamic_enum))
            },
            TypeInfo::Value(info) => {
                if let Some(deserialize_reflect) = self.registration.data::<ReflectDeserialize>() {
                    let value = deserialize_reflect.deserialize(deserializer).unwrap();
                    Ok(value)
                } else {
                    Err(Error::custom("Found value type with no deserializer/constructor"))
                }
            },
            _ => unimplemented!()
        }
    }
}

struct BorrowedStringVisitor;
impl<'de> Visitor<'de> for BorrowedStringVisitor {
    type Value = &'de str;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("raw value")
    }

    fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Ok(value)
    }

    fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>, {
        deserializer.deserialize_str(self)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Reflect)]
pub struct Ident(pub String);
impl<'de> Deserialize<'de> for Ident {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct StringVisitor;
        impl<'de> Visitor<'de> for StringVisitor {
            type Value = String;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("identifier")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(value.to_string())
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(value)
            }
        }

        Ok(Ident(deserializer.deserialize_identifier(StringVisitor)?))
    }
}

struct VariantDeserializer {
    enum_info: &'static EnumInfo,
}
impl<'de> DeserializeSeed<'de> for VariantDeserializer {
    type Value = &'static VariantInfo;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct VariantVisitor(&'static EnumInfo);

        impl<'de> Visitor<'de> for VariantVisitor {
            type Value = &'static VariantInfo;

            fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
                formatter.write_str("expected either a variant index or variant name")
            }

            fn visit_str<E>(self, variant_name: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                self.0.variant(variant_name).ok_or_else(|| {
                    let names = self.0.iter().map(|variant| variant.name());
                    Error::custom(format_args!(
                        "unknown variant `{}`, expected one of {:?}",
                        variant_name,
                        names.collect::<Vec<_>>()
                    ))
                })
            }
        }

        deserializer.deserialize_identifier(VariantVisitor(self.enum_info))
    }
}

struct StructVisitor<'a> {
    info: &'static dyn StructLikeInfo,

    set_represented_type: bool,
    registration: &'a TypeRegistration,
    registry: &'a TypeRegistry,
    world: &'a mut World,
}
impl<'a, 'de> Visitor<'de> for StructVisitor<'a> {
    type Value = DynamicStruct;

    fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
        formatter.write_str("reflected struct value")
    }

    fn visit_map<V>(self, mut map: V) -> Result<Self::Value, V::Error>
    where
        V: MapAccess<'de>,
    {
        let mut dynamic_struct = DynamicStruct::default();
        let registry = self.registry;

        while let Some(Ident(key)) = map.next_key::<Ident>()? {
            let field = self.info.get_field(&key).ok_or_else(|| {
                Error::custom(format_args!(
                    "unknown field `{}`",
                    key,
                ))
            })?;
            let registration = registry.get(field.type_id()).ok_or(Error::custom("Field not in type registry"))?;
            let value = map.next_value_seed(TypedPartialReflectDeserializer {
                set_represented_type: self.set_represented_type,
                world: self.world,
                registration,
                registry,
            })?;
            dynamic_struct.insert_boxed(&key, value);
        }

        Ok(dynamic_struct)
    }
}

struct TupleVisitor<'a> {
    info: &'static dyn TupleLikeInfo,

    set_represented_type: bool,
    registration: &'a TypeRegistration,
    registry: &'a TypeRegistry,
    world: &'a mut World,
}
impl<'a, 'de> Visitor<'de> for TupleVisitor<'a> {
    type Value = DynamicTuple;

    fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
        formatter.write_str("reflected struct value")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>, {
        let mut dynamic_tuple = DynamicTuple::default();
        let info = self.info;
        let registry = self.registry;

        for i in 0..info.get_field_len() {
            if let Some(value) = seq.next_element_seed(TypedPartialReflectDeserializer {
                set_represented_type: self.set_represented_type,
                world: self.world,
                registration: registry.get(
                    info.get_field(i).unwrap().type_id()
                ).ok_or(Error::custom("Field not in type registry"))?,
                registry
            })? {
                dynamic_tuple.insert_boxed(value);
            } else {
                break;
            }
        }

        Ok(dynamic_tuple)
    }
}

struct EnumVisitor<'a> {
    info: &'static EnumInfo,

    set_represented_type: bool,
    registration: &'a TypeRegistration,
    registry: &'a TypeRegistry,
    world: &'a mut World,
}
impl<'a, 'de> Visitor<'de> for EnumVisitor<'a> {
    type Value = DynamicEnum;

    fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
        formatter.write_str("reflected struct value")
    }

    fn visit_enum<A>(self, mut data: A) -> Result<Self::Value, A::Error>
        where
            A: EnumAccess<'de>, {
        let mut dynamic_enum = DynamicEnum::default();
        let info = self.info;
        let registry = self.registry;
        let (variant_info, variant) = data.variant_seed(VariantDeserializer {
            enum_info: info,
        })?;

        let value: DynamicVariant = match variant_info {
            VariantInfo::Unit(..) => variant.unit_variant()?.into(),
            VariantInfo::Struct(struct_info) => variant
                .struct_variant(
                    struct_info.field_names(),
                    StructVisitor {
                        info: struct_info,

                        set_represented_type: self.set_represented_type,
                        world: self.world,
                        registration: self.registration,
                        registry: self.registry,
                    },
                )?
                .into(),
            VariantInfo::Tuple(tuple_info) if tuple_info.field_len() == 1 => {
                let registration = registry.get(tuple_info.field_at(0).unwrap().type_id())
                    .ok_or(Error::custom("Field type not in registry"))?;
                let value = variant.newtype_variant_seed(TypedPartialReflectDeserializer {
                    set_represented_type: self.set_represented_type,
                    world: self.world,
                    registration,
                    registry: self.registry,
                })?;
                let mut dynamic_tuple = DynamicTuple::default();
                dynamic_tuple.insert_boxed(value);
                dynamic_tuple.into()
            },
            VariantInfo::Tuple(tuple_info) => variant
                .tuple_variant(
                    tuple_info.field_len(),
                    TupleVisitor {
                        info: tuple_info,

                        set_represented_type: self.set_represented_type,
                        world: self.world,
                        registration: self.registration,
                        registry: self.registry,
                    },
                )?
                .into(),
        };

        dynamic_enum.set_variant(variant_info.name(), value);
        Ok(dynamic_enum)
    }
}