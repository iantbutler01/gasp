use log::debug;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString}; // Added PyList

use crate::json_parser::StreamParser;
use crate::json_types::JsonValue;
use crate::python_types::{json_to_python, PyTypeInfo};

/// Wrapper for the StreamParser that handles typed conversions
#[derive(Debug)]
pub struct TypedStreamParser {
    stream_parser: StreamParser,
    type_info: Option<PyTypeInfo>,
    partial_data: Option<JsonValue>,
    partial_instance: Option<Py<PyAny>>, // Store the Python instance
    is_done: bool,
    pub expected_tags: Vec<String>,
    is_pydantic_model: bool, // Flag for Pydantic models
}

/// Helper function to check if brackets/braces are balanced
fn count_brackets(s: &str) -> i32 {
    let mut count = 0;
    for c in s.chars() {
        match c {
            '{' => count += 1,
            '}' => count -= 1,
            '[' => count += 1,
            ']' => count -= 1,
            _ => {}
        }
    }
    count
}

impl TypedStreamParser {
    pub fn new() -> Self {
        Self {
            stream_parser: StreamParser::new(Vec::new(), Vec::new()), // Now takes 2 args
            type_info: None,
            partial_data: None,
            partial_instance: None,
            is_done: false,
            expected_tags: Vec::new(),
            is_pydantic_model: false,
        }
    }

    pub fn with_type(type_info: PyTypeInfo) -> Self {
        Self {
            stream_parser: StreamParser::new(Vec::new(), Vec::new()), // Now takes 2 args
            type_info: Some(type_info), // Store the original type_info
            partial_data: None,
            partial_instance: None,
            is_done: false,
            expected_tags: Vec::new(),
            is_pydantic_model: false,
        }
    }

    /// Process a chunk of JSON data
    pub fn step(&mut self, chunk: &str) -> PyResult<Option<JsonValue>> {
        // Use the stream parser to process the chunk, passing stored type_info
        let result = match self.stream_parser.step(chunk) {
            Ok(val) => val,
            Err(e) => {
                return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "JSON parsing error: {:?}",
                    e
                )))
            }
        };

        // If we got a result, this is the new complete state of the partial data.
        if let Some(value) = result.clone() {
            // result is Option<JsonValue> from stream_parser.step
            self.partial_data = Some(value); // Directly use the new state

            // Check if parsing is complete based on stream parser state
            self.is_done = self.stream_parser.is_done();
        } else if chunk.is_empty() && !self.is_done {
            // If an empty chunk is fed and we are not done, it might be a signal to finalize
            // based on existing partial_data. The to_python_object will handle current partial_data.
            // self.is_done might be set by stream_parser if it finishes on empty chunk.
            self.is_done = self.stream_parser.is_done();
        }

        // The 'result' from stream_parser.step is Option<JsonValue> representing the latest yield.
        // We've stored it in self.partial_data.
        // The PyParser.feed() method will call self.to_python_object() which uses self.partial_data.
        // So, step itself doesn't need to return the JsonValue, PyParser.feed will get it via to_python_object.
        // However, the original design returned Option<JsonValue> here, which might be used by some callers
        // or tests directly if TypedStreamParser is used outside PyParser.
        // For now, let's keep returning it, though it's somewhat redundant with get_partial().
        Ok(result)
    }

    /// Get the current partial data
    pub fn get_partial(&self) -> Option<&JsonValue> {
        self.partial_data.as_ref()
    }

    /// Check if parsing is complete
    pub fn is_done(&self) -> bool {
        self.is_done
    }

    /// Convert current partial data to Python object using type info
    pub fn to_python_object(&mut self, py: Python) -> PyResult<Option<PyObject>> {
        let result_before_final_coercion: Option<PyObject> = match &self.partial_data {
            Some(data) => {
                if self.is_pydantic_model {
                    Some(json_to_python(py, data, self.type_info.as_ref())?)
                } else if let (Some(type_info_ref_outer), _) = (self.type_info.as_ref(), data) {
                    // Handle List[Class] case specifically for instance creation/update
                    if type_info_ref_outer.kind == crate::python_types::PyTypeKind::List
                        && !type_info_ref_outer.args.is_empty()
                    {
                        let element_type_info = &type_info_ref_outer.args[0];
                        if element_type_info.kind == crate::python_types::PyTypeKind::Class {
                            if let JsonValue::Object(map) = data {
                                // Only proceed if data is an object for class instantiation
                                if let Some(py_type_ref) = &element_type_info.py_type {
                                    let py_type_obj = py_type_ref.as_ref(py);
                                    // Try to update existing instance if it's for this element type
                                    if let Some(existing_instance) = &self.partial_instance {
                                        if existing_instance.as_ref(py).is_instance(py_type_obj)?
                                            && !existing_instance
                                                .as_ref(py)
                                                .is_instance_of::<PyDict>()
                                        {
                                            let updated_instance =
                                                crate::python_types::update_instance_from_json(
                                                    py,
                                                    existing_instance.as_ref(py),
                                                    map,
                                                    &element_type_info.fields,
                                                )?;
                                            // self.partial_instance remains the same, just updated
                                            Some(updated_instance)
                                        } else {
                                            // Existing instance is not of the correct type, or is a dict. Create new.
                                            let instance =
                                                crate::python_types::create_instance_from_json(
                                                    py,
                                                    py_type_obj,
                                                    map,
                                                    &element_type_info.fields,
                                                )?;
                                            if !instance.as_ref(py).is_instance_of::<PyDict>() {
                                                self.partial_instance =
                                                    Some(instance.clone_ref(py));
                                            }
                                            Some(instance)
                                        }
                                    } else {
                                        // No existing instance, create new
                                        let instance =
                                            crate::python_types::create_instance_from_json(
                                                py,
                                                py_type_obj,
                                                map,
                                                &element_type_info.fields,
                                            )?;
                                        if !instance.as_ref(py).is_instance_of::<PyDict>() {
                                            self.partial_instance = Some(instance.clone_ref(py));
                                        }
                                        Some(instance)
                                    }
                                } else {
                                    // No py_type_ref for element_type_info, fall back to general json_to_python
                                    Some(json_to_python(py, data, self.type_info.as_ref())?)
                                }
                            } else {
                                // Data is not an object, but List[Class] expected. Let json_to_python handle coercion.
                                Some(json_to_python(py, data, self.type_info.as_ref())?)
                            }
                        } else {
                            // Not List[Class], e.g. List[Union], List[str], or direct Class/Union etc.
                            Some(json_to_python(py, data, self.type_info.as_ref())?)
                        }
                    }
                    // Handle direct Class types (not in a List)
                    else if type_info_ref_outer.kind == crate::python_types::PyTypeKind::Class {
                        if let JsonValue::Object(map) = data {
                            if let Some(py_type_ref) = &type_info_ref_outer.py_type {
                                let py_type_obj = py_type_ref.as_ref(py);
                                // Try to update existing instance if it's for this type
                                if let Some(existing_instance) = &self.partial_instance {
                                    if existing_instance.as_ref(py).is_instance(py_type_obj)?
                                        && !existing_instance.as_ref(py).is_instance_of::<PyDict>()
                                    {
                                        let updated_instance =
                                            crate::python_types::update_instance_from_json(
                                                py,
                                                existing_instance.as_ref(py),
                                                map,
                                                &type_info_ref_outer.fields,
                                            )?;
                                        Some(updated_instance)
                                    } else {
                                        // Create new instance
                                        let instance =
                                            crate::python_types::create_instance_from_json(
                                                py,
                                                py_type_obj,
                                                map,
                                                &type_info_ref_outer.fields,
                                            )?;
                                        if !instance.as_ref(py).is_none()
                                            && !instance.as_ref(py).is_instance_of::<PyDict>()
                                        {
                                            self.partial_instance = Some(instance.clone_ref(py));
                                        }
                                        Some(instance)
                                    }
                                } else {
                                    // No existing instance, create new
                                    let instance = crate::python_types::create_instance_from_json(
                                        py,
                                        py_type_obj,
                                        map,
                                        &type_info_ref_outer.fields,
                                    )?;
                                    if !instance.as_ref(py).is_none()
                                        && !instance.as_ref(py).is_instance_of::<PyDict>()
                                    {
                                        self.partial_instance = Some(instance.clone_ref(py));
                                    }
                                    Some(instance)
                                }
                            } else {
                                Some(json_to_python(py, data, self.type_info.as_ref())?)
                            }
                        } else {
                            Some(json_to_python(py, data, self.type_info.as_ref())?)
                        }
                    }
                    // Handle Union types - try to create instances for Class types in the union
                    else if type_info_ref_outer.kind == crate::python_types::PyTypeKind::Union {
                        if let JsonValue::Object(map) = data {
                            // Check if any type in the union is a Class that we can instantiate
                            for union_arg in &type_info_ref_outer.args {
                                if union_arg.kind == crate::python_types::PyTypeKind::Class {
                                    if let Some(py_type_ref) = &union_arg.py_type {
                                        let py_type_obj = py_type_ref.as_ref(py);
                                        // Try to create instance for this union member
                                        if let Some(existing_instance) = &self.partial_instance {
                                            if existing_instance
                                                .as_ref(py)
                                                .is_instance(py_type_obj)?
                                                && !existing_instance
                                                    .as_ref(py)
                                                    .is_instance_of::<PyDict>()
                                            {
                                                let updated_instance =
                                                    crate::python_types::update_instance_from_json(
                                                        py,
                                                        existing_instance.as_ref(py),
                                                        map,
                                                        &union_arg.fields,
                                                    )?;
                                                return Ok(Some(updated_instance));
                                            }
                                        }
                                        // Try to create new instance for this union member
                                        let instance =
                                            crate::python_types::create_instance_from_json(
                                                py,
                                                py_type_obj,
                                                map,
                                                &union_arg.fields,
                                            )?;
                                        if !instance.as_ref(py).is_none()
                                            && !instance.as_ref(py).is_instance_of::<PyDict>()
                                        {
                                            self.partial_instance = Some(instance.clone_ref(py));
                                            return Ok(Some(instance));
                                        }
                                    }
                                }
                            }
                        }
                        // Fall back to general json_to_python for Union types
                        Some(json_to_python(py, data, self.type_info.as_ref())?)
                    }
                    // This case handles other types (List[str], Dict, etc.)
                    else {
                        Some(json_to_python(py, data, self.type_info.as_ref())?)
                    }
                } else {
                    // self.type_info is None
                    Some(json_to_python(py, data, None)?)
                }
            }
            None => None,
        };

        if let Some(py_obj_candidate) = result_before_final_coercion {
            if let Some(root_ti) = self.type_info.as_ref() {
                if root_ti.kind == crate::python_types::PyTypeKind::List {
                    if !py_obj_candidate.as_ref(py).is_instance_of::<PyList>() {
                        debug!("[to_python_object] Root type is List, but current object is not a list. Wrapping it.");
                        let list_wrapper = PyList::new(py, &[py_obj_candidate]);
                        return Ok(Some(list_wrapper.into()));
                    }
                }
            }
            Ok(Some(py_obj_candidate))
        } else {
            Ok(None)
        }
    }
}

/// Python wrapper for the TypedStreamParser
#[pyclass(name = "Parser", unsendable)]
pub struct PyParser {
    parser: TypedStreamParser,
}

#[pymethods]
impl PyParser {
    #[new]
    #[pyo3(signature = (type_obj=None, ignored_tags=vec!["think".to_string(), "thinking".to_string(), "system".to_string(), "thought".to_string()]))]
    fn new(py: Python, type_obj: Option<&PyAny>, ignored_tags: Vec<String>) -> PyResult<Self> {
        debug!(
            "[PyParser::new] type_obj: {:?}",
            type_obj.map(|o| o
                .repr()
                .unwrap_or_else(|_| PyString::new(py, "Error getting repr").into()))
        );
        match type_obj {
            Some(obj) => {
                // Extract type info from the Python type
                let mut type_info = PyTypeInfo::extract_from_python(obj)?;
                debug!(
                    "[PyParser::new] Extracted type_info: name='{}', kind='{:?}', origin='{:?}'",
                    type_info.name, type_info.kind, type_info.origin
                );

                // Make sure we store the original Python type reference
                if type_info.py_type.is_none() {
                    type_info.py_type = Some(obj.into_py(py));
                }

                // Get the type name to use as the expected tag
                let tag_name = match &type_info.kind {
                    crate::python_types::PyTypeKind::List => "list".to_string(),
                    crate::python_types::PyTypeKind::Tuple => "tuple".to_string(),
                    crate::python_types::PyTypeKind::Dict => "dict".to_string(),
                    _ => {
                        // For other types, try to get __name__ attribute
                        if let Ok(name_attr) = obj.getattr("__name__") {
                            name_attr.extract::<String>()?.to_lowercase() // Convert to lowercase
                        } else {
                            // Use the name from type_info if available, otherwise empty string
                            type_info.name.clone().to_lowercase() // Convert to lowercase
                        }
                    }
                };
                debug!(
                    "[PyParser::new] Determined tag_name (after potential lowercase): '{}'",
                    tag_name
                );

                // Create a parser that only looks for this specific tag
                let tags = if !tag_name.is_empty() {
                    vec![tag_name.clone()] // Clone tag_name here
                } else {
                    vec![]
                };
                debug!("[PyParser::new] tags for StreamParser: {:?}", tags);
                debug!(
                    "[PyParser::new] ignored_tags for StreamParser: {:?}",
                    ignored_tags
                );

                let mut parser = TypedStreamParser::with_type(type_info);
                parser.expected_tags = tags.clone();
                parser.stream_parser = StreamParser::new(tags, ignored_tags.clone());

                Ok(Self { parser })
            }
            None => {
                debug!("[PyParser::new] No type_obj provided.");
                let mut parser = TypedStreamParser::new();
                debug!(
                    "[PyParser::new] (No type_obj) ignored_tags for StreamParser: {:?}",
                    ignored_tags
                );
                parser.stream_parser = StreamParser::new(Vec::new(), ignored_tags.clone());

                Ok(Self { parser })
            }
        }
    }

    /// Create a parser for a Pydantic model
    #[staticmethod]
    #[pyo3(text_signature = "(pydantic_model)")]
    fn from_pydantic(py: Python, pydantic_model: &PyAny) -> PyResult<Self> {
        let mut type_info = PyTypeInfo::extract_from_python(pydantic_model)?;
        if type_info.py_type.is_none() {
            type_info.py_type = Some(pydantic_model.into_py(py));
        }

        let original_tag_name = if let Ok(name) = pydantic_model.getattr("__name__") {
            name.extract::<String>()?
        } else {
            type_info.name.clone()
        };
        let tag_name_lower = original_tag_name.to_lowercase();

        let tags_for_stream_parser = if !tag_name_lower.is_empty() {
            vec![tag_name_lower.clone()]
        } else {
            vec![]
        };

        let mut typed_stream_parser = TypedStreamParser::with_type(type_info);
        typed_stream_parser.expected_tags = if !original_tag_name.is_empty() {
            vec![original_tag_name]
        } else {
            vec![]
        };
        typed_stream_parser.is_pydantic_model = true;

        let default_ignored_tags = vec![
            "think".to_string(),
            "thinking".to_string(),
            "system".to_string(),
            "thought".to_string(),
        ];

        typed_stream_parser.stream_parser =
            StreamParser::new(tags_for_stream_parser, default_ignored_tags);

        Ok(Self {
            parser: typed_stream_parser,
        })
    }

    #[pyo3(text_signature = "($self, chunk)")]
    fn feed(&mut self, py: Python, chunk: &str) -> PyResult<Option<PyObject>> {
        self.parser.step(chunk)?;
        self.parser.to_python_object(py)
    }

    #[pyo3(text_signature = "($self)")]
    fn is_complete(&self) -> bool {
        self.parser.is_done()
    }

    #[pyo3(text_signature = "($self)")]
    fn get_partial(&mut self, py: Python) -> PyResult<Option<PyObject>> {
        self.parser.to_python_object(py)
    }

    #[pyo3(text_signature = "($self)")]
    fn validate(&mut self, py: Python) -> PyResult<Option<PyObject>> {
        self.get_partial(py)
    }
}
