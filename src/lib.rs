#![feature(box_patterns)]
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

mod json_types;
mod parser_types;
mod rd_json_stack_parser;
mod types;
mod wail_parser;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while1},
    character::complete::{alpha1, char, multispace0, multispace1},
    combinator::opt,
    multi::{many0, separated_list0},
    sequence::{delimited, preceded, tuple},
    IResult,
};

use pyo3::types::{PyDict, PyFloat, PyList, PyLong, PyString};
use pyo3::Python;
use std::collections::HashMap;

use crate::json_types::{JsonValue, Number};

use rd_json_stack_parser::Parser as JsonParser;

fn json_value_to_py_object(py: Python, value: &JsonValue) -> PyObject {
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

/// Python wrapper for WAIL validation
#[pyclass]
#[derive(Debug)]
struct WAILGenerator {
    wail_content: String,
}

#[pymethods]
impl WAILGenerator {
    #[new]
    fn new() -> Self {
        Self {
            wail_content: String::new(),
        }
    }

    /// Load WAIL schema content
    #[pyo3(text_signature = "($self, content)")]
    fn load_wail(&mut self, content: String) -> PyResult<()> {
        self.wail_content = content;
        Ok(())
    }

    #[pyo3(text_signature = "($self, **kwargs)", signature = (**kwargs))]
    fn get_prompt(
        &self,
        kwargs: Option<&PyDict>,
    ) -> PyResult<(Option<String>, Vec<String>, Vec<String>)> {
        let parser = wail_parser::WAILParser::new();

        // Convert kwargs to HashMap<String, JsonValue> if provided
        let template_arg_values = if let Some(kwargs) = kwargs {
            let mut arg_dict = HashMap::new();

            for (key, value) in kwargs.iter() {
                let key_str = key.extract::<String>()?;
                // Convert Python values to JsonValue
                let json_value = if value.is_instance_of::<PyString>() {
                    JsonValue::String(value.extract::<String>()?)
                } else if value.is_instance_of::<PyFloat>() {
                    JsonValue::Number(Number::Float(value.extract::<f64>()?))
                } else if value.is_instance_of::<PyLong>() {
                    JsonValue::Number(Number::Integer(value.extract::<i64>()?))
                } else {
                    return Err(PyValueError::new_err(format!(
                        "Unsupported type for template argument: {}",
                        key_str
                    )));
                };
                arg_dict.insert(key_str, json_value);
            }
            Some(arg_dict)
        } else {
            None
        };

        println!("template_arg_values: {:?}", template_arg_values);

        // First parse and validate the WAIL schema
        match parser.parse_wail_file(&self.wail_content) {
            Ok(_) => {
                let (warnings, errors) = parser.validate();

                // Convert warnings to strings
                let warning_strs: Vec<String> = warnings
                    .iter()
                    .map(|w| match w {
                        wail_parser::ValidationWarning::UndefinedType {
                            type_name,
                            location,
                        } => format!("Undefined type '{}' at {}", type_name, location),
                        wail_parser::ValidationWarning::PossibleTypo {
                            type_name,
                            similar_to,
                            location,
                        } => format!(
                            "Possible typo: '{}' might be '{}' at {}",
                            type_name, similar_to, location
                        ),
                        wail_parser::ValidationWarning::NoMainBlock => {
                            "No main block found in WAIL schema".to_string()
                        }
                    })
                    .collect();

                // Convert errors to strings
                let error_strs: Vec<String> = errors
                    .iter()
                    .map(|e| match e {
                        wail_parser::ValidationError::UndefinedTypeInTemplate {
                            template_name,
                            type_name,
                            is_return_type,
                        } => {
                            let type_kind = if *is_return_type {
                                "return type"
                            } else {
                                "parameter type"
                            };
                            format!(
                                "Undefined {} '{}' in template '{}'",
                                type_kind, type_name, template_name
                            )
                        }
                    })
                    .collect();

                if errors.is_empty() {
                    Ok((
                        Some(parser.prepare_prompt(template_arg_values.as_ref())),
                        warning_strs,
                        error_strs,
                    ))
                } else {
                    Ok((None, warning_strs, error_strs))
                }
            }
            Err(e) => Err(PyValueError::new_err(format!(
                "Failed to parse WAIL schema: {:?}",
                e
            ))),
        }
    }

    #[pyo3(text_signature = "($self, llm_output)")]
    fn parse_llm_output(&self, llm_output: String) -> PyResult<PyObject> {
        // Do all JSON parsing and validation outside the GIL
        let parser = wail_parser::WAILParser::new();

        // Parse WAIL schema first
        if let Err(e) = parser.parse_wail_file(&self.wail_content) {
            return Err(PyValueError::new_err(format!(
                "Failed to parse WAIL schema: {:?}",
                e
            )));
        }

        // Parse and validate the LLM output
        let parsed_output = parser
            .parse_llm_output(&llm_output)
            .map_err(|e| PyValueError::new_err(format!("Failed to parse LLM output: {:?}", e)))?;

        parser
            .validate_json(&parsed_output.to_string())
            .map_err(|e| {
                PyValueError::new_err(format!("Failed to validate LLM output: {:?}", e))
            })?;

        // Only acquire the GIL when we need to create Python objects
        Python::with_gil(|py| Ok(json_value_to_py_object(py, &parsed_output)))
    }

    /// Validate the loaded WAIL schema and the LLM output against the schema
    #[pyo3(text_signature = "($self)")]
    fn validate_wail(&self) -> PyResult<(Vec<String>, Vec<String>)> {
        let parser = wail_parser::WAILParser::new();

        // First parse and validate the WAIL schema
        match parser.parse_wail_file(&self.wail_content) {
            Ok(_) => {
                let (warnings, errors) = parser.validate();

                // Convert warnings to strings
                let warning_strs: Vec<String> = warnings
                    .iter()
                    .map(|w| match w {
                        wail_parser::ValidationWarning::UndefinedType {
                            type_name,
                            location,
                        } => format!("Undefined type '{}' at {}", type_name, location),
                        wail_parser::ValidationWarning::PossibleTypo {
                            type_name,
                            similar_to,
                            location,
                        } => format!(
                            "Possible typo: '{}' might be '{}' at {}",
                            type_name, similar_to, location
                        ),
                        wail_parser::ValidationWarning::NoMainBlock => {
                            "No main block found in WAIL schema".to_string()
                        }
                    })
                    .collect();

                // Convert errors to strings
                let error_strs: Vec<String> = errors
                    .iter()
                    .map(|e| match e {
                        wail_parser::ValidationError::UndefinedTypeInTemplate {
                            template_name,
                            type_name,
                            is_return_type,
                        } => {
                            let type_kind = if *is_return_type {
                                "return type"
                            } else {
                                "parameter type"
                            };
                            format!(
                                "Undefined {} '{}' in template '{}'",
                                type_kind, type_name, template_name
                            )
                        }
                    })
                    .collect();

                Ok((warning_strs, error_strs))
            }
            Err(e) => Err(PyValueError::new_err(format!(
                "Failed to parse WAIL schema: {:?}",
                e
            ))),
        }
    }
}

/// A Python module for working with WAIL files
#[pymodule]
fn gasp(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<WAILGenerator>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wail_validation() {
        let schema = r#"
    object Person {
        name: String 
        age: Number
        interests: String[]
    }
    template GetPerson() -> Person {
        prompt: """Test""" 
    }
    main {
        let person = GetPerson();
        prompt { {{person}} }
    }"#;

        let parser = wail_parser::WAILParser::new();
        parser.parse_wail_file(schema).unwrap();

        let valid = r#"{"person": {"name": "Alice", "age": 25, "interests": ["coding"]}}"#;
        assert!(parser.validate_json(valid).is_ok());

        let invalid_types = r#"{"person": {"name": 42, "age": "25", "interests": "coding"}}"#;
        assert!(parser.validate_json(invalid_types).is_err());

        let missing_field = r#"{"person": {"name": "Alice", "interests": ["coding"]}}"#;
        assert!(parser.validate_json(missing_field).is_err());
    }

    #[test]
    fn test_union_validation() {
        let schema = r#"
   object Success {
       message: String
   }

   object Error {
       code: Number
       message: String
   }

   union Response = Success | Error;
   
   object Container {
       items: Response[]
   }

   template Test() -> Container {
       prompt: """Test"""
   }

   main {
       let container = Test();
       prompt { {{container}} }
   }"#;

        let parser = wail_parser::WAILParser::new();
        parser.parse_wail_file(schema).unwrap();

        // Valid array of union objects
        let valid = r#"{"container": {
       "items": [
           {"message": "ok"},
           {"code": 404, "message": "Not found"}
       ]
   }}"#;
        assert!(parser.validate_json(valid).is_ok());

        // Invalid - object missing required field
        let invalid_obj = r#"{"container": {
       "items": [{"code": 500}]
   }}"#;

        println!("{:?}", parser.validate_json(invalid_obj));
        assert!(parser.validate_json(invalid_obj).is_err());

        // Invalid - wrong type for field
        let invalid_type = r#"{"container": {
       "items": [{"code": "500", "message": 404}]
   }}"#;
        assert!(parser.validate_json(invalid_type).is_err());
    }

    #[test]
    fn test_union_template_returns() {
        // Test 1: Inline union return
        {
            let schema = r#"
           object Success { message: String }
           object Error { code: Number }
           
           template Test() -> Success | Error {
               prompt: """Test"""
           }
           
           main {
               let result = Test();
               prompt { {{result}} }
           }"#;

            let parser = wail_parser::WAILParser::new();
            parser.parse_wail_file(schema).unwrap();

            let valid_success = r#"{"result": {"message": "ok"}}"#;
            assert!(parser.parse_llm_output(valid_success).is_ok());
            assert!(parser.validate_json(valid_success).is_ok());

            let valid_error = r#"{"result": {"code": 404}}"#;
            assert!(parser.parse_llm_output(valid_error).is_ok());
            assert!(parser.validate_json(valid_error).is_ok());

            let invalid = r#"{"result": {"code": "404"}}"#;
            assert!(parser.parse_llm_output(invalid).is_ok());
            assert!(parser.validate_json(invalid).is_err());
        }

        // Test 2: Named union return
        {
            let schema = r#"
           object Success { message: String }
           object Error { code: Number }
           union Response = Success | Error;
           
           template Test() -> Response {
               prompt: """Test"""
           }
           
           main {
               let result = Test();
               prompt { {{result}} }
           }"#;

            let parser = wail_parser::WAILParser::new();
            parser.parse_wail_file(schema).unwrap();

            let valid_success = r#"{"result": {"message": "ok"}}"#;
            assert!(parser.parse_llm_output(valid_success).is_ok());
            assert!(parser.validate_json(valid_success).is_ok());

            let valid_error = r#"{"result": {"code": 404}}"#;
            assert!(parser.parse_llm_output(valid_error).is_ok());
            assert!(parser.validate_json(valid_error).is_ok());

            let invalid = r#"{"result": {"code": "404"}}"#;
            assert!(parser.parse_llm_output(invalid).is_ok());
            assert!(parser.validate_json(invalid).is_err());
        }

        // Test 3: Array of named union return
        {
            let schema = r#"
           object Success { message: String }
           object Error { code: Number }
           union Response = Success | Error;
           
           template Test() -> Response[] {
               prompt: """Test"""
           }
           
           main {
               let result = Test();
               prompt { {{result}} }
           }"#;

            let parser = wail_parser::WAILParser::new();
            parser.parse_wail_file(schema).unwrap();

            let valid = r#"{"result": [
               {"message": "ok"},
               {"code": 404}
           ]}"#;
            assert!(parser.parse_llm_output(valid).is_ok());
            assert!(parser.validate_json(valid).is_ok());

            let invalid = r#"{"result": [
               {"message": "ok"},
               {"code": "404"}
           ]}"#;
            assert!(parser.parse_llm_output(invalid).is_ok());
            assert!(parser.validate_json(invalid).is_err());
        }
    }

    #[test]
    fn test_validation_error_messages() {
        let schema = r#"
    object Success { message: String }
    object Error { 
        code: Number
        details: String
    }
    union Response = Success | Error;
    
    template Test() -> Response[] {
        prompt: """Test"""
    }
    
    main {
        let result = Test();
        prompt { {{result}} }
    }"#;

        let parser = wail_parser::WAILParser::new();
        parser.parse_wail_file(schema).unwrap();

        // Test wrong type in array
        let wrong_type = r#"{"result": [
        {"message": 123},
        {"code": "404", "details": "error"}
    ]}"#;
        let err = parser.validate_json(wrong_type).unwrap_err();
        assert!(err.contains("Array element at index 0"));
        assert!(err.contains("Field 'message'"));
        assert!(err.contains("expected String"));

        // Test invalid union type
        let invalid_union = r#"{"result": [
        {"something": "wrong"}
    ]}"#;
        let err = parser.validate_json(invalid_union).unwrap_err();
        println!("{:?}", err);
        assert!(err.contains("Array element at index 0"));
        assert!(err.contains("Expected one of: Success | Error"));

        // Test wrong type in nested field
        let wrong_nested = r#"{"result": [
        {"code": false, "details": "error"}
    ]}"#;
        let err = parser.validate_json(wrong_nested).unwrap_err();
        assert!(err.contains("Array element at index 0"));
        assert!(err.contains("Field 'code'"));
        assert!(err.contains("expected Number"));
    }
}
