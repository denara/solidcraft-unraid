//! Replace only top-level fields during serialization. This avoids cloning or turning
//! large embedded payloads into JSON values and automatically preserves future fields.
use serde::ser::{Serialize, SerializeMap, SerializeStruct, Serializer};
use serde_json::{Map, Value};

pub(super) fn replace<T: Serialize>(value: &T, fields: Map<String, Value>) -> serde_json::Result<Value> {
    value.serialize(Replace(fields))
}

struct Replace(Map<String, Value>);
struct Fields {
    replacements: Map<String, Value>,
    values: Map<String, Value>,
}

impl Fields {
    fn field<T: ?Sized + Serialize>(&mut self, key: &str, value: &T) -> serde_json::Result<()> {
        let value = match self.replacements.remove(key) {
            Some(value) => value,
            None => serde_json::to_value(value)?,
        };
        self.values.insert(key.to_owned(), value);
        Ok(())
    }
}

impl SerializeStruct for Fields {
    type Ok = Value;
    type Error = serde_json::Error;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error> {
        self.field(key, value)
    }
    fn end(self) -> Result<Value, Self::Error> {
        Ok(Value::Object(self.values))
    }
}

impl SerializeMap for Fields {
    type Ok = Value;
    type Error = serde_json::Error;
    fn serialize_key<T: ?Sized + Serialize>(&mut self, _key: &T) -> Result<(), Self::Error> {
        Err(serde::ser::Error::custom("metadata requires map entries"))
    }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, _value: &T) -> Result<(), Self::Error> {
        Err(serde::ser::Error::custom("metadata requires map entries"))
    }
    fn serialize_entry<K: ?Sized + Serialize, V: ?Sized + Serialize>(&mut self, key: &K, value: &V) -> Result<(), Self::Error> {
        let key = serde_json::to_value(key)?;
        let key = key.as_str().ok_or_else(|| <Self::Error as serde::ser::Error>::custom("metadata key must be a string"))?;
        self.field(key, value)
    }
    fn end(self) -> Result<Value, Self::Error> {
        Ok(Value::Object(self.values))
    }
}

macro_rules! forward {
    ($($name:ident($($arg:ident: $ty:ty),*);)*) => {$(
        fn $name(self, $($arg: $ty),*) -> Result<Self::Ok, Self::Error> {
            serde_json::value::Serializer.$name($($arg),*)
        }
    )*};
}

impl Serializer for Replace {
    type Ok = Value;
    type Error = serde_json::Error;
    type SerializeSeq = <serde_json::value::Serializer as Serializer>::SerializeSeq;
    type SerializeTuple = <serde_json::value::Serializer as Serializer>::SerializeTuple;
    type SerializeTupleStruct = <serde_json::value::Serializer as Serializer>::SerializeTupleStruct;
    type SerializeTupleVariant = <serde_json::value::Serializer as Serializer>::SerializeTupleVariant;
    type SerializeMap = Fields;
    type SerializeStruct = Fields;
    type SerializeStructVariant = <serde_json::value::Serializer as Serializer>::SerializeStructVariant;
    forward! {
        serialize_bool(v: bool); serialize_i8(v: i8); serialize_i16(v: i16); serialize_i32(v: i32); serialize_i64(v: i64);
        serialize_u8(v: u8); serialize_u16(v: u16); serialize_u32(v: u32); serialize_u64(v: u64);
        serialize_f32(v: f32); serialize_f64(v: f64); serialize_char(v: char); serialize_str(v: &str);
        serialize_bytes(v: &[u8]); serialize_none(); serialize_unit(); serialize_unit_struct(name: &'static str);
        serialize_unit_variant(name: &'static str, index: u32, variant: &'static str);
    }
    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<Value, Self::Error> {
        serde_json::to_value(value)
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(self, _name: &'static str, value: &T) -> Result<Value, Self::Error> {
        serde_json::to_value(value)
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Value, Self::Error> {
        serde_json::value::Serializer.serialize_newtype_variant(name, index, variant, value)
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        serde_json::value::Serializer.serialize_seq(len)
    }
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        serde_json::value::Serializer.serialize_tuple(len)
    }
    fn serialize_tuple_struct(self, name: &'static str, len: usize) -> Result<Self::SerializeTupleStruct, Self::Error> {
        serde_json::value::Serializer.serialize_tuple_struct(name, len)
    }
    fn serialize_tuple_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        serde_json::value::Serializer.serialize_tuple_variant(name, index, variant, len)
    }
    fn serialize_struct_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        serde_json::value::Serializer.serialize_struct_variant(name, index, variant, len)
    }
    fn serialize_map(self, _len: Option<usize>) -> Result<Fields, Self::Error> {
        Ok(Fields { replacements: self.0, values: Map::new() })
    }
    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Fields, Self::Error> {
        self.serialize_map(None)
    }
}
