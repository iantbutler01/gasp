use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

mod json_parser;
mod json_sax_scanner;
mod json_tok;
mod json_types;
mod parser;
mod python_types;
mod tag_finder;

use json_parser::StreamParser;
use parser::PyParser;
use pyo3::types::{PyDict, PyList};
use pyo3::Python;

use crate::json_types::{JsonValue, Number};

pub fn to_py(py: Python, value: &JsonValue) -> PyObject {
    match value {
        JsonValue::Object(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                dict.set_item(k, json_value_to_py_object(py, v)).unwrap();
            }
            dict.into()
        }
        JsonValue::Array(arr) => {
            let list = PyList::empty(py);
            for item in arr {
                list.append(json_value_to_py_object(py, item)).unwrap();
            }
            list.into()
        }
        JsonValue::String(s) => s.into_py(py),
        JsonValue::Number(n) => match n {
            Number::Integer(i) => i.into_py(py),
            Number::Float(f) => f.into_py(py),
        },
        JsonValue::Boolean(b) => b.into_py(py),
        JsonValue::Null => py.None(),
    }
}

pub fn json_value_to_py_object(py: Python, value: &JsonValue) -> PyObject {
    match value {
        JsonValue::Object(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                dict.set_item(k, json_value_to_py_object(py, v)).unwrap();
            }
            dict.into()
        }
        JsonValue::Array(arr) => {
            let list = PyList::empty(py);
            for item in arr {
                list.append(json_value_to_py_object(py, item)).unwrap();
            }
            list.into()
        }
        JsonValue::String(s) => s.into_py(py),
        JsonValue::Number(n) => match n {
            Number::Integer(i) => i.into_py(py),
            Number::Float(f) => f.into_py(py),
        },
        JsonValue::Boolean(b) => b.into_py(py),
        JsonValue::Null => py.None(),
    }
}

/// A simple StreamParser class for Python
#[pyclass(name = "StreamParser", unsendable)]
struct PyStreamParser {
    parser: StreamParser,
    last_val: Option<JsonValue>,
}

#[pymethods]
impl PyStreamParser {
    #[new]
    fn new() -> Self {
        Self {
            parser: StreamParser::default(),
            last_val: None,
        }
    }

    /// Feed a chunk; returns the parsed value once complete, else `None`.
    #[pyo3(text_signature = "($self, chunk)")]
    fn parse<'p>(&mut self, py: Python<'p>, chunk: &str) -> PyResult<Option<PyObject>> {
        // Pass None for root_target_type as PyStreamParser is not type-aware in its Python API
        let step_out = self
            .parser
            .step(chunk, None) // This call should already be correct from previous attempt.
            .map_err(|e| PyValueError::new_err(format!("stream error: {:?}", e)))?;

        if let Some(val) = step_out {
            self.last_val = Some(val.clone());
            return Ok(Some(to_py(py, &val)));
        }
        Ok(None)
    }

    /// Check if the parser is done
    #[pyo3(text_signature = "($self)")]
    fn is_done(&self) -> bool {
        self.parser.is_done()
    }
}

/// Python module for parsing structured outputs into typed objects
#[pymodule]
fn gasp(py: Python, m: &PyModule) -> PyResult<()> {
    // Initialize the logger. try_init() is used to avoid panic if already initialized.
    let _ = env_logger::try_init();

    // Add base parser
    m.add_class::<PyStreamParser>()?;

    // Add typed parser
    m.add_class::<PyParser>()?;

    Ok(())
}
