use log::debug;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::collections::HashMap; // Added this line

use crate::json_types::{JsonValue, Number};

#[derive(Debug, Clone, PartialEq)]
pub enum PyTypeKind {
    Any,
    String,
    Integer,
    Float,
    Boolean,
    List,
    Tuple,
    Dict,
    Optional,
    Union,
    Class,
    None,
}

#[derive(Debug, Clone)]
pub struct PyTypeInfo {
    pub kind: PyTypeKind,
    pub name: String,
    pub module: Option<String>,
    pub origin: Option<String>,
    pub args: Vec<PyTypeInfo>,
    pub fields: HashMap<String, PyTypeInfo>,
    pub is_optional: bool,
    pub py_type: Option<Py<PyAny>>, // Store the original Python type object
}

impl PyTypeInfo {
    pub fn new(kind: PyTypeKind, name: String) -> Self {
        Self {
            kind,
            name,
            module: None,
            origin: None,
            args: Vec::new(),
            fields: HashMap::new(),
            is_optional: false,
            py_type: None,
        }
    }

    pub fn with_module(mut self, module: String) -> Self {
        self.module = Some(module);
        self
    }

    pub fn with_py_type(mut self, py_type: Py<PyAny>) -> Self {
        self.py_type = Some(py_type);
        self
    }

    pub fn with_origin(mut self, origin: String) -> Self {
        self.origin = Some(origin);
        self
    }

    pub fn with_args(mut self, args: Vec<PyTypeInfo>) -> Self {
        self.args = args;
        self
    }

    pub fn with_fields(mut self, fields: HashMap<String, PyTypeInfo>) -> Self {
        self.fields = fields;
        self
    }

    pub fn with_optional(mut self, is_optional: bool) -> Self {
        self.is_optional = is_optional;
        self
    }

    /// Check if a JsonValue matches this type
    pub fn matches(&self, value: &JsonValue) -> bool {
        match (&self.kind, value) {
            (PyTypeKind::Any, _) => true,
            (PyTypeKind::String, JsonValue::String(_)) => true,
            (PyTypeKind::Integer, JsonValue::Number(Number::Integer(_))) => true,
            (PyTypeKind::Float, JsonValue::Number(_)) => true,
            (PyTypeKind::Boolean, JsonValue::Boolean(_)) => true,
            (PyTypeKind::None, JsonValue::Null) => true,
            (PyTypeKind::List, JsonValue::Array(_)) => {
                if self.args.is_empty() {
                    true
                } else {
                    // Check if all elements match the list type
                    if let JsonValue::Array(arr) = value {
                        arr.iter().all(|item| self.args[0].matches(item))
                    } else {
                        false
                    }
                }
            }
            (PyTypeKind::Tuple, JsonValue::Array(_)) => {
                // Tuples are represented as arrays in JSON
                if self.args.is_empty() {
                    true
                } else if let JsonValue::Array(arr) = value {
                    // For homogeneous tuples (Tuple[int, ...]) check all elements
                    if self.args.len() == 1 {
                        arr.iter().all(|item| self.args[0].matches(item))
                    } else {
                        // For fixed tuples (Tuple[str, int, bool]) check each position
                        arr.len() == self.args.len()
                            && arr
                                .iter()
                                .zip(&self.args)
                                .all(|(item, expected_type)| expected_type.matches(item))
                    }
                } else {
                    false
                }
            }
            (PyTypeKind::Dict, JsonValue::Object(_)) => {
                if self.args.len() < 2 {
                    true
                } else {
                    // TODO: Check if all keys and values match the dict types
                    true
                }
            }
            (PyTypeKind::Optional, _) => {
                if value == &JsonValue::Null {
                    true
                } else if !self.args.is_empty() {
                    self.args[0].matches(value)
                } else {
                    false
                }
            }
            (PyTypeKind::Union, _) => {
                // Check if value matches any of the union types
                self.args.iter().any(|arg| arg.matches(value))
            }
            (PyTypeKind::Class, JsonValue::Object(_)) => {
                // For classes, we check if the object has the required fields
                if let JsonValue::Object(obj) = value {
                    // Simple check: all required fields exist and match their types
                    self.fields.iter().all(|(field_name, field_type)| {
                        if let Some(field_value) = obj.get(field_name) {
                            field_type.matches(field_value)
                        } else {
                            // Field is missing, but might be optional
                            field_type.is_optional
                        }
                    })
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    pub fn extract_from_python(py_type: &PyAny) -> PyResult<Self> {
        // Store reference to the original Python type
        let py_type_ref = py_type.into_py(py_type.py());

        // Check if this is a type alias (created with 'type' statement)
        if let Ok(value_attr) = py_type.getattr("__value__") {
            // Check if the __value__ is a Union type
            if let Ok(origin) = value_attr.getattr("__origin__") {
                let origin_str = origin.str()?.extract::<String>()?;
                if origin_str == "typing.Union" || origin_str.ends_with(".Union") {
                    // This is a union type alias
                    // Get the alias name
                    let alias_name = if let Ok(name) = py_type.getattr("__name__") {
                        name.extract::<String>()?
                    } else {
                        "Union".to_string()
                    };

                    // Extract the Union type arguments
                    let type_args = if let Ok(args) = value_attr.getattr("__args__") {
                        let args_seq = args.extract::<Vec<&PyAny>>()?;
                        let mut type_infos = Vec::new();
                        for arg in args_seq {
                            type_infos.push(PyTypeInfo::extract_from_python(arg)?);
                        }
                        type_infos
                    } else {
                        Vec::new()
                    };

                    // Return as a Union type with the alias name
                    return Ok(PyTypeInfo::new(PyTypeKind::Union, alias_name)
                        .with_module("typing".to_string())
                        .with_origin("Union".to_string())
                        .with_args(type_args)
                        .with_py_type(py_type_ref));
                }
            }
        }

        // Get type name
        let type_name = if let Ok(name) = py_type.getattr("__name__") {
            name.extract::<String>()?
        } else {
            // For typing objects like List, Optional, etc.
            let repr = py_type.repr()?.extract::<String>()?;
            if let Some(idx) = repr.find('[') {
                repr[..idx].trim().to_string()
            } else {
                repr
            }
        };

        // Get module name
        let module_name = if let Ok(module) = py_type.getattr("__module__") {
            Some(module.extract::<String>()?)
        } else {
            None
        };

        // Special handling for typing constructs that might not have __origin__ in some Python versions
        // Check the string representation first
        let repr_str = py_type.repr()?.extract::<String>()?;
        if repr_str.starts_with("typing.List[") || repr_str.starts_with("list[") {
            // Get type arguments
            let type_args = if let Ok(args) = py_type.getattr("__args__") {
                let args_seq = args.extract::<Vec<&PyAny>>()?;
                let mut type_infos = Vec::new();
                for arg in args_seq {
                    type_infos.push(PyTypeInfo::extract_from_python(arg)?);
                }
                type_infos
            } else {
                Vec::new()
            };

            return Ok(PyTypeInfo::new(PyTypeKind::List, "list".to_string())
                .with_module("typing".to_string())
                .with_origin("list".to_string())
                .with_args(type_args)
                .with_py_type(py_type_ref));
        }

        if repr_str.starts_with("typing.Tuple[") || repr_str.starts_with("tuple[") {
            // Get type arguments
            let type_args = if let Ok(args) = py_type.getattr("__args__") {
                let args_seq = args.extract::<Vec<&PyAny>>()?;
                let mut type_infos = Vec::new();
                for arg in args_seq {
                    type_infos.push(PyTypeInfo::extract_from_python(arg)?);
                }
                type_infos
            } else {
                Vec::new()
            };

            return Ok(PyTypeInfo::new(PyTypeKind::Tuple, "tuple".to_string())
                .with_module("typing".to_string())
                .with_origin("tuple".to_string())
                .with_args(type_args)
                .with_py_type(py_type_ref));
        }

        // Check if this is a typing module construct (List, Dict, Optional, etc.)
        if let Ok(origin) = py_type.getattr("__origin__") {
            let origin_name = origin.str()?.extract::<String>()?;

            // Get type arguments
            let type_args = if let Ok(args) = py_type.getattr("__args__") {
                let args_seq = args.extract::<Vec<&PyAny>>()?;
                let mut type_infos = Vec::new();
                for arg in args_seq {
                    type_infos.push(PyTypeInfo::extract_from_python(arg)?);
                }
                type_infos
            } else {
                Vec::new()
            };

            // Handle specific typing constructs
            // Strip module prefix if present
            let base_origin = if let Some(pos) = origin_name.rfind('.') {
                &origin_name[pos + 1..]
            } else {
                &origin_name
            };

            match base_origin {
                "list" => {
                    return Ok(PyTypeInfo::new(PyTypeKind::List, "list".to_string())
                        .with_module(module_name.unwrap_or_else(|| "builtins".to_string()))
                        .with_origin(origin_name)
                        .with_args(type_args)
                        .with_py_type(py_type_ref));
                }
                "tuple" => {
                    return Ok(PyTypeInfo::new(PyTypeKind::Tuple, "tuple".to_string())
                        .with_module(module_name.unwrap_or_else(|| "builtins".to_string()))
                        .with_origin(origin_name)
                        .with_args(type_args)
                        .with_py_type(py_type_ref));
                }
                "dict" => {
                    return Ok(PyTypeInfo::new(PyTypeKind::Dict, "dict".to_string())
                        .with_module(module_name.unwrap_or_else(|| "builtins".to_string()))
                        .with_origin(origin_name)
                        .with_args(type_args)
                        .with_py_type(py_type_ref));
                }
                "Union" => {
                    // Get type arguments with proper py_type references
                    let type_args = if let Ok(args) = py_type.getattr("__args__") {
                        let args_seq = args.extract::<Vec<&PyAny>>()?;
                        let mut type_infos = Vec::new();
                        for arg in args_seq {
                            // Extract each arg with its proper py_type reference
                            let mut arg_info = PyTypeInfo::extract_from_python(arg)?;
                            // Ensure the py_type is set
                            if arg_info.py_type.is_none() {
                                arg_info.py_type = Some(arg.into_py(py_type.py()));
                            }
                            type_infos.push(arg_info);
                        }
                        type_infos
                    } else {
                        Vec::new()
                    };

                    // Check if this is Optional (Union[T, None])
                    let is_optional = type_args.iter().any(|arg| arg.kind == PyTypeKind::None);

                    if is_optional && type_args.len() == 2 {
                        // Find the non-None type
                        let non_none_type = type_args
                            .iter()
                            .find(|arg| arg.kind != PyTypeKind::None)
                            .cloned()
                            .unwrap_or_else(|| PyTypeInfo::new(PyTypeKind::Any, "Any".to_string()));

                        return Ok(
                            PyTypeInfo::new(PyTypeKind::Optional, "Optional".to_string())
                                .with_module(module_name.unwrap_or_else(|| "typing".to_string()))
                                .with_origin(origin_name.clone())
                                .with_args(vec![non_none_type])
                                .with_optional(true)
                                .with_py_type(py_type_ref),
                        );
                    } else {
                        return Ok(PyTypeInfo::new(PyTypeKind::Union, "Union".to_string())
                            .with_module(module_name.unwrap_or_else(|| "typing".to_string()))
                            .with_origin(origin_name.clone())
                            .with_args(type_args)
                            .with_py_type(py_type_ref));
                    }
                }
                _ => {
                    // Generic container type
                    return Ok(PyTypeInfo::new(PyTypeKind::Any, origin_name.clone())
                        .with_module(module_name.unwrap_or_else(|| "typing".to_string()))
                        .with_origin(origin_name)
                        .with_args(type_args)
                        .with_py_type(py_type_ref));
                }
            }
        }

        // Handle built-in types
        match type_name.as_str() {
            "str" => {
                return Ok(PyTypeInfo::new(PyTypeKind::String, "str".to_string())
                    .with_module(module_name.unwrap_or_else(|| "builtins".to_string()))
                    .with_py_type(py_type_ref));
            }
            "int" => {
                return Ok(PyTypeInfo::new(PyTypeKind::Integer, "int".to_string())
                    .with_module(module_name.unwrap_or_else(|| "builtins".to_string()))
                    .with_py_type(py_type_ref));
            }
            "float" => {
                return Ok(PyTypeInfo::new(PyTypeKind::Float, "float".to_string())
                    .with_module(module_name.unwrap_or_else(|| "builtins".to_string()))
                    .with_py_type(py_type_ref));
            }
            "bool" => {
                return Ok(PyTypeInfo::new(PyTypeKind::Boolean, "bool".to_string())
                    .with_module(module_name.unwrap_or_else(|| "builtins".to_string()))
                    .with_py_type(py_type_ref));
            }
            "list" => {
                return Ok(PyTypeInfo::new(PyTypeKind::List, "list".to_string())
                    .with_module(module_name.unwrap_or_else(|| "builtins".to_string()))
                    .with_py_type(py_type_ref));
            }
            "dict" => {
                return Ok(PyTypeInfo::new(PyTypeKind::Dict, "dict".to_string())
                    .with_module(module_name.unwrap_or_else(|| "builtins".to_string()))
                    .with_py_type(py_type_ref));
            }
            "NoneType" => {
                return Ok(PyTypeInfo::new(PyTypeKind::None, "None".to_string())
                    .with_module(module_name.unwrap_or_else(|| "builtins".to_string()))
                    .with_py_type(py_type_ref));
            }
            "type" => {
                // This is a class type
                return Ok(PyTypeInfo::new(PyTypeKind::Class, type_name)
                    .with_module(module_name.unwrap_or_else(|| "builtins".to_string()))
                    .with_py_type(py_type_ref));
            }
            _ => {
                // Check if this is a class by seeing if it has __annotations__
                if let Ok(annotations) = py_type.getattr("__annotations__") {
                    let annotations_dict = annotations.downcast::<PyDict>()?;
                    let mut fields = HashMap::new();

                    for (key, value) in annotations_dict.iter() {
                        let field_name = key.extract::<String>()?;
                        let field_type = PyTypeInfo::extract_from_python(value)?;
                        fields.insert(field_name, field_type);
                    }

                    return Ok(PyTypeInfo::new(PyTypeKind::Class, type_name)
                        .with_module(module_name.unwrap_or_else(|| "builtins".to_string()))
                        .with_fields(fields)
                        .with_py_type(py_type_ref));
                } else if let Ok(_bases) = py_type.getattr("__bases__") {
                    // This is likely a class type too
                    return Ok(PyTypeInfo::new(PyTypeKind::Class, type_name)
                        .with_module(module_name.unwrap_or_else(|| "builtins".to_string()))
                        .with_py_type(py_type_ref));
                }

                // Fallback to Any
                return Ok(PyTypeInfo::new(PyTypeKind::Any, type_name)
                    .with_module(module_name.unwrap_or_else(|| "builtins".to_string()))
                    .with_py_type(py_type_ref));
            }
        }
    }
}

/// Convert a JsonValue to a Python object based on type info
pub fn json_to_python(
    py: Python,
    value: &JsonValue,
    type_info: Option<&PyTypeInfo>,
) -> PyResult<PyObject> {
    match value {
        JsonValue::String(s) => {
            if let Some(ti) = type_info {
                match ti.kind {
                    PyTypeKind::List if !ti.args.is_empty() => {
                        let element_type_info = &ti.args[0];
                        if matches!(
                            element_type_info.kind,
                            PyTypeKind::Class | PyTypeKind::Union
                        ) {
                            debug!("[json_to_python] Coercing String to List[Class/Union]. String: {:.50?}, Expected element: {:?}", s, element_type_info.name);
                            let mut obj_map = HashMap::new();
                            // Heuristic: use first field name or "content"
                            let field_name = element_type_info
                                .fields
                                .keys()
                                .next()
                                .cloned()
                                .unwrap_or_else(|| "content".to_string());
                            obj_map.insert(field_name, JsonValue::String(s.clone()));

                            let item_type_name_to_use = if element_type_info.kind
                                == PyTypeKind::Union
                            {
                                element_type_info
                                    .args
                                    .iter()
                                    .find(|arg| arg.kind == PyTypeKind::Class)
                                    .map_or(element_type_info.name.clone(), |arg| arg.name.clone())
                            } else {
                                // Class
                                element_type_info.name.clone()
                            };
                            obj_map.insert(
                                "_type_name".to_string(),
                                JsonValue::String(item_type_name_to_use),
                            );

                            let wrapped_value = JsonValue::Array(vec![JsonValue::Object(obj_map)]);
                            return json_to_python(py, &wrapped_value, Some(ti));
                            // Recurse with original List type_info
                        }
                    }
                    PyTypeKind::Class | PyTypeKind::Union => {
                        debug!("[json_to_python] Coercing String to Class/Union. String: {:.50?}, Expected: {:?}", s, ti.name);
                        let mut obj_map = HashMap::new();
                        let field_name = ti
                            .fields
                            .keys()
                            .next()
                            .cloned()
                            .unwrap_or_else(|| "content".to_string());
                        obj_map.insert(field_name, JsonValue::String(s.clone()));

                        let type_name_to_use = if ti.kind == PyTypeKind::Union {
                            ti.args
                                .iter()
                                .find(|arg| arg.kind == PyTypeKind::Class)
                                .map_or(ti.name.clone(), |arg| arg.name.clone())
                        } else {
                            // Class
                            ti.name.clone()
                        };
                        obj_map.insert(
                            "_type_name".to_string(),
                            JsonValue::String(type_name_to_use),
                        );

                        let wrapped_value = JsonValue::Object(obj_map);
                        return json_to_python(py, &wrapped_value, Some(ti)); // Recurse with original Class/Union type_info
                    }
                    _ => {} // Fall through to default string conversion
                }
            }
            Ok(s.clone().into_py(py))
        }
        JsonValue::Number(n) => {
            // Apply similar coercion for Numbers
            if let Some(ti) = type_info {
                match ti.kind {
                    PyTypeKind::List if !ti.args.is_empty() => {
                        let element_type_info = &ti.args[0];
                        if matches!(
                            element_type_info.kind,
                            PyTypeKind::Class | PyTypeKind::Union
                        ) {
                            debug!("[json_to_python] Coercing Number to List[Class/Union]. Number: {:?}, Expected element: {:?}", n, element_type_info.name);
                            let mut obj_map = HashMap::new();
                            let field_name = element_type_info
                                .fields
                                .keys()
                                .next()
                                .cloned()
                                .unwrap_or_else(|| "value".to_string());
                            obj_map.insert(field_name, JsonValue::Number(n.clone()));
                            let item_type_name_to_use = if element_type_info.kind
                                == PyTypeKind::Union
                            {
                                element_type_info
                                    .args
                                    .iter()
                                    .find(|arg| arg.kind == PyTypeKind::Class)
                                    .map_or(element_type_info.name.clone(), |arg| arg.name.clone())
                            } else {
                                element_type_info.name.clone()
                            };
                            obj_map.insert(
                                "_type_name".to_string(),
                                JsonValue::String(item_type_name_to_use),
                            );
                            let wrapped_value = JsonValue::Array(vec![JsonValue::Object(obj_map)]);
                            return json_to_python(py, &wrapped_value, Some(ti));
                        }
                    }
                    PyTypeKind::Class | PyTypeKind::Union => {
                        debug!("[json_to_python] Coercing Number to Class/Union. Number: {:?}, Expected: {:?}", n, ti.name);
                        let mut obj_map = HashMap::new();
                        let field_name = ti
                            .fields
                            .keys()
                            .next()
                            .cloned()
                            .unwrap_or_else(|| "value".to_string());
                        obj_map.insert(field_name, JsonValue::Number(n.clone()));
                        let type_name_to_use = if ti.kind == PyTypeKind::Union {
                            ti.args
                                .iter()
                                .find(|arg| arg.kind == PyTypeKind::Class)
                                .map_or(ti.name.clone(), |arg| arg.name.clone())
                        } else {
                            ti.name.clone()
                        };
                        obj_map.insert(
                            "_type_name".to_string(),
                            JsonValue::String(type_name_to_use),
                        );
                        let wrapped_value = JsonValue::Object(obj_map);
                        return json_to_python(py, &wrapped_value, Some(ti));
                    }
                    _ => {}
                }
            }
            // Default number conversion (original logic was more detailed here)
            match n {
                Number::Integer(i) => Ok(i.into_py(py)),
                Number::Float(f) => Ok(f.into_py(py)),
            }
        }
        JsonValue::Boolean(b) => {
            // Apply similar coercion for Booleans
            if let Some(ti) = type_info {
                match ti.kind {
                    PyTypeKind::List if !ti.args.is_empty() => {
                        let element_type_info = &ti.args[0];
                        if matches!(
                            element_type_info.kind,
                            PyTypeKind::Class | PyTypeKind::Union
                        ) {
                            debug!("[json_to_python] Coercing Boolean to List[Class/Union]. Boolean: {:?}, Expected element: {:?}", b, element_type_info.name);
                            let mut obj_map = HashMap::new();
                            let field_name = element_type_info
                                .fields
                                .keys()
                                .next()
                                .cloned()
                                .unwrap_or_else(|| "flag".to_string());
                            obj_map.insert(field_name, JsonValue::Boolean(*b));
                            let item_type_name_to_use = if element_type_info.kind
                                == PyTypeKind::Union
                            {
                                element_type_info
                                    .args
                                    .iter()
                                    .find(|arg| arg.kind == PyTypeKind::Class)
                                    .map_or(element_type_info.name.clone(), |arg| arg.name.clone())
                            } else {
                                element_type_info.name.clone()
                            };
                            obj_map.insert(
                                "_type_name".to_string(),
                                JsonValue::String(item_type_name_to_use),
                            );
                            let wrapped_value = JsonValue::Array(vec![JsonValue::Object(obj_map)]);
                            return json_to_python(py, &wrapped_value, Some(ti));
                        }
                    }
                    PyTypeKind::Class | PyTypeKind::Union => {
                        debug!("[json_to_python] Coercing Boolean to Class/Union. Boolean: {:?}, Expected: {:?}", b, ti.name);
                        let mut obj_map = HashMap::new();
                        let field_name = ti
                            .fields
                            .keys()
                            .next()
                            .cloned()
                            .unwrap_or_else(|| "flag".to_string());
                        obj_map.insert(field_name, JsonValue::Boolean(*b));
                        let type_name_to_use = if ti.kind == PyTypeKind::Union {
                            ti.args
                                .iter()
                                .find(|arg| arg.kind == PyTypeKind::Class)
                                .map_or(ti.name.clone(), |arg| arg.name.clone())
                        } else {
                            ti.name.clone()
                        };
                        obj_map.insert(
                            "_type_name".to_string(),
                            JsonValue::String(type_name_to_use),
                        );
                        let wrapped_value = JsonValue::Object(obj_map);
                        return json_to_python(py, &wrapped_value, Some(ti));
                    }
                    _ => {}
                }
            }
            Ok(b.into_py(py))
        }
        JsonValue::Object(map) => {
            // Coercion: If type_info is List[Class/Union] but value is a single Object, wrap it in an array.
            if let Some(ti) = type_info {
                if ti.kind == PyTypeKind::List && !ti.args.is_empty() {
                    let element_type_info = &ti.args[0];
                    if matches!(
                        element_type_info.kind,
                        PyTypeKind::Class | PyTypeKind::Union
                    ) {
                        debug!("[json_to_python] Coercing single Object into List[Class/Union]. Expected element: {:?}", element_type_info.name);
                        let wrapped_value = JsonValue::Array(vec![JsonValue::Object(map.clone())]);
                        return json_to_python(py, &wrapped_value, Some(ti)); // Recurse with original List type_info
                    }
                }
            }

            // If we have type info for a union, try to determine which type to use
            if let Some(PyTypeInfo {
                kind: PyTypeKind::Union,
                args,
                ..
            }) = type_info
            {
                // First, check if there's a _type_name field to disambiguate
                if let Some(JsonValue::String(type_name)) = map.get("_type_name") {
                    // Find the matching type in the union args
                    for arg in args {
                        if &arg.name == type_name {
                            // Recursively convert with the specific type
                            return json_to_python(py, value, Some(arg));
                        }
                    }
                }

                // If no _type_name or no match found, try to disambiguate based on fields
                for arg in args {
                    if arg.kind == PyTypeKind::Class && arg.matches(value) {
                        // This type matches based on fields, use it
                        return json_to_python(py, value, Some(arg));
                    }
                }

                // If we still can't determine, fall through to dict
            }

            // If we have type info for a class, try to construct the class
            if let Some(PyTypeInfo {
                kind: PyTypeKind::Class,
                name,
                fields,
                py_type,
                ..
            }) = type_info
            {
                // First, use the stored Python type reference if available
                if let Some(py_type_ref) = py_type {
                    let py_type_obj = py_type_ref.as_ref(py);
                    return create_instance_from_json(py, py_type_obj, map, fields);
                }

                // Fallback to previous module-based lookup if no direct reference is available
                // First try the gasp module
                if let Ok(module) = py.import("gasp") {
                    if let Ok(py_type) = module.getattr(name.as_str()) {
                        return create_instance_from_json(py, py_type, map, fields);
                    }
                }

                // Then try builtins
                if let Ok(module) = py.import("builtins") {
                    if let Ok(py_type) = module.getattr(name.as_str()) {
                        return create_instance_from_json(py, py_type, map, fields);
                    }
                }

                // Finally try __main__
                if let Ok(module) = py.import("__main__") {
                    if let Ok(py_type) = module.getattr(name.as_str()) {
                        return create_instance_from_json(py, py_type, map, fields);
                    }
                }
            }
            if let Some(ti) = type_info {
                if ti.kind == PyTypeKind::Class {
                    // This implies that the PyTypeKind::Class block above did not return early
                    // with a successfully created instance, or create_instance_from_json returned None,
                    // and somehow we didn't return py.None() from there.
                    // This is an explicit final guard.
                    debug!("[json_to_python] Fallback for JsonValue::Object, but expected kind was Class ('{}'). All instantiation attempts failed. STRICTLY returning None.", ti.name);
                    return Ok(py.None());
                }
            }

            Ok(py.None())
        }
        JsonValue::Array(arr) => {
            let list = PyList::empty(py);

            // Check if we're dealing with a tuple type
            if let Some(PyTypeInfo {
                kind: PyTypeKind::Tuple,
                args,
                py_type,
                ..
            }) = type_info
            {
                // Process tuple elements
                for (i, item) in arr.iter().enumerate() {
                    // For fixed tuples, use the appropriate type for each position
                    let elem_type = if args.len() > 1 && i < args.len() {
                        Some(&args[i])
                    } else if args.len() == 1 {
                        // Homogeneous tuple (Tuple[int, ...])
                        Some(&args[0])
                    } else {
                        None
                    };

                    list.append(json_to_python(py, item, elem_type)?)?;
                }

                // Convert list to tuple
                if let Some(py_type_ref) = py_type {
                    let py_type_obj = py_type_ref.as_ref(py);
                    // Try to construct using the Python type (this will create a tuple)
                    if let Ok(result) = py_type_obj.call1((list,)) {
                        return Ok(result.into());
                    }
                }

                // Fallback: use Python's tuple() function
                let tuple_type = py.eval("tuple", None, None)?;
                return Ok(tuple_type.call1((list,))?.into());
            }

            // Get element type if available for lists
            let element_type = if let Some(PyTypeInfo {
                kind: PyTypeKind::List,
                args,
                ..
            }) = type_info
            {
                if !args.is_empty() {
                    Some(&args[0])
                } else {
                    None
                }
            } else {
                None
            };

            for item in arr {
                // For each item, we need to properly handle it based on its type and the expected element type
                if let Some(elem_type) = element_type {
                    match (item, &elem_type.kind) {
                        // If we expect a class and have an object, create an instance
                        (JsonValue::Object(obj_map), PyTypeKind::Class)
                            if elem_type.py_type.is_some() =>
                        {
                            let py_type_ref = elem_type.py_type.as_ref().unwrap();
                            let py_type_obj = py_type_ref.as_ref(py);

                            // Create a proper instance of the class
                            let instance = create_instance_from_json(
                                py,
                                py_type_obj,
                                obj_map,
                                &elem_type.fields,
                            )?;
                            list.append(instance)?;
                        }
                        // Otherwise, process normally
                        _ => {
                            list.append(json_to_python(py, item, Some(elem_type))?)?;
                        }
                    }
                } else {
                    // No type info available, just convert normally
                    list.append(json_to_python(py, item, None)?)?;
                }
            }

            // If the list itself has a type, convert it
            if let Some(PyTypeInfo { py_type, .. }) = type_info {
                if let Some(py_type_ref) = py_type {
                    let py_type_obj = py_type_ref.as_ref(py);

                    // Try to construct using the Python type
                    if let Ok(result) = py_type_obj.call1((list,)) {
                        return Ok(result.into());
                    }
                }
            }

            Ok(list.into())
        }
        JsonValue::Null => Ok(py.None()),
    }
}

// Helper to instantiate objects in a list
fn process_list_with_element_type(
    py: Python,
    arr: &Vec<JsonValue>,
    elem_type: &PyTypeInfo,
) -> PyResult<PyObject> {
    // Create a new Python list
    let list = PyList::empty(py);

    for item in arr {
        // Process each element based on type information
        if elem_type.kind == PyTypeKind::Class && elem_type.py_type.is_some() {
            match item {
                JsonValue::Object(obj_map) => {
                    // Get the Python type for the element
                    let py_type_ref = elem_type.py_type.as_ref().unwrap();
                    let py_type_obj = py_type_ref.as_ref(py);

                    // Create a properly typed instance for this element
                    let instance =
                        create_instance_from_json(py, py_type_obj, obj_map, &elem_type.fields)?;
                    list.append(instance)?;
                }
                _ => {
                    // Not an object but still needs conversion
                    list.append(json_to_python(py, item, Some(elem_type))?)?;
                }
            }
        } else {
            // Standard element conversion
            list.append(json_to_python(py, item, Some(elem_type))?)?;
        }
    }

    Ok(list.into())
}

// Creates a Python instance from a JSON map with proper type conversion
pub fn create_instance_from_json(
    py: Python,
    py_type: &PyAny,
    map: &HashMap<String, JsonValue>,
    fields: &HashMap<String, PyTypeInfo>,
) -> PyResult<PyObject> {
    // Try to create the instance using __gasp_from_partial__ if available
    let partial_data = PyDict::new(py);

    // Convert the JSON map to a Python dict
    for (k, v) in map {
        let field_type = fields.get(k);

        // Process the value with the correct field type information
        let py_value = match (v, field_type) {
            // Process lists that contain complex objects
            (JsonValue::Array(arr), Some(field_info))
                if field_info.kind == PyTypeKind::List && !field_info.args.is_empty() =>
            {
                // Get the element type from the list's type args
                let elem_type = &field_info.args[0];

                // Process the list with the element type information
                process_list_with_element_type(py, arr, elem_type)?
            }
            // For nested objects, ensure we pass the type info
            (JsonValue::Object(_), Some(field_info)) if field_info.kind == PyTypeKind::Class => {
                json_to_python(py, v, Some(field_info))?
            }
            // For other lists
            (JsonValue::Array(_), Some(field_info)) if field_info.kind == PyTypeKind::List => {
                json_to_python(py, v, Some(field_info))?
            }
            // For other types, proceed normally
            _ => json_to_python(py, v, field_type)?,
        };

        partial_data.set_item(k, py_value)?;
    }

    // First try to use __gasp_from_partial__ method if available
    let py_type_repr_gasp = py_type.repr()?.extract::<String>()?;
    if let Ok(from_partial) = py_type.getattr("__gasp_from_partial__") {
        if let Ok(instance) = from_partial.call1((partial_data,)) {
            let type_repr_success = py_type.repr()?.extract::<String>()?;
            debug!(
                "[RUST:create_instance_from_json] Success via __gasp_from_partial__ for type: {}",
                type_repr_success
            );
            return Ok(instance.into());
        } else {
            let type_repr_fail = py_type.repr()?.extract::<String>()?;
            debug!("[RUST:create_instance_from_json] Call to __gasp_from_partial__ failed for type: {}", type_repr_fail);
        }
    } else {
        let type_repr_no_method = py_type.repr()?.extract::<String>()?;
        debug!(
            "[RUST:create_instance_from_json] Type {} does not have __gasp_from_partial__.",
            type_repr_no_method
        );
    }

    // For non-Deserializable classes (or if __gasp_from_partial__ failed), try to instantiate with kwargs
    let py_type_repr_kwargs = py_type.repr()?.extract::<String>()?;
    debug!(
        "[RUST:create_instance_from_json] Attempting kwargs instantiation for type: {}",
        py_type_repr_kwargs
    );
    if let Ok(instance) = py_type.call((), Some(partial_data)) {
        let type_repr_success = py_type.repr()?.extract::<String>()?;
        debug!(
            "[RUST:create_instance_from_json] Success via kwargs instantiation for type: {}",
            type_repr_success
        );
        return Ok(instance.into());
    } else {
        let type_repr_fail = py_type.repr()?.extract::<String>()?;
        debug!(
            "[RUST:create_instance_from_json] Kwargs instantiation failed for type: {}",
            type_repr_fail
        );
    }

    // Fallback to normal instantiation if __gasp_from_partial__ isn't available or kwargs failed
    let py_type_repr_call0 = py_type.repr()?.extract::<String>()?;
    debug!(
        "[RUST:create_instance_from_json] Attempting no-arg instantiation (call0) for type: {}",
        py_type_repr_call0
    );
    if let Ok(instance_obj) = py_type.call0() {
        let instance = instance_obj;
        let type_repr_success = py_type.repr()?.extract::<String>()?;
        debug!(
            "[RUST:create_instance_from_json] Success via no-arg instantiation for type: {}",
            type_repr_success
        );
        for (key_obj, value_obj) in partial_data.iter() {
            if let Ok(key_str) = key_obj.extract::<&str>() {
                if let Err(e) = instance.setattr(key_str, value_obj) {
                    let err_msg = format!("[RUST:create_instance_from_json] Error setting attribute '{}' on no-arg instance: {:?}", key_str, e);
                    debug!("{}", err_msg);
                }
            }
        }
        return Ok(instance.into());
    } else {
        let type_repr_fail = py_type.repr()?.extract::<String>()?;
        debug!(
            "[RUST:create_instance_from_json] No-arg instantiation (call0) failed for type: {}",
            type_repr_fail
        );
    }

    // If standard instantiation failed, but we have a specific class type,
    // try to create a "bare" instance using __new__ and then populate it.
    let py_type_repr_new_attempt = py_type.repr()?.extract::<String>()?;
    debug!("[RUST:create_instance_from_json] Standard instantiation failed for type {}. Attempting bare instance creation via __new__.", py_type_repr_new_attempt);
    if let Ok(new_method) = py_type.getattr("__new__") {
        if let Ok(bare_instance_any) = new_method.call1((py_type,)) {
            let bare_instance = bare_instance_any;
            let py_type_repr_new_success = py_type.repr()?.extract::<String>()?;
            debug!("[RUST:create_instance_from_json] Successfully created bare instance via __new__ for type: {}", py_type_repr_new_success);

            // Initialize all known fields to defaults first
            debug!("[RUST:create_instance_from_json] Initializing default fields for bare instance of type: {}", py_type_repr_new_success);
            for (field_name, field_type_info) in fields.iter() {
                let default_value = match field_type_info.kind {
                    PyTypeKind::String => pyo3::types::PyString::new(py, "").to_object(py),
                    PyTypeKind::Integer => 0_i32.to_object(py),
                    PyTypeKind::Float => 0.0_f32.to_object(py),
                    PyTypeKind::Boolean => false.to_object(py),
                    PyTypeKind::List | PyTypeKind::Tuple => PyList::empty(py).to_object(py),
                    PyTypeKind::Dict => PyDict::new(py).to_object(py),
                    _ => py.None(),
                };
                if let Err(e) = bare_instance.setattr(field_name.as_str(), default_value) {
                    let err_msg = format!("[RUST:create_instance_from_json] Error setting default attribute '{}' on bare instance: {:?}", field_name, e);
                    debug!("{}", err_msg);
                }
            }

            debug!("[RUST:create_instance_from_json] Populating bare instance with partial_data for type: {}", py_type_repr_new_success);
            for (key_obj, value_obj) in partial_data.iter() {
                if let Ok(key_str) = key_obj.extract::<&str>() {
                    if let Err(e) = bare_instance.setattr(key_str, value_obj) {
                        let err_msg = format!("[RUST:create_instance_from_json] Error setting attribute '{}' from partial_data on bare instance: {:?}", key_str, e);
                        debug!("{}", err_msg);
                    }
                }
            }
            let populated_bare_instance_repr = bare_instance.repr()?.extract::<String>()?;
            debug!(
                "[RUST:create_instance_from_json] Populated bare instance: {}",
                populated_bare_instance_repr
            );
            return Ok(bare_instance.into());
        } else {
            let py_type_repr_new_fail = py_type.repr()?.extract::<String>()?;
            debug!(
                "[RUST:create_instance_from_json] Call to __new__ failed for type: {}",
                py_type_repr_new_fail
            );
        }
    } else {
        let py_type_repr_no_new = py_type.repr()?.extract::<String>()?;
        debug!(
            "[RUST:create_instance_from_json] Type {} does not have __new__ method.",
            py_type_repr_no_new
        );
    }

    // Final fallback: if __new__ path also failed.
    let dict_type_obj = py.get_type::<PyDict>();
    let is_target_dict_type = py_type.get_type().eq(dict_type_obj)?;

    if !is_target_dict_type {
        let py_type_repr_final_none = py_type.repr()?.extract::<String>()?;
        Ok(py.None())
    } else {
        let py_type_repr_final_dict = py_type.repr()?.extract::<String>()?;
        Ok(partial_data.into())
    }
}

/// Update an existing Python instance with new JSON data
pub fn update_instance_from_json(
    py: Python,
    instance: &PyAny,
    map: &HashMap<String, JsonValue>,
    fields: &HashMap<String, PyTypeInfo>,
) -> PyResult<PyObject> {
    // If the instance has __gasp_update__ method, use it
    if let Ok(update_method) = instance.getattr("__gasp_update__") {
        let partial_data = PyDict::new(py);

        // Convert the JSON map to a Python dict
        for (k, v) in map {
            let field_type = fields.get(k);
            let py_value = json_to_python(py, v, field_type)?;
            partial_data.set_item(k, py_value)?;
        }

        update_method.call1((partial_data,))?;
        return Ok(instance.into());
    }

    // Otherwise, update attributes directly
    for (k, v) in map {
        let field_type = fields.get(k);

        // Process the value with the correct field type information
        let py_value = match (v, field_type) {
            // Process lists that contain complex objects
            (JsonValue::Array(arr), Some(field_info))
                if field_info.kind == PyTypeKind::List && !field_info.args.is_empty() =>
            {
                // Get the element type from the list's type args
                let elem_type = &field_info.args[0];
                process_list_with_element_type(py, arr, elem_type)?
            }
            // For nested objects, ensure we pass the type info
            (JsonValue::Object(_), Some(field_info)) if field_info.kind == PyTypeKind::Class => {
                json_to_python(py, v, Some(field_info))?
            }
            // For other lists
            (JsonValue::Array(_), Some(field_info)) if field_info.kind == PyTypeKind::List => {
                json_to_python(py, v, Some(field_info))?
            }
            // For other types, proceed normally
            _ => json_to_python(py, v, field_type)?,
        };

        instance.setattr(k.as_str(), py_value)?;
    }

    Ok(instance.into())
}
