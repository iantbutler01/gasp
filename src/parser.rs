use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::json_parser::StreamParser;
use crate::json_types::JsonValue;
use crate::python_types::{json_to_python, PyTypeInfo};

/// Wrapper for the StreamParser that handles typed conversions
#[derive(Debug)]
pub struct TypedStreamParser {
    stream_parser: StreamParser,
    type_info: Option<PyTypeInfo>,
    partial_data: Option<JsonValue>,
    is_done: bool,
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
            stream_parser: StreamParser::default(),
            type_info: None,
            partial_data: None,
            is_done: false,
        }
    }

    pub fn with_type(type_info: PyTypeInfo) -> Self {
        Self {
            stream_parser: StreamParser::default(),
            type_info: Some(type_info),
            partial_data: None,
            is_done: false,
        }
    }

    /// Process a chunk of JSON data
    pub fn step(&mut self, chunk: &str) -> PyResult<Option<JsonValue>> {
        // Use the stream parser to process the chunk directly (with its tags)
        let result = match self.stream_parser.step(chunk) {
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
    pub fn to_python_object(&self, py: Python) -> PyResult<Option<PyObject>> {
        match &self.partial_data {
            Some(data) => Ok(Some(json_to_python(py, data, self.type_info.as_ref())?)),
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
    #[pyo3(text_signature = "(type_obj=None)")]
    fn new(py: Python, type_obj: Option<&PyAny>) -> PyResult<Self> {
        match type_obj {
            Some(obj) => {
                // Extract type info from the Python type
                let type_info = PyTypeInfo::extract_from_python(obj)?;
                Ok(Self {
                    parser: TypedStreamParser::with_type(type_info),
                })
            }
            None => Ok(Self {
                parser: TypedStreamParser::new(),
            }),
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
        let type_info = PyTypeInfo::extract_from_python(pydantic_model)?;

        Ok(Self {
            parser: TypedStreamParser::with_type(type_info),
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
    fn get_partial(&self, py: Python) -> PyResult<Option<PyObject>> {
        self.parser.to_python_object(py)
    }

    /// Perform full validation on the completed object
    #[pyo3(text_signature = "($self)")]
    fn validate(&self, py: Python) -> PyResult<Option<PyObject>> {
        // For now, just return the partial object
        // In future, we'll implement validation against the model
        self.get_partial(py)
    }
}
