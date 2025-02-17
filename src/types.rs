use crate::{
    json_types::JsonError,
    parser_types::{WAILAnnotation, WAILField},
};
use std::collections::HashMap;
use std::hash::Hash;

use crate::json_types::{JsonValue, Number};

static STRING_TYPE: &str = "String";
static NUMBER_TYPE: &str = "Number";
static OBJECT_TYPE: &str = "Object";
static TOOL_TYPE: &str = "Tool";
static ARRAY_TYPE: &str = "Array";

#[derive(Debug, Clone)]
pub enum JsonValidationError {
    ObjectMissingAllFields,
    ObjectMissingMetaType,
    ObjectMissingRequiredField(String),
    ObjectNestedTypeValidation((String, Box<JsonValidationError>)),
    ArrayElementTypeError((usize, Box<JsonValidationError>)),
    NotMemberOfUnion((String, Vec<(String, Box<JsonValidationError>)>)),
    ExpectedTypeError((Option<String>, String)),
    TemplateNotFound(String),
    MissingTemplateResponse(String),
    ExpectedObject(),
    JsonParserError(JsonError),
}

#[derive(Debug)]
pub enum PathSegment {
    Root((String, Option<String>)),
    Field(String),
    ArrayIndex(usize),
    UnionType(String, Vec<(String, Box<JsonValidationError>)>),
    MissingMetaType,
    ExpectedType(Option<String>, String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum WAILValue {
    String(String),
    Number(i64),
    Float(f64),
    TypeRef(String), // For when we reference a type like "String" or "Number"
}

#[derive(Debug, Clone, PartialEq)]
pub enum WAILSimpleType<'a> {
    String(WAILString<'a>),
    Boolean(WAILBoolean<'a>),
    Number(WAILNumber<'a>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum WAILCompositeType<'a> {
    Object(WAILObject<'a>),
    Array(WAILArray<'a>),
    Union(WAILUnion<'a>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct WAILUnion<'a> {
    pub members: Vec<WAILField<'a>>,
    pub type_data: WAILTypeData<'a>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WAILArray<'a> {
    pub values: Vec<WAILType<'a>>,
    pub type_data: WAILTypeData<'a>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WAILType<'a> {
    Simple(WAILSimpleType<'a>),
    Composite(WAILCompositeType<'a>),
    Value(WAILValue), // For literal values
}

pub fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

pub fn format_annoations(annotations: Vec<WAILAnnotation>) -> String {
    if annotations.is_empty() {
        return String::new();
    }
    format!(
        " # {}",
        annotations
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<String>>()
            .join("\n# ")
    )
}

impl<'a> WAILType<'a> {
    pub fn to_schema(&self) -> String {
        match self {
            WAILType::Simple(simple) => match simple {
                WAILSimpleType::String(x) => {
                    if let JsonValue::String(val) = self.type_data().json_type.clone() {
                        if val == "_type" {
                            x.value.to_string()
                        } else {
                            "string".to_string()
                        }
                    } else {
                        "string".to_string()
                    }
                }
                WAILSimpleType::Number(_) => "number".to_string(),
                WAILSimpleType::Boolean(_) => "boolean".to_string(),
            },
            WAILType::Composite(composite) => match composite {
                WAILCompositeType::Object(obj) => {
                    let mut schema = String::from("\n{\n");
                    if let Some(fields) = &obj.type_data.field_definitions {
                        for field in fields {
                            if field.field_type.element_type().is_some() {
                                schema.push_str(&format!(
                                    "  {}: {}[]{}\n",
                                    field.name,
                                    field.field_type.element_type().unwrap().to_schema(),
                                    format_annoations(field.annotations.clone())
                                ));
                            } else {
                                schema.push_str(&format!(
                                    "  {}: {}{}\n",
                                    field.name,
                                    capitalize(&field.field_type.to_schema()),
                                    format_annoations(field.annotations.clone())
                                ));
                            }
                        }
                    }
                    schema.push('}');
                    schema
                }
                WAILCompositeType::Array(arr) => {
                    format!(
                        "An array of: {}",
                        arr.type_data.element_type.as_ref().unwrap().to_schema()
                    )
                }
                WAILCompositeType::Union(union) => {
                    let mut schema = String::from("\nAny of these JSON-like formats:\n\n");
                    for (i, member) in union.members.iter().enumerate() {
                        if i > 0 {
                            schema.push_str("\n\n-- OR --\n\n");
                        }
                        schema.push_str(&format!("Format {}: ", i + 1));
                        match member.field_type {
                            WAILType::Simple(_) => {
                                schema.push_str(&member.field_type.to_schema());
                            }
                            _ => {
                                schema.push_str(&format!(
                                    "{}: ",
                                    member.field_type.type_data().type_name
                                ));
                                schema.push_str(&member.field_type.to_schema());
                            }
                        }
                    }
                    schema
                }
            },
            WAILType::Value(value) => match value {
                WAILValue::String(s) => format!("\"{}\"", s),
                WAILValue::Number(n) => n.to_string(),
                WAILValue::Float(f) => f.to_string(),
                WAILValue::TypeRef(t) => t.clone(),
            },
        }
    }

    pub fn is_object_ref(&self) -> bool {
        match self {
            WAILType::Composite(WAILCompositeType::Object(_)) => true,
            _ => false,
        }
    }

    pub fn type_name(&self) -> &'a str {
        return self.type_data().type_name;
    }

    pub fn field_definitions(&self) -> Option<Vec<WAILField<'a>>> {
        return self.type_data().field_definitions.clone();
    }

    pub fn element_type(&self) -> Option<Box<WAILType<'a>>> {
        return self.type_data().element_type.clone();
    }

    pub fn type_data(&self) -> &WAILTypeData<'a> {
        match self {
            WAILType::Simple(simple) => match simple {
                WAILSimpleType::String(s) => &s.type_data,
                WAILSimpleType::Number(n) => match n {
                    WAILNumber::Integer(i) => &i.type_data,
                    WAILNumber::Float(f) => &f.type_data,
                },
                WAILSimpleType::Boolean(b) => &b.type_data,
            },
            WAILType::Composite(composite) => match composite {
                WAILCompositeType::Object(o) => &o.type_data,
                WAILCompositeType::Array(a) => &a.type_data,
                WAILCompositeType::Union(u) => &u.type_data,
            },
            WAILType::Value(_) => unreachable!(),
        }
    }

    pub fn validate_json(&self, json: &JsonValue) -> Result<(), JsonValidationError> {
        match (self, json) {
            // Object validation with path context
            (WAILType::Composite(WAILCompositeType::Object(obj)), JsonValue::Object(map)) => {
                let fields = obj
                    .type_data
                    .field_definitions
                    .as_ref()
                    .ok_or(JsonValidationError::ObjectMissingAllFields)?;

                for field in fields {
                    match map.get(&field.name) {
                        Some(value) => field.field_type.validate_json(value).map_err(|err| {
                            JsonValidationError::ObjectNestedTypeValidation((
                                field.name.clone(),
                                Box::new(err),
                            ))
                        })?,
                        None => {
                            if field.name != "_type" {
                                return Err(JsonValidationError::ObjectMissingRequiredField(
                                    field.name.clone(),
                                ));
                            } else {
                                return Err(JsonValidationError::ObjectMissingMetaType);
                            }
                        }
                    }
                }
                Ok(())
            }

            // Array validation with index context
            (WAILType::Composite(WAILCompositeType::Array(arr)), JsonValue::Array(values)) => {
                if let Some(element_type) = &arr.type_data.element_type {
                    for (idx, value) in values.iter().enumerate() {
                        element_type.validate_json(value).map_err(|e| {
                            JsonValidationError::ArrayElementTypeError((idx, Box::new(e)))
                        })?;
                    }
                }
                Ok(())
            }

            (WAILType::Composite(WAILCompositeType::Union(union)), value) => {
                let mut errors = Vec::new();
                let mut has_error = false;
                for member_type in &union.members {
                    match member_type.field_type.validate_json(value) {
                        Ok(()) => return Ok(()),
                        Err(e) => {
                            has_error = true;
                            errors
                                .push((member_type.field_type.type_name().to_string(), Box::new(e)))
                        }
                    }
                }

                let members = union
                    .members
                    .iter()
                    .map(|m| m.field_type.type_name())
                    .collect::<Vec<_>>()
                    .join(" | ")
                    .to_string();

                if has_error {
                    Err(JsonValidationError::NotMemberOfUnion((members, errors)))
                } else {
                    Ok(())
                }
            }

            // Simple type validation with type context
            (WAILType::Simple(WAILSimpleType::String(_)), JsonValue::String(_)) => Ok(()),
            (WAILType::Simple(WAILSimpleType::Number(wail_num)), JsonValue::Number(json_num)) => {
                match (wail_num, json_num) {
                    (WAILNumber::Float(_), Number::Integer(_)) => Err(
                        JsonValidationError::ExpectedTypeError((None, "Float".to_string())),
                    ),
                    _ => Ok(()),
                }
            }
            // Type mismatch with expected type info
            (wail_type, _) => Err(JsonValidationError::ExpectedTypeError((
                None,
                match wail_type {
                    WAILType::Simple(WAILSimpleType::String(_)) => "String".to_string(),
                    WAILType::Simple(WAILSimpleType::Number(_)) => "Number".to_string(),
                    WAILType::Composite(WAILCompositeType::Object(_)) => "Object".to_string(),
                    WAILType::Composite(WAILCompositeType::Array(_)) => "Array".to_string(),
                    WAILType::Simple(WAILSimpleType::Boolean(_)) => "Boolean".to_string(),
                    _ => "String".to_string(),
                },
            ))),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct WAILTypeData<'a> {
    pub json_type: JsonValue,
    pub type_name: &'a str,
    pub field_definitions: Option<Vec<WAILField<'a>>>,
    pub element_type: Option<Box<WAILType<'a>>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WAILInteger<'a> {
    pub value: u64,
    pub type_data: WAILTypeData<'a>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WAILFloat<'a> {
    pub value: f64,
    pub type_data: WAILTypeData<'a>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WAILNumber<'a> {
    Integer(WAILInteger<'a>),
    Float(WAILFloat<'a>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct WAILObject<'a> {
    pub value: HashMap<WAILString<'a>, WAILType<'a>>,
    pub type_data: WAILTypeData<'a>,
}

#[derive(Debug, Clone)]
pub struct WAILString<'a> {
    pub value: String,
    pub type_data: WAILTypeData<'a>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WAILBoolean<'a> {
    pub value: String,
    pub type_data: WAILTypeData<'a>,
}

impl<'a> Hash for WAILString<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state)
    }
}

impl<'a> PartialEq for WAILString<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<'a> Eq for WAILString<'a> {}

use std::convert::{TryFrom, TryInto};

// First for WAILString since it's used by other types
impl<'a> TryFrom<JsonValue> for WAILString<'a> {
    type Error = String;

    fn try_from(value: JsonValue) -> Result<Self, Self::Error> {
        match value.clone() {
            JsonValue::String(s) => Ok(WAILString {
                value: s,
                type_data: WAILTypeData {
                    json_type: value,
                    type_name: STRING_TYPE,
                    field_definitions: None,
                    element_type: None,
                },
            }),
            _ => Err("Expected String JsonValue".to_string()),
        }
    }
}

// For WAILNumber
impl<'a> TryFrom<JsonValue> for WAILNumber<'a> {
    type Error = String;

    fn try_from(value: JsonValue) -> Result<Self, Self::Error> {
        match value.clone() {
            JsonValue::Number(n) => match n {
                Number::Integer(i) => Ok(WAILNumber::Integer(WAILInteger {
                    value: i as u64,
                    type_data: WAILTypeData {
                        json_type: value,
                        type_name: NUMBER_TYPE,
                        field_definitions: None,
                        element_type: None,
                    },
                })),
                Number::Float(f) => Ok(WAILNumber::Float(WAILFloat {
                    value: f,
                    type_data: WAILTypeData {
                        json_type: value,
                        type_name: NUMBER_TYPE,
                        field_definitions: None,
                        element_type: None,
                    },
                })),
            },
            _ => Err("Expected Number JsonValue".to_string()),
        }
    }
}

// For WAILSimpleType
impl<'a> TryFrom<JsonValue> for WAILSimpleType<'a> {
    type Error = String;

    fn try_from(value: JsonValue) -> Result<Self, Self::Error> {
        match value {
            JsonValue::String(_) => Ok(WAILSimpleType::String(value.try_into()?)),
            JsonValue::Number(_) => Ok(WAILSimpleType::Number(value.try_into()?)),
            _ => Err("Expected Simple Type JsonValue".to_string()),
        }
    }
}

// For WAILObject
impl<'a> TryFrom<JsonValue> for WAILObject<'a> {
    type Error = String;

    fn try_from(value: JsonValue) -> Result<Self, Self::Error> {
        match value.clone() {
            JsonValue::Object(map) => {
                let mut wail_map = HashMap::new();
                for (k, v) in map.clone() {
                    let wail_key = WAILString::try_from(JsonValue::String(k))?;
                    let wail_value = WAILType::try_from(v)?;
                    wail_map.insert(wail_key, wail_value);
                }

                let field_defs = map
                    .clone()
                    .iter()
                    .map(|(k, v)| WAILField {
                        name: k.to_string(),
                        field_type: WAILType::try_from(v.clone()).unwrap(),
                        annotations: Vec::new(),
                    })
                    .collect::<Vec<WAILField>>();

                Ok(WAILObject {
                    value: wail_map,
                    type_data: WAILTypeData {
                        json_type: value,
                        type_name: OBJECT_TYPE,
                        field_definitions: Some(field_defs),
                        element_type: None,
                    },
                })
            }
            _ => Err("Expected Object JsonValue".to_string()),
        }
    }
}

// For WAILType
impl<'a> TryFrom<JsonValue> for WAILType<'a> {
    type Error = String;

    fn try_from(value: JsonValue) -> Result<Self, Self::Error> {
        match value {
            JsonValue::String(_) | JsonValue::Number(_) => Ok(WAILType::Simple(value.try_into()?)),
            JsonValue::Object(ref map) => Ok(WAILType::Composite(WAILCompositeType::Object(
                value.try_into()?,
            ))),
            _ => Err("Unsupported JsonValue type".to_string()),
        }
    }
}

// And for converting back to JsonValue
impl<'a> From<WAILType<'a>> for JsonValue {
    fn from(wail_type: WAILType<'a>) -> JsonValue {
        match wail_type {
            WAILType::Simple(simple) => match simple {
                WAILSimpleType::String(s) => JsonValue::String(s.value),
                WAILSimpleType::Number(n) => match n {
                    WAILNumber::Integer(i) => JsonValue::Number(Number::Integer(i.value as i64)),
                    WAILNumber::Float(f) => JsonValue::Number(Number::Float(f.value)),
                },
                WAILSimpleType::Boolean(b) => JsonValue::Boolean(b.value.to_lowercase() == "true"),
            },
            WAILType::Composite(composite) => match composite {
                WAILCompositeType::Object(o) => {
                    let map: HashMap<String, JsonValue> = o
                        .value
                        .into_iter()
                        .map(|(k, v)| (k.value, v.into()))
                        .collect();
                    JsonValue::Object(map)
                }
                WAILCompositeType::Array(array) => {
                    JsonValue::Array(array.values.into_iter().map(|v| v.into()).collect())
                }
                WAILCompositeType::Union(union) => JsonValue::Object(HashMap::new()),
            },
            WAILType::Value(value) => match value {
                WAILValue::String(s) => JsonValue::String(s),
                WAILValue::Number(n) => JsonValue::Number(Number::Integer(n)),
                WAILValue::Float(f) => JsonValue::Number(Number::Float(f)),
                WAILValue::TypeRef(t) => JsonValue::String(t),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wail_string() {
        let json = JsonValue::String("hello".to_string());
        let wail: WAILString = json.clone().try_into().unwrap();
        assert_eq!(wail.value, "hello");
        let back: JsonValue = WAILType::Simple(WAILSimpleType::String(wail)).into();
        assert!(matches!(back, JsonValue::String(s) if s == "hello"));
    }

    #[test]
    fn test_wail_number_integer() {
        let json = JsonValue::Number(Number::Integer(42));
        let wail: WAILNumber = json.clone().try_into().unwrap();
        assert!(matches!(wail, WAILNumber::Integer(ref n) if n.value == 42));
        let back: JsonValue = WAILType::Simple(WAILSimpleType::Number(wail)).into();
        assert!(matches!(back, JsonValue::Number(Number::Integer(42))));
    }

    #[test]
    fn test_wail_number_float() {
        let json = JsonValue::Number(Number::Float(3.14));
        let wail: WAILNumber = json.clone().try_into().unwrap();
        assert!(matches!(wail, WAILNumber::Float(ref f) if f.value == 3.14));
        let back: JsonValue = WAILType::Simple(WAILSimpleType::Number(wail)).into();
        assert!(matches!(back, JsonValue::Number(Number::Float(f)) if f == 3.14));
    }

    #[test]
    fn test_wail_object() {
        let mut map = HashMap::new();
        map.insert("key".to_string(), JsonValue::String("value".to_string()));
        let json = JsonValue::Object(map);

        let wail: WAILObject = json.clone().try_into().unwrap();
        assert_eq!(wail.value.len(), 1);

        let back: JsonValue = WAILType::Composite(WAILCompositeType::Object(wail)).into();
        assert!(matches!(back, JsonValue::Object(m) if m.len() == 1));
    }

    #[test]
    fn test_invalid_conversions() {
        // Test string expected, got number
        let result: Result<WAILString, _> = JsonValue::Number(Number::Integer(42)).try_into();
        assert!(result.is_err());

        // Test number expected, got string
        let result: Result<WAILNumber, _> =
            JsonValue::String("not a number".to_string()).try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_array_type_json_conversion() {
        let string_array = WAILType::Composite(WAILCompositeType::Array(WAILArray {
            type_data: WAILTypeData {
                json_type: JsonValue::Array(vec![]),
                type_name: ARRAY_TYPE,
                field_definitions: None,
                element_type: Some(Box::new(WAILType::Simple(WAILSimpleType::String(
                    WAILString {
                        value: "hello".to_string(),
                        type_data: WAILTypeData {
                            json_type: JsonValue::String("hello".to_string()),
                            type_name: STRING_TYPE,
                            field_definitions: None,
                            element_type: None,
                        },
                    },
                )))),
            },
            values: vec![
                WAILType::Simple(WAILSimpleType::String(WAILString {
                    value: "hello".to_string(),
                    type_data: WAILTypeData {
                        json_type: JsonValue::String("hello".to_string()),
                        type_name: STRING_TYPE,
                        field_definitions: None,
                        element_type: None,
                    },
                })),
                WAILType::Simple(WAILSimpleType::String(WAILString {
                    value: "world".to_string(),
                    type_data: WAILTypeData {
                        json_type: JsonValue::String("world".to_string()),
                        type_name: STRING_TYPE,
                        field_definitions: None,
                        element_type: None,
                    },
                })),
            ],
        }));

        let json = JsonValue::from(string_array);
        assert!(matches!(json, JsonValue::Array(ref values) if values.len() == 2));

        if let JsonValue::Array(values) = json {
            assert!(matches!(&values[0], JsonValue::String(s) if s == "hello"));
            assert!(matches!(&values[1], JsonValue::String(s) if s == "world"));
        }
    }

    #[test]
    fn test_json_validation() {
        // Create a Person type
        let person_fields = vec![
            WAILField {
                name: "name".to_string(),
                field_type: WAILType::Simple(WAILSimpleType::String(WAILString {
                    value: String::new(),
                    type_data: WAILTypeData {
                        json_type: JsonValue::String(String::new()),
                        type_name: "String",
                        field_definitions: None,
                        element_type: None,
                    },
                })),
                annotations: vec![],
            },
            WAILField {
                name: "age".to_string(),
                field_type: WAILType::Simple(WAILSimpleType::Number(WAILNumber::Integer(
                    WAILInteger {
                        value: 0,
                        type_data: WAILTypeData {
                            json_type: JsonValue::Number(Number::Integer(0)),
                            type_name: "Number",
                            field_definitions: None,
                            element_type: None,
                        },
                    },
                ))),
                annotations: vec![],
            },
        ];

        let person_type = WAILType::Composite(WAILCompositeType::Object(WAILObject {
            value: HashMap::new(),
            type_data: WAILTypeData {
                json_type: JsonValue::Object(HashMap::new()),
                type_name: "Person",
                field_definitions: Some(person_fields),
                element_type: None,
            },
        }));

        // Valid person
        let mut valid_person = HashMap::new();
        valid_person.insert("name".to_string(), JsonValue::String("John".to_string()));
        valid_person.insert("age".to_string(), JsonValue::Number(Number::Integer(30)));
        assert!(person_type
            .validate_json(&JsonValue::Object(valid_person))
            .is_ok());

        // Invalid person - missing age
        let mut invalid_person = HashMap::new();
        invalid_person.insert("name".to_string(), JsonValue::String("John".to_string()));
        assert!(person_type
            .validate_json(&JsonValue::Object(invalid_person))
            .is_err());

        // Invalid person - wrong type for age
        let mut invalid_person = HashMap::new();
        invalid_person.insert("name".to_string(), JsonValue::String("John".to_string()));
        invalid_person.insert("age".to_string(), JsonValue::String("30".to_string()));
        assert!(person_type
            .validate_json(&JsonValue::Object(invalid_person))
            .is_err());
    }
}
