// Copyright 2024 The Authors
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::collections::HashMap;
use std::fmt;
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
    InvalidNumber(String),
    UnmatchedBrace,
    UnmatchedBracket,
    ExpectedColon,
    ExpectedComma,
    InvalidEscape,
    InvalidString,
    ReservedKeyword(String),
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
