use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyType};
use std::collections::HashMap;

use crate::json_types::{JsonValue, Number};

#[derive(Debug, Clone, PartialEq)]
pub enum PyTypeKind {
    Any,
    String,
    Integer,
    Float,
    Boolean,
    List,
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

    /// Get the most specific type for this field
    pub fn get_most_specific_type(&self) -> PyTypeKind {
        if self.is_optional {
            if self.args.len() == 1 {
                return self.args[0].get_most_specific_type();
            }
            return PyTypeKind::Any;
        }

        match self.kind {
            PyTypeKind::Union => {
                if self.args.is_empty() {
                    PyTypeKind::Any
                } else {
                    // For now, just return the first type in the union
                    // More sophisticated union handling would go here
                    self.args[0].get_most_specific_type()
                }
            }
            _ => self.kind.clone(),
        }
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
            match origin_name.as_str() {
                "list" => {
                    return Ok(PyTypeInfo::new(PyTypeKind::List, "list".to_string())
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
                } else if let Ok(bases) = py_type.getattr("__bases__") {
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
        JsonValue::Object(map) => {
            let dict = PyDict::new(py);

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

            // Fall back to regular dict
            for (k, v) in map {
                let field_type = if let Some(PyTypeInfo {
                    kind: PyTypeKind::Class,
                    fields,
                    ..
                }) = type_info
                {
                    fields.get(k)
                } else {
                    None
                };

                dict.set_item(k, json_to_python(py, v, field_type)?)?;
            }

            Ok(dict.into())
        }
        JsonValue::Array(arr) => {
            let list = PyList::empty(py);

            // Get element type if available
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
        JsonValue::String(s) => {
            // If type is enum, try to convert to enum
            if let Some(PyTypeInfo {
                kind: PyTypeKind::Class,
                name,
                module,
                py_type,
                ..
            }) = type_info
            {
                // First try using the stored Python type reference
                if let Some(py_type_ref) = py_type {
                    let py_type_obj = py_type_ref.as_ref(py);
                    // Try to get the enum value
                    if let Ok(enum_value) = py_type_obj.getattr(s.as_str()) {
                        return Ok(enum_value.into());
                    }
                }

                // Fall back to module-based lookup
                if let Some(module_name) = module {
                    if module_name == "enum" {
                        // Try to get the enum class
                        if let Ok(py_module) = py.import(module_name.as_str()) {
                            if let Ok(py_enum) = py_module.getattr(name.as_str()) {
                                // Try to get the enum value
                                if let Ok(enum_value) = py_enum.getattr(s.as_str()) {
                                    return Ok(enum_value.into());
                                }
                            }
                        }
                    }
                }
            }

            Ok(s.clone().into_py(py))
        }
        JsonValue::Number(n) => match n {
            Number::Integer(i) => {
                // Check if we have a type reference to use directly
                if let Some(PyTypeInfo { py_type, kind, .. }) = type_info {
                    if let Some(py_type_ref) = py_type {
                        let py_type_obj = py_type_ref.as_ref(py);

                        // Convert based on the actual Python type
                        match kind {
                            PyTypeKind::Float => Ok((*i as f64).into_py(py)),
                            PyTypeKind::Integer => Ok(i.into_py(py)),
                            // Try to construct the type with the value
                            _ => {
                                if let Ok(result) = py_type_obj.call1((*i,)) {
                                    return Ok(result.into());
                                }
                                // Fall back to default conversion
                                Ok(i.into_py(py))
                            }
                        }
                    } else if kind == &PyTypeKind::Float {
                        // Fall back to float conversion if we know it's a float
                        Ok((*i as f64).into_py(py))
                    } else {
                        Ok(i.into_py(py))
                    }
                } else {
                    Ok(i.into_py(py))
                }
            }
            Number::Float(f) => {
                // Check if we have a type reference to use directly
                if let Some(PyTypeInfo { py_type, .. }) = type_info {
                    if let Some(py_type_ref) = py_type {
                        let py_type_obj = py_type_ref.as_ref(py);

                        // Try to construct the type with the value
                        if let Ok(result) = py_type_obj.call1((*f,)) {
                            return Ok(result.into());
                        }
                    }
                }
                // Fall back to default conversion
                Ok(f.into_py(py))
            }
        },
        JsonValue::Boolean(b) => Ok(b.into_py(py)),
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
    if let Ok(from_partial) = py_type.getattr("__gasp_from_partial__") {
        if let Ok(instance) = from_partial.call1((partial_data,)) {
            return Ok(instance.into());
        }
    }

    // Fallback to normal instantiation if __gasp_from_partial__ isn't available
    if let Ok(instance) = py_type.call0() {
        // Populate fields manually
        for (k, v) in map {
            let field_type = fields.get(k);

            // Process the value with the correct field type information for setting attributes
            let py_value = match (v, field_type) {
                // For nested objects, ensure we pass the type info
                (JsonValue::Object(_), Some(field_info))
                    if field_info.kind == PyTypeKind::Class =>
                {
                    json_to_python(py, v, Some(field_info))?
                }
                // For lists with type info, ensure each element gets proper typing
                (JsonValue::Array(_), Some(field_info)) if field_info.kind == PyTypeKind::List => {
                    json_to_python(py, v, Some(field_info))?
                }
                // For other types, proceed normally
                _ => json_to_python(py, v, field_type)?,
            };

            instance.setattr(k.as_str(), py_value)?;
        }
        return Ok(instance.into());
    }

    // If we couldn't create an instance, fall back to returning a dict
    let dict = PyDict::new(py);
    for (k, v) in map {
        let field_type = fields.get(k);
        let py_value = json_to_python(py, v, field_type)?;
        dict.set_item(k, py_value)?;
    }
    Ok(dict.into())
}

/// Convert a Python object to a JsonValue
pub fn python_to_json(py: Python, obj: &PyAny) -> PyResult<JsonValue> {
    if obj.is_none() {
        return Ok(JsonValue::Null);
    }

    if let Ok(s) = obj.extract::<String>() {
        return Ok(JsonValue::String(s));
    }

    if let Ok(i) = obj.extract::<i64>() {
        return Ok(JsonValue::Number(Number::Integer(i)));
    }

    if let Ok(f) = obj.extract::<f64>() {
        return Ok(JsonValue::Number(Number::Float(f)));
    }

    if let Ok(b) = obj.extract::<bool>() {
        return Ok(JsonValue::Boolean(b));
    }

    if let Ok(list) = obj.downcast::<PyList>() {
        let mut arr = Vec::new();
        for item in list.iter() {
            arr.push(python_to_json(py, item)?);
        }
        return Ok(JsonValue::Array(arr));
    }

    if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut map = HashMap::new();
        for (k, v) in dict.iter() {
            let key = k.extract::<String>()?;
            map.insert(key, python_to_json(py, v)?);
        }
        return Ok(JsonValue::Object(map));
    }

    // If it's an object with __dict__
    if let Ok(dict) = obj.getattr("__dict__") {
        if let Ok(py_dict) = dict.downcast::<PyDict>() {
            let mut map = HashMap::new();
            for (k, v) in py_dict.iter() {
                let key = k.extract::<String>()?;
                if !key.starts_with('_') {
                    // Skip private attributes
                    map.insert(key, python_to_json(py, v)?);
                }
            }
            return Ok(JsonValue::Object(map));
        }
    }

    // Default: convert to string representation
    let repr = obj.repr()?.extract::<String>()?;
    Ok(JsonValue::String(repr))
}
