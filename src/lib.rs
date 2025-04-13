use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

mod json_types;
mod parser_types;
mod rd_json_stack_parser;
mod template_parser;
mod types;
mod wail_parser;

use pyo3::types::{PyDict, PyFloat, PyList, PyLong, PyString};
use pyo3::Python;
use std::collections::HashMap;
use std::path::PathBuf;
use types::JsonValidationError;
use wail_parser::WAILFileType;

use crate::json_types::{JsonValue, Number};

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
    base_dir: PathBuf,
}

#[pymethods]
impl WAILGenerator {
    #[new]
    #[pyo3(text_signature = "(base_dir=None)")]
    fn new(base_dir: Option<String>) -> Self {
        let dir = match base_dir {
            Some(path) => PathBuf::from(path),
            None => std::env::current_dir().unwrap(),
        };

        Self {
            wail_content: String::new(),
            base_dir: dir,
        }
    }

    #[pyo3(text_signature = "(&self, base_dir)")]
    fn set_base_dir(&mut self, base_dir: String) {
        self.base_dir = PathBuf::from(base_dir);
    }

    /// Load WAIL schema content
    #[pyo3(text_signature = "($self, content)")]
    fn load_wail(&mut self, content: String) -> PyResult<Option<Py<PyDict>>> {
        use pyo3::types::PyDict;
        use pyo3::Python;

        self.wail_content = content;

        let parser = wail_parser::WAILParser::new(self.base_dir.clone());
        let res =
            parser.parse_wail_file(self.wail_content.clone(), WAILFileType::Application, true);

        match res {
            Ok(_) => Ok(None),
            Err(e) => Python::with_gil(|py| {
                let py_dict = PyDict::new(py);
                match e {
                    wail_parser::WAILParseError::UnexpectedToken { found, location } => {
                        py_dict.set_item("error_type", "UnexpectedToken")?;
                        py_dict.set_item("found", found)?;
                        py_dict.set_item("location", format!("{:?}", location))?;
                    }
                    wail_parser::WAILParseError::UnexpectedKeyword { found, location } => {
                        py_dict.set_item("error_type", "UnexpectedKeyword")?;
                        py_dict.set_item("found", found)?;
                        py_dict.set_item("location", format!("{:?}", location))?;
                    }
                    wail_parser::WAILParseError::UnexpectedEOF { expected, location } => {
                        py_dict.set_item("error_type", "UnexpectedEOF")?;
                        py_dict.set_item("expected", expected)?;
                        py_dict.set_item("location", format!("{:?}", location))?;
                    }
                    wail_parser::WAILParseError::InvalidIdentifier { found, location } => {
                        py_dict.set_item("error_type", "InvalidIdentifier")?;
                        py_dict.set_item("found", found)?;
                        py_dict.set_item("location", format!("{:?}", location))?;
                    }
                    wail_parser::WAILParseError::UndefinedType { name, location } => {
                        py_dict.set_item("error_type", "UndefinedType")?;
                        py_dict.set_item("name", name)?;
                        py_dict.set_item("location", format!("{:?}", location))?;
                    }
                    wail_parser::WAILParseError::DuplicateDefinition { name, location } => {
                        py_dict.set_item("error_type", "DuplicateDefinition")?;
                        py_dict.set_item("name", name)?;
                        py_dict.set_item("location", format!("{:?}", location))?;
                    }
                    wail_parser::WAILParseError::MissingMainBlock => {
                        py_dict.set_item("error_type", "MissingMainBlock")?;
                    }
                    wail_parser::WAILParseError::CircularImport { path, chain } => {
                        py_dict.set_item("error_type", "CircularImport")?;
                        py_dict.set_item("path", path)?;
                        py_dict.set_item("chain", chain)?;
                    }
                    wail_parser::WAILParseError::AmbiguousSymbol { name, matches } => {
                        py_dict.set_item("error_type", "AmbiguousSymbol")?;
                        py_dict.set_item("name", name)?;
                        py_dict.set_item("matches", matches)?;
                    }
                    wail_parser::WAILParseError::SymbolNotFound { name } => {
                        py_dict.set_item("error_type", "SymbolNotFound")?;
                        py_dict.set_item("name", name)?;
                    }
                    wail_parser::WAILParseError::InvalidImportPath { path, error } => {
                        py_dict.set_item("error_type", "InvalidImportPath")?;
                        py_dict.set_item("path", path)?;
                        py_dict.set_item("error", error)?;
                    }
                    wail_parser::WAILParseError::FileError { path, error } => {
                        py_dict.set_item("error_type", "FileError")?;
                        py_dict.set_item("path", path)?;
                        py_dict.set_item("error", error)?;
                    }
                    wail_parser::WAILParseError::ImportNotFound { name, path } => {
                        py_dict.set_item("error_type", "ImportNotFound")?;
                        py_dict.set_item("name", name)?;
                        py_dict.set_item("path", path)?;
                    }
                    wail_parser::WAILParseError::InvalidTemplateCall {
                        template_name,
                        reason,
                        location,
                    } => {
                        py_dict.set_item("error_type", "InvalidTemplateCall")?;
                        py_dict.set_item("template_name", template_name)?;
                        py_dict.set_item("reason", reason)?;
                        py_dict.set_item("location", format!("{:?}", location))?;
                    }
                }
                Ok(Some(py_dict.into()))
            }),
        }
    }

    #[pyo3(text_signature = "($self, **kwargs)", signature = (**kwargs))]
    fn get_prompt(
        &self,
        kwargs: Option<&PyDict>,
    ) -> PyResult<(Option<String>, Vec<String>, Vec<String>)> {
        let parser = wail_parser::WAILParser::new(self.base_dir.clone());

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

        // First parse and validate the WAIL schema
        match parser.parse_wail_file(self.wail_content.clone(), WAILFileType::Application, true) {
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
        let parser = wail_parser::WAILParser::new(self.base_dir.clone());

        // Parse WAIL schema first
        if let Err(e) =
            parser.parse_wail_file(self.wail_content.clone(), WAILFileType::Application, true)
        {
            return Err(PyValueError::new_err(format!(
                "Failed to parse WAIL schema: {:?}",
                e
            )));
        }

        // Parse and validate the LLM output
        let mut parsed_output = parser
            .parse_llm_output(&llm_output)
            .map_err(|e| PyValueError::new_err(format!("Failed to parse LLM output: {:?}", e)))?;

        parser.validate_and_fix(&mut parsed_output).map_err(|e| {
            PyValueError::new_err(format!("Failed to validate LLM output: {:?}", e))
        })?;

        // Only acquire the GIL when we need to create Python objects
        Python::with_gil(|py| Ok(json_value_to_py_object(py, &parsed_output)))
    }

    /// Validate the loaded WAIL schema and the LLM output against the schema
    #[pyo3(text_signature = "($self)")]
    fn validate_wail(&self) -> PyResult<(Vec<String>, Vec<String>)> {
        let parser = wail_parser::WAILParser::new(self.base_dir.clone());

        // First parse and validate the WAIL schema
        match parser.parse_wail_file(self.wail_content.clone(), WAILFileType::Application, true) {
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
    use std::env::var;

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

        let test_dir = std::env::current_dir().unwrap();
        let parser = wail_parser::WAILParser::new(test_dir);
        parser
            .parse_wail_file(schema.to_string(), WAILFileType::Application, true)
            .unwrap();

        let valid = r#"{"person": {"name": "Alice", "age": 25, "interests": ["coding"], "_type": "Person"}}"#;
        let res = parser.validate_json(valid);
        println!("res {:?}", res);
        assert!(res.is_ok());

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

        let test_dir = std::env::current_dir().unwrap();
        let parser = wail_parser::WAILParser::new(test_dir);
        parser
            .parse_wail_file(schema.to_string(), WAILFileType::Application, true)
            .unwrap();

        // Valid array of union objects
        let valid = r#"{"container": {
        "_type": "Container",
       "items": [
           {"message": "ok", "_type": "Success"},
           {"code": 404, "message": "Not found", "_type": "Error"}
       ]
   }}"#;
        let res = parser.validate_json(valid);
        println!("{:?}", res);
        assert!(res.is_ok());

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

            let test_dir = std::env::current_dir().unwrap();
            let parser = wail_parser::WAILParser::new(test_dir);
            parser
                .parse_wail_file(schema.to_string(), WAILFileType::Application, true)
                .unwrap();

            let valid_success = r#"
            <action>
            {"message": "ok", "_type": "Success"}
            </action>
            "#;
            let res = parser.parse_llm_output(valid_success);
            assert!(res.is_ok());
            let val = res.unwrap();

            let res2 = parser.validate_json(&val.to_string());
            assert!(res2.is_ok());

            let valid_error = r#"<action>
            {"code": 404, "_type": "Code"}
            </action>"#;
            let res = parser.parse_llm_output(valid_error);
            assert!(res.is_ok());
            assert!(parser.validate_json(&res.unwrap().to_string()).is_ok());

            let invalid = r#"<action>
            {"code": "404", "_type": "Code"}
            </action>"#;
            let res = parser.parse_llm_output(invalid);
            assert!(res.is_ok());
            assert!(parser.validate_json(&res.unwrap().to_string()).is_err());
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

            let test_dir = std::env::current_dir().unwrap();
            let parser = wail_parser::WAILParser::new(test_dir);
            parser
                .parse_wail_file(schema.to_string(), WAILFileType::Application, true)
                .unwrap();

            let valid_success = r#"<action>{"message": "ok", "_type": "Success"}</action>"#;
            let res = parser.parse_llm_output(valid_success);
            assert!(res.is_ok());
            assert!(parser.validate_json(&res.unwrap().to_string()).is_ok());

            let valid_error = r#"<action>{"code": 404, "_type": "Error"}</action>"#;
            let res = parser.parse_llm_output(valid_error);
            assert!(res.is_ok());
            assert!(parser.validate_json(&res.unwrap().to_string()).is_ok());

            let invalid = r#"<action>{"code": "404", "_type": "Error"}</action>"#;
            let res = parser.parse_llm_output(invalid);
            assert!(res.is_ok());
            assert!(parser.validate_json(&res.unwrap().to_string()).is_err());
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

            let test_dir = std::env::current_dir().unwrap();
            let parser = wail_parser::WAILParser::new(test_dir);
            parser
                .parse_wail_file(schema.to_string(), WAILFileType::Application, true)
                .unwrap();

            let valid = r#"<action>[
               {"message": "ok", "_type": "Success"},
               {"code": 404, "_type": "Error"}
           ]</action>"#;
            let res = parser.parse_llm_output(valid);
            assert!(res.is_ok());
            assert!(parser.validate_json(&res.unwrap().to_string()).is_ok());

            let invalid = r#"<action>[
               {"message": "ok", "_type": "Success"},
               {"code": "404", "_type": "Error"}
           ]</action>"#;
            let res = parser.parse_llm_output(invalid);
            assert!(res.is_ok());
            assert!(parser.validate_json(&res.unwrap().to_string()).is_err());
        }
    }

    #[test]
    fn test_bad_array_recovery() {
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

        let test_dir = std::env::current_dir().unwrap();
        let parser = wail_parser::WAILParser::new(test_dir);
        parser
            .parse_wail_file(schema.to_string(), WAILFileType::Application, true)
            .unwrap();

        let result = r#"
        <action>
            {"message": 123},
            {"code": "404", "details": "error"}
</action>
        "#;

        let mut json = parser.parse_llm_output(&result).unwrap();

        parser.validate_and_fix(&mut json).unwrap();

        println!("{:?}", json.to_string());
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

        let test_dir = std::env::current_dir().unwrap();
        let parser = wail_parser::WAILParser::new(test_dir);
        parser
            .parse_wail_file(schema.to_string(), WAILFileType::Application, true)
            .unwrap();

        // Test wrong type in array
        let wrong_type = r#"{"result": [
            {"message": 123},
            {"code": "404", "details": "error"}
        ]}"#;

        let err = parser.validate_json(wrong_type).unwrap_err();

        if let (_, _, JsonValidationError::ArrayElementTypeError((0, box_err))) = &err {
            println!("{:?}", box_err);

            if let JsonValidationError::NotMemberOfUnion((_, errors)) = &**box_err {
                let (_msg, box2_err) = errors.first().unwrap();

                let JsonValidationError::ObjectNestedTypeValidation((field, _)) = &**box2_err
                else {
                    panic!("not_field");
                };

                assert_eq!(*field, "message".to_string());
            } else {
                panic!("Expected ");
            }
        } else {
            panic!("Expected ArrayElementTypeError");
        }

        // Test invalid union type
        let invalid_union = r#"{"result": [
            {"something": "wrong"}
        ]}"#;
        let (_, _, err) = parser.validate_json(invalid_union).unwrap_err();
        assert!(matches!(err,
            JsonValidationError::ArrayElementTypeError((0, box_err))
            if matches!(*box_err, JsonValidationError::NotMemberOfUnion(_))
        ));

        // Test wrong type in nested field
        let wrong_nested = r#"{"result": [
            {"code": false, "details": "error"}
        ]}"#;
        let err = parser.validate_json(wrong_nested).unwrap_err();
        println!("{:?}", err);
        if let (_, _, JsonValidationError::ArrayElementTypeError((0, box_err))) = &err {
            println!("{:?}", box_err);

            if let JsonValidationError::NotMemberOfUnion((_, errors)) = &**box_err {
                let (_msg, box2_err) = errors.last().unwrap();

                println!("{:?}", box2_err);

                let JsonValidationError::ObjectNestedTypeValidation((field, _)) = &**box2_err
                else {
                    panic!("not_field");
                };

                assert_eq!(*field, "code".to_string());
            } else {
                panic!("Expected ");
            }
        } else {
            panic!("Expected ArrayElementTypeError");
        }
    }
    #[test]
    fn test_json_fix() {
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

        let test_dir = std::env::current_dir().unwrap();
        let parser = wail_parser::WAILParser::new(test_dir);
        parser
            .parse_wail_file(schema.to_string(), WAILFileType::Application, true)
            .unwrap();

        // Test wrong type in array
        let wrong_type = r#"{"result": [
            {"message": 123},
            {"code": "404", "details": "error"}
        ]}"#;

        use crate::rd_json_stack_parser::Parser;
        let mut json = Parser::new(wrong_type.as_bytes().to_vec()).parse().unwrap();

        // First validate to get the error
        parser.validate_and_fix(&mut json).unwrap();

        // Check that the fixes were applied correctly
        if let JsonValue::Object(obj) = &json {
            if let Some(JsonValue::Array(arr)) = obj.get("result") {
                if let Some(JsonValue::Object(first_obj)) = arr.get(0) {
                    if let Some(JsonValue::String(message)) = first_obj.get("message") {
                        assert_eq!(message, "123");
                    } else {
                        panic!("message was not converted to string");
                    }
                }
                if let Some(JsonValue::Object(second_obj)) = arr.get(1) {
                    if let Some(JsonValue::Number(Number::Integer(code))) = second_obj.get("code") {
                        assert_eq!(*code, 404);
                    } else {
                        panic!("code was not converted to number");
                    }
                }
            }
        }

        let wrong_type = r#"{"result":
            {"message": 123},
        }"#;

        let mut json = Parser::new(wrong_type.as_bytes().to_vec()).parse().unwrap();

        // First validate to get the error
        parser.validate_and_fix(&mut json).unwrap();

        // Check that the fixes were applied correctly
        if let JsonValue::Object(obj) = &json {
            if let Some(JsonValue::Array(arr)) = obj.get("result") {
                if let Some(JsonValue::Object(first_obj)) = arr.get(0) {
                    if let Some(JsonValue::String(message)) = first_obj.get("message") {
                        assert_eq!(message, "123");
                    } else {
                        panic!("message was not converted to string");
                    }
                }
            }
        }
    }
}
