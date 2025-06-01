use log::debug;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};

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
        // let type_info_clone_for_stream_parser = Some(type_info.clone()); // No longer needed for constructor
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
        let result = match self.stream_parser.step(chunk, self.type_info.as_ref()) {
            Ok(val) => val,
            Err(e) => {
                return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "JSON parsing error: {:?}",
                    e
                )))
            }
        };

        // If we got a result, update our partial data
        if let Some(value) = result.clone() {
            self.partial_data = Some(match self.partial_data.take() {
                Some(partial) => self.merge_json_values(partial, value),
                None => value,
            });

            // Check if parsing is complete based on stream parser state
            self.is_done = self.stream_parser.is_done();
        }

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
        match &self.partial_data {
            Some(data) => {
                // If it's a Pydantic model, we always just convert the current JSON data to a Python dict.
                // Pydantic models are expected to return dicts from feed() until validate() is called.
                if self.is_pydantic_model {
                    return Ok(Some(json_to_python(py, data, self.type_info.as_ref())?));
                }

                // For non-Pydantic classes, attempt incremental instance updates.
                if let (Some(type_info), JsonValue::Object(map)) = (self.type_info.as_ref(), data) {
                    if type_info.kind == crate::python_types::PyTypeKind::Class {
                        if let Some(py_type_ref) = &type_info.py_type {
                            let py_type_obj = py_type_ref.as_ref(py);

                            if let Some(existing_instance) = &self.partial_instance {
                                // Ensure existing_instance is not a dict before trying to update it like a class instance
                                if !existing_instance.as_ref(py).is_instance_of::<PyDict>() {
                                    let instance = crate::python_types::update_instance_from_json(
                                        py,
                                        existing_instance.as_ref(py),
                                        map,
                                        &type_info.fields,
                                    )?;
                                    return Ok(Some(instance));
                                } else {
                                    // existing_instance is a dict, this shouldn't happen if not pydantic
                                    // Fall through to full conversion from JSON
                                }
                            }
                            // No existing non-dict instance, or it was a dict (shouldn't be for non-pydantic)
                            // Create a new instance from full JSON data
                            let instance = crate::python_types::create_instance_from_json(
                                py,
                                py_type_obj,
                                map,
                                &type_info.fields,
                            )?;
                            // Only store if it's not a dict (i.e., it's a proper class instance)
                            if !instance.as_ref(py).is_instance_of::<PyDict>() {
                                self.partial_instance = Some(instance.clone_ref(py));
                            }
                            return Ok(Some(instance));
                        }
                    }
                }
                // Default conversion for non-class types or if class instance logic didn't apply
                Ok(Some(json_to_python(py, data, self.type_info.as_ref())?))
            }
            None => Ok(None),
        }
    }

    /// Merge two JSON values
    /// This handles incremental updates to the same object
    fn merge_json_values(&self, existing: JsonValue, new_data: JsonValue) -> JsonValue {
        match (existing, new_data) {
            // Merge objects by updating existing fields
            (JsonValue::Object(mut existing_map), JsonValue::Object(new_map)) => {
                for (key, value) in new_map {
                    match existing_map.get(&key) {
                        Some(existing_value) => {
                            // Recursively merge if both are objects or arrays
                            existing_map
                                .insert(key, self.merge_json_values(existing_value.clone(), value));
                        }
                        None => {
                            // Just add the new value
                            existing_map.insert(key, value);
                        }
                    }
                }
                JsonValue::Object(existing_map)
            }

            // For arrays, append or update elements
            (JsonValue::Array(mut existing_arr), JsonValue::Array(new_arr)) => {
                // Extend array if needed
                if new_arr.len() > existing_arr.len() {
                    existing_arr.resize_with(new_arr.len(), || JsonValue::Null);
                }

                // Update elements
                for (i, value) in new_arr.into_iter().enumerate() {
                    if i < existing_arr.len() {
                        existing_arr[i] = self.merge_json_values(existing_arr[i].clone(), value);
                    } else {
                        existing_arr.push(value);
                    }
                }
                JsonValue::Array(existing_arr)
            }

            // For other cases, just use the new value
            (_, new_value) => new_value,
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

                // Create the stream parser with ignored tags
                let mut parser = TypedStreamParser::with_type(type_info); // This initializes TypedStreamParser.stream_parser with type_info
                parser.expected_tags = tags.clone();
                // Configure the existing stream_parser's tags instead of replacing it, or ensure new one gets type_info
                // For now, let's assume tags are primarily for TagFinder, and StreamParser's type_info is primary for Builder
                // If tags are meant to re-initialize the StreamParser, it must also get the type_info.
                // The current TypedStreamParser.with_type already initializes its internal StreamParser with Vec::new() for tags.
                // We need to update that internal StreamParser if tags change here.
                // A cleaner way might be for TypedStreamParser::with_type to also take initial tags.
                // Or, add a method to TypedStreamParser to update tags, which re-initializes its StreamParser.

                // TypedStreamParser.with_type correctly initializes its internal StreamParser.
                // We now need to ensure the TagFinder within that StreamParser gets the correct tags.
                // This requires a method to update tags on StreamParser/TagFinder, or for new() to take them.
                // For now, we re-initialize StreamParser, ensuring it gets the correct number of args.
                // The type_info is managed by TypedStreamParser and passed via its step method.
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
        // Get the model fields
        let model_fields = if let Ok(fields) = pydantic_model.getattr("model_fields") {
            fields.downcast::<PyDict>()?
        } else if let Ok(fields) = pydantic_model.getattr("__fields__") {
            // Fallback for Pydantic v1
            fields.downcast::<PyDict>()?
        } else {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Not a valid Pydantic model",
            ));
        };

        // Extract type information
        let mut type_info = PyTypeInfo::extract_from_python(pydantic_model)?;

        // Make sure we store the original Python type reference
        if type_info.py_type.is_none() {
            type_info.py_type = Some(pydantic_model.into_py(py));
        }

        // Get the type name to use as the expected tag
        let original_tag_name = if let Ok(name) = pydantic_model.getattr("__name__") {
            name.extract::<String>()?
        } else {
            // Use the name from type_info if available, otherwise empty string
            type_info.name.clone()
        };
        let tag_name_lower = original_tag_name.to_lowercase();
        debug!(
            "[PyParser::from_pydantic] original_tag_name: '{}', lowercased: '{}'",
            original_tag_name, tag_name_lower
        );

        // Create a parser that only looks for this specific tag (lowercase)
        let tags_for_stream_parser = if !tag_name_lower.is_empty() {
            vec![tag_name_lower.clone()]
        } else {
            vec![]
        };
        debug!(
            "[PyParser::from_pydantic] tags_for_stream_parser: {:?}",
            tags_for_stream_parser
        );

        let mut typed_stream_parser = TypedStreamParser::with_type(type_info);
        // TypedStreamParser.expected_tags can keep original case if needed for other logic,
        // but StreamParser (and TagFinder) must use lowercase for matching.
        typed_stream_parser.expected_tags = if !original_tag_name.is_empty() {
            vec![original_tag_name]
        } else {
            vec![]
        };
        typed_stream_parser.is_pydantic_model = true; // Set the flag for Pydantic models

        let default_ignored_tags = vec![
            "think".to_string(),
            "thinking".to_string(),
            "system".to_string(),
            "thought".to_string(),
        ];
        debug!(
            "[PyParser::from_pydantic] default_ignored_tags for StreamParser: {:?}",
            default_ignored_tags
        );

        // Similar to PyParser::new logic.
        // The type_info is managed by TypedStreamParser and passed via its step method.
        typed_stream_parser.stream_parser =
            StreamParser::new(tags_for_stream_parser, default_ignored_tags);

        Ok(Self {
            parser: typed_stream_parser,
        })
    }

    /// Feed a chunk of JSON data and return a partial object if available
    #[pyo3(text_signature = "($self, chunk)")]
    fn feed(&mut self, py: Python, chunk: &str) -> PyResult<Option<PyObject>> {
        // Process the chunk
        self.parser.step(chunk)?;

        // Convert to Python object if we have data
        self.parser.to_python_object(py)
    }

    /// Check if parsing is complete
    #[pyo3(text_signature = "($self)")]
    fn is_complete(&self) -> bool {
        self.parser.is_done()
    }

    /// Get the current partial object without validation
    #[pyo3(text_signature = "($self)")]
    fn get_partial(&mut self, py: Python) -> PyResult<Option<PyObject>> {
        self.parser.to_python_object(py)
    }

    /// Perform full validation on the completed object
    #[pyo3(text_signature = "($self)")]
    fn validate(&mut self, py: Python) -> PyResult<Option<PyObject>> {
        // For now, just return the partial object
        // In future, we'll implement validation against the model
        self.get_partial(py)
    }
}
