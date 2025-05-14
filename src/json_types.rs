use std::collections::HashMap;
use std::fmt;

use crate::json_tok::Kind;

#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Object(HashMap<String, JsonValue>),
    Array(Vec<JsonValue>),
    String(String),
    Number(Number),
    Boolean(bool),
    Null,
}

impl fmt::Display for JsonValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsonValue::Object(map) => {
                f.write_str("{")?;
                let mut first = true;
                for (key, value) in map {
                    if !first {
                        f.write_str(", ")?;
                    }
                    write!(f, "\"{}\": {}", key, value)?;
                    first = false;
                }
                f.write_str("}")
            }
            JsonValue::Array(vec) => {
                f.write_str("[")?;
                let mut first = true;
                for value in vec {
                    if !first {
                        f.write_str(", ")?;
                    }
                    write!(f, "{}", value)?;
                    first = false;
                }
                f.write_str("]")
            }
            JsonValue::String(s) => write!(f, "\"{}\"", s.replace('"', "\\\"")),
            JsonValue::Number(n) => write!(f, "{}", n),
            JsonValue::Boolean(b) => write!(f, "{}", b),
            JsonValue::Null => f.write_str("null"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Number {
    Integer(i64),
    Float(f64),
}

impl Number {
    pub fn is_i64(&self) -> bool {
        match self {
            Self::Integer(_) => true,
            _ => false,
        }
    }

    pub fn as_i64(&self) -> i64 {
        match self {
            Self::Integer(i) => i.clone(),
            _ => panic!("Tried to take float as integer."),
        }
    }
}

impl fmt::Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Number::Integer(i) => write!(f, "{}", i),
            Number::Float(n) => {
                let s = n.to_string();
                // Ensure float numbers are formatted with a decimal point
                if !s.contains('.') && !s.contains('e') {
                    write!(f, "{}.0", s)
                } else {
                    write!(f, "{}", s)
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum JsonError {
    DuplicateKey(String),
    UnexpectedChar(char),
    UnexpectedEof,
    EOF,
    InvalidNumber(String),
    InvalidEscape,
    InvalidKey,
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl JsonValue {
    pub fn as_string(&self) -> Option<&String> {
        match self {
            JsonValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<JsonValue>> {
        match self {
            JsonValue::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&HashMap<String, JsonValue>> {
        match self {
            JsonValue::Object(o) => Some(o),
            _ => None,
        }
    }

    pub fn as_number(&self) -> Option<&Number> {
        match self {
            JsonValue::Number(n) => Some(n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            JsonValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, JsonValue::Null)
    }
}

/* ------------------------------------------------------------------ */
/*  Generic “key” helper                                              */
/* ------------------------------------------------------------------ */

/// Anything that can address a child inside a `JsonValue`.
///
/// * `&str`  → object key  
/// * `usize` → array index
pub trait JsonIndex {
    fn at(self, parent: &JsonValue) -> Option<&JsonValue>;
}

impl<'a> JsonIndex for &str {
    fn at(self, parent: &JsonValue) -> Option<&JsonValue> {
        match parent {
            JsonValue::Object(map) => map.get(self),
            _ => None,
        }
    }
}
impl<'a> JsonIndex for usize {
    fn at(self, parent: &JsonValue) -> Option<&JsonValue> {
        match parent {
            JsonValue::Array(arr) => arr.get(self),
            _ => None,
        }
    }
}

/* ------------------------------------------------------------------ */
/*  Accessor                                                          */
/* ------------------------------------------------------------------ */

impl JsonValue {
    /// Borrow a child value by object key or array index.
    ///
    /// ```
    /// let v = JsonValue::Object(...);
    /// if let Some(name) = v.get("name") { … }
    /// ```
    pub fn get<K>(&self, key: K) -> Option<&JsonValue>
    where
        K: JsonIndex,
    {
        key.at(self)
    }

    pub fn get_mut<'a>(&'a mut self, key: &str) -> Option<&'a mut JsonValue> {
        match self {
            JsonValue::Object(map) => map.get_mut(key),
            _ => None,
        }
    }

    // ── NEW: mutable index lookup on arrays ─────────────────────────
    pub fn get_idx_mut<'a>(&'a mut self, idx: usize) -> Option<&'a mut JsonValue> {
        match self {
            JsonValue::Array(vec) => vec.get_mut(idx),
            _ => None,
        }
    }
}

use std::ops::Index;

impl Index<&str> for JsonValue {
    type Output = JsonValue;
    fn index(&self, key: &str) -> &Self::Output {
        self.get(key).expect("object key not found")
    }
}

impl Index<usize> for JsonValue {
    type Output = JsonValue;
    fn index(&self, idx: usize) -> &Self::Output {
        self.get(idx).expect("array index out of bounds")
    }
}
