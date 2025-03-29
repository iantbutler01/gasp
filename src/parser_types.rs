use crate::json_types::{JsonValue, Number};
use crate::types::*;
use regex;
use std::collections::HashMap;
use std::env::var;
use std::fmt::Display;
use std::marker::PhantomData;

#[derive(Debug, Clone, PartialEq)]
pub enum WAILAnnotation {
    Description(String), // Detailed explanation of purpose/meaning
    Example(String),     // Concrete examples of valid values/usage
    Validation(String),  // Rules about what makes a valid value
    Format(String),      // Expected text format or structure
    Important(String),   // Critical information the LLM should pay special attention to
    Context(String),     // Additional context about where/how this is used
    Default(String),     // Default/fallback value if not specified
    Field {
        // Field level annotations
        name: String,
        annotations: Vec<WAILAnnotation>,
    },
}

impl Display for WAILAnnotation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WAILAnnotation::Description(desc) => write!(f, "Description: {}", desc),
            WAILAnnotation::Example(ex) => write!(f, "Example: {}", ex),
            WAILAnnotation::Validation(rule) => write!(f, "Validation: {}", rule),
            WAILAnnotation::Format(fmt) => write!(f, "Format: {}", fmt),
            WAILAnnotation::Important(note) => write!(f, "Important: {}", note),
            WAILAnnotation::Context(ctx) => write!(f, "Context: {}", ctx),
            WAILAnnotation::Default(def) => write!(f, "Default: {}", def),
            WAILAnnotation::Field { name, annotations } => {
                write!(f, "Field: {}\n", name)?;
                for annotation in annotations {
                    write!(f, "  {}\n", annotation)?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TemplateArgument {
    String(String),
    Number(i64),
    Float(f64),
    TypeRef(String), // For when we reference a type like "String" or "Number"
    TemplateArgRef(String),
    ObjectRef(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct WAILTemplateCall {
    pub template_name: String,
    pub arguments: HashMap<String, TemplateArgument>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MainStatement {
    Assignment {
        variable: String,
        template_call: WAILTemplateCall,
    },
    ObjectInstantiation {
        variable: String,
        object_type: String,
        arguments: HashMap<String, TemplateArgument>,
    },
    TemplateCall(WAILTemplateCall),
    Comment(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct WAILField<'a> {
    pub name: String,
    pub field_type: WAILType<'a>,
    pub annotations: Vec<WAILAnnotation>,
}

#[derive(Debug, Clone)]
pub struct WAILObjectDef<'a> {
    pub name: String,
    pub fields: Vec<WAILField<'a>>,
}

#[derive(Debug, Clone)]
pub struct WAILObjectInstantiation {
    pub binding_name: String,
    pub object_type: String,
    pub fields: HashMap<String, TemplateArgument>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WAILTemplateDef<'a> {
    pub name: String,
    pub inputs: Vec<WAILField<'a>>,
    pub output: WAILField<'a>,
    pub prompt_template: String,
    pub annotations: Vec<WAILAnnotation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WAILMainDef<'a> {
    pub statements: Vec<MainStatement>,
    pub prompt: String,
    pub template_args: HashMap<String, WAILType<'a>>,
    pub _phantom: PhantomData<&'a ()>,
}

impl TemplateArgument {
    pub fn to_string(&self) -> String {
        match self {
            TemplateArgument::String(s) => s.clone(),
            TemplateArgument::Number(n) => n.to_string(),
            TemplateArgument::Float(f) => f.to_string(),
            TemplateArgument::TypeRef(t) => t.clone(),
            TemplateArgument::TemplateArgRef(t) => format!("${}", t),
            TemplateArgument::ObjectRef(o) => o.clone(),
        }
    }
}

// Helper function to get nested JSON values using dot notation
fn get_nested_value<'a>(json: &'a HashMap<String, JsonValue>, path: &str) -> Option<&'a JsonValue> {
    let parts: Vec<&str> = path
        .split('.')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if parts.is_empty() {
        return None;
    }

    let mut current = json.get(parts[0]);

    // Traverse the remaining path components
    for part in parts.iter().skip(1) {
        current = match current {
            Some(JsonValue::Object(obj)) => obj.get(*part),
            Some(JsonValue::Array(arr)) if part.parse::<usize>().is_ok() => {
                // Support array indexing if the part is a valid index
                arr.get(part.parse::<usize>().unwrap())
            }
            _ => return None,
        };
    }

    current
}

impl<'a> WAILMainDef<'a> {
    pub fn new(
        statements: Vec<MainStatement>,
        prompt: String,
        template_args: Option<HashMap<String, WAILType<'a>>>,
    ) -> Self {
        WAILMainDef {
            statements,
            prompt,
            template_args: template_args.unwrap_or_default(),
            _phantom: PhantomData,
        }
    }

    pub fn interpolate_prompt(
        &self,
        template_registry: &HashMap<String, WAILTemplateDef>,
        object_instances: &HashMap<String, WAILObjectInstantiation>,
        template_arg_values: Option<&HashMap<String, JsonValue>>,
    ) -> Result<String, String> {
        let mut result = self.prompt.clone();

        use crate::template_parser::{parse_template, TemplateSegment};

        // Parse the template into nodes
        let nodes =
            parse_template(&result).map_err(|e| format!("Template parsing error: {}", e))?;

        let mut output = String::new();
        let (_, segments) = nodes;

        for node in segments {
            match node {
                TemplateSegment::Text(text) => output.push_str(&text),
                TemplateSegment::Variable(var_name) => {
                    // Try to find a template call first
                    let replacement = self.statements.iter().find_map(|stmt| match stmt {
                        MainStatement::Assignment {
                            variable,
                            template_call,
                        } if variable == &var_name => {
                            let template = template_registry.get(&template_call.template_name)?;

                            template
                                .interpolate_prompt(
                                    Some(&template_call.arguments),
                                    &object_instances.clone(),
                                )
                                .ok()
                        }
                        MainStatement::ObjectInstantiation {
                            variable,
                            object_type,
                            arguments,
                        } if variable == &var_name => {
                            // For object instantiations, format as a proper JSON object with quoted values
                            let mut obj = HashMap::new();
                            for (key, value) in arguments {
                                let formatted_value = match value {
                                    TemplateArgument::String(s) => format!("\"{}\"", s),
                                    TemplateArgument::Number(n) => n.to_string(),
                                    TemplateArgument::Float(f) => f.to_string(),
                                    TemplateArgument::TypeRef(t) => format!("\"{}\"", t),
                                    TemplateArgument::TemplateArgRef(t) => format!("\"${}\"", t),
                                    TemplateArgument::ObjectRef(o) => format!("\"{}\"", o),
                                };
                                obj.insert(key.clone(), formatted_value);
                            }
                            Some(format!(
                                "{{\"type\": \"{}\", {}}}",
                                object_type,
                                obj.iter()
                                    .map(|(k, v)| format!("\"{}\": {}", k, v))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ))
                        }
                        _ => None,
                    });

                    let value = if let Some(template_result) = replacement {
                        template_result
                    } else if let Some(arg_values) = template_arg_values {
                        if let Some(value) = get_nested_value(arg_values, &var_name) {
                            match value {
                                JsonValue::String(s) => s.clone(),
                                JsonValue::Number(n) => n.to_string(),
                                JsonValue::Object(obj) => {
                                    let mut parts = Vec::new();
                                    for (k, v) in obj {
                                        match v {
                                            JsonValue::String(s) => {
                                                parts.push(format!("{}: {}", k, s))
                                            }
                                            JsonValue::Number(n) => {
                                                parts.push(format!("{}: {}", k, n))
                                            }
                                            _ => parts.push(format!(
                                                "{}: {}",
                                                k,
                                                v.to_string().trim_matches('"')
                                            )),
                                        }
                                    }
                                    parts.join(", ")
                                }
                                JsonValue::Array(arr) => {
                                    let mut parts = Vec::new();
                                    for v in arr {
                                        match v {
                                            JsonValue::String(s) => parts.push(s.clone()),
                                            JsonValue::Number(n) => parts.push(n.to_string()),
                                            _ => parts
                                                .push(v.to_string().trim_matches('"').to_string()),
                                        }
                                    }
                                    parts.join(", ")
                                }
                                JsonValue::Boolean(b) => b.to_string(),
                                JsonValue::Null => "null".to_string(),
                            }
                        } else {
                            return Err(format!("Variable not found: {}", var_name));
                        }
                    } else {
                        return Err(format!("No value found for variable: {}", var_name));
                    };
                    output.push_str(&value);
                }
                TemplateSegment::EachLoop { path, body } => {
                    if let Some(arg_values) = template_arg_values {
                        if let Some(JsonValue::Array(items)) = get_nested_value(arg_values, &path) {
                            for (i, item) in items.iter().enumerate() {
                                let mut item_context = HashMap::new();
                                item_context.insert(".".to_string(), item.clone());

                                // If item is an object, add its fields to the context
                                if let JsonValue::Object(obj) = item {
                                    for (key, value) in obj {
                                        item_context.insert(key.clone(), value.clone());
                                    }
                                }

                                // Parse and process the loop body as a nested template
                                let body_str =
                                    body.iter().map(|s| s.to_string()).collect::<String>();
                                let body_nodes = parse_template(&body_str)
                                    .map_err(|e| format!("Loop body parsing error: {}", e))?;

                                let (_, segments) = body_nodes;
                                for body_node in segments {
                                    match body_node {
                                        TemplateSegment::Text(text) => {
                                            output.push_str(&text);
                                        }
                                        TemplateSegment::Variable(var_name) => {
                                            let value = if var_name == "." {
                                                match item {
                                                    JsonValue::String(s) => s.clone(),
                                                    JsonValue::Number(n) => n.to_string(),
                                                    JsonValue::Object(obj) => {
                                                        let mut parts = Vec::new();
                                                        for (k, v) in obj {
                                                            match v {
                                                                JsonValue::String(s) => parts
                                                                    .push(format!("{}: {}", k, s)),
                                                                JsonValue::Number(n) => parts
                                                                    .push(format!("{}: {}", k, n)),
                                                                _ => parts.push(format!(
                                                                    "{}: {}",
                                                                    k,
                                                                    v.to_string().trim_matches('"')
                                                                )),
                                                            }
                                                        }
                                                        parts.join(", ")
                                                    }
                                                    JsonValue::Array(arr) => {
                                                        let mut parts = Vec::new();
                                                        for v in arr {
                                                            match v {
                                                                JsonValue::String(s) => {
                                                                    parts.push(s.clone())
                                                                }
                                                                JsonValue::Number(n) => {
                                                                    parts.push(n.to_string())
                                                                }
                                                                _ => parts.push(
                                                                    v.to_string()
                                                                        .trim_matches('"')
                                                                        .to_string(),
                                                                ),
                                                            }
                                                        }
                                                        parts.join("\n")
                                                    }
                                                    JsonValue::Boolean(b) => b.to_string(),
                                                    JsonValue::Null => "null".to_string(),
                                                }
                                            } else if let Some(value) =
                                                get_nested_value(&item_context, &var_name)
                                            {
                                                match value {
                                                    JsonValue::String(s) => s.clone(),
                                                    JsonValue::Number(n) => n.to_string(),
                                                    JsonValue::Object(obj) => {
                                                        let mut parts = Vec::new();
                                                        for (k, v) in obj {
                                                            match v {
                                                                JsonValue::String(s) => parts
                                                                    .push(format!("{}: {}", k, s)),
                                                                JsonValue::Number(n) => parts
                                                                    .push(format!("{}: {}", k, n)),
                                                                _ => parts.push(format!(
                                                                    "{}: {}",
                                                                    k,
                                                                    v.to_string().trim_matches('"')
                                                                )),
                                                            }
                                                        }
                                                        parts.join(", ")
                                                    }
                                                    JsonValue::Array(arr) => {
                                                        let mut parts = Vec::new();
                                                        for v in arr {
                                                            match v {
                                                                JsonValue::String(s) => {
                                                                    parts.push(s.clone())
                                                                }
                                                                JsonValue::Number(n) => {
                                                                    parts.push(n.to_string())
                                                                }
                                                                _ => parts.push(
                                                                    v.to_string()
                                                                        .trim_matches('"')
                                                                        .to_string(),
                                                                ),
                                                            }
                                                        }
                                                        parts.join(", ")
                                                    }
                                                    JsonValue::Boolean(b) => b.to_string(),
                                                    JsonValue::Null => "null".to_string(),
                                                }
                                            } else {
                                                return Err(format!(
                                                    "Loop variable not found: {}",
                                                    var_name
                                                ));
                                            };
                                            output.push_str(&value);
                                        }
                                        TemplateSegment::EachLoop { .. } => {
                                            return Err(
                                                "Nested loops are not supported".to_string()
                                            );
                                        }
                                    }
                                }
                                // Add newline between items
                                if i < items.len() - 1 {
                                    output.push_str("\n");
                                }
                            }
                        }
                    }
                }
            }
        }

        result = output;

        if let Some(arg_values) = template_arg_values {
            for (name, value) in arg_values {
                let value_str = match value {
                    JsonValue::String(s) => s.clone(),
                    JsonValue::Number(n) => n.to_string(),
                    JsonValue::Object(obj) => {
                        let mut parts = Vec::new();
                        for (k, v) in obj {
                            let value_str = match v {
                                JsonValue::String(s) => format!("\"{}\"", s),
                                JsonValue::Number(n) => n.to_string(),
                                JsonValue::Boolean(b) => b.to_string(),
                                JsonValue::Null => "null".to_string(),
                                JsonValue::Array(arr) => format!(
                                    "[{}]",
                                    arr.iter()
                                        .map(|v| v.to_string())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                ),
                                JsonValue::Object(inner_obj) => format!(
                                    "{{{}}}",
                                    inner_obj
                                        .iter()
                                        .map(|(k, v)| format!("\"{}\": {}", k, v))
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                ),
                            };
                            parts.push(format!("\"{}\": {}", k, value_str));
                        }
                        format!("{{{}}}", parts.join(", "))
                    }
                    JsonValue::Array(arr) => {
                        let mut parts = Vec::new();
                        for v in arr {
                            match v {
                                JsonValue::String(s) => parts.push(format!("\"{}\"", s)),
                                JsonValue::Number(n) => parts.push(n.to_string()),
                                _ => parts.push(v.to_string()),
                            }
                        }
                        format!("[{}]", parts.join(", "))
                    }
                    JsonValue::Boolean(b) => b.to_string(),
                    JsonValue::Null => "null".to_string(),
                };
                result = result.replace(&format!("${}", name), &value_str);
            }
        }

        Ok(result)
    }

    pub fn validate_llm_response(
        &self,
        json: &JsonValue,
        registry: &HashMap<String, WAILTemplateDef<'a>>,
    ) -> Result<(), (String, Option<String>, JsonValidationError)> {
        // For each template call in statements, validate its output
        for statement in &self.statements {
            match statement {
                MainStatement::Assignment {
                    variable,
                    template_call,
                } => {
                    // Get the template's output type from registry
                    let template = registry.get(&template_call.template_name).ok_or_else(|| {
                        (
                            template_call.template_name.clone(),
                            Some(variable.clone()),
                            JsonValidationError::TemplateNotFound(
                                template_call.template_name.clone(),
                            ),
                        )
                    })?;

                    let template_output = &template.output;

                    // Get the corresponding value from JSON response
                    let value = match json {
                        JsonValue::Object(map) => map.get(variable).ok_or_else(|| {
                            (
                                template_call.template_name.clone(),
                                Some(variable.clone()),
                                JsonValidationError::MissingTemplateResponse(variable.clone()),
                            )
                        })?,
                        _ => {
                            return Err((
                                template_call.template_name.clone(),
                                Some(variable.clone()),
                                JsonValidationError::ExpectedObject(),
                            ))
                        }
                    };

                    // Validate the value against the template's output type
                    template_output
                        .field_type
                        .validate_json(value)
                        .map_err(|e| {
                            (
                                template_call.template_name.clone(),
                                Some(variable.clone()),
                                e,
                            )
                        })?;
                }
                MainStatement::TemplateCall(template_call) => {
                    // Similar validation for direct template calls
                    let template_res = registry.get(&template_call.template_name);

                    let template = match template_res {
                        Some(t) => t,
                        None => {
                            return Err((
                                template_call.template_name.clone(),
                                None::<String>,
                                JsonValidationError::MissingTemplateResponse(
                                    template_call.template_name.clone(),
                                ),
                            ));
                        }
                    };

                    // Get the corresponding value from JSON response
                    let value = match json {
                        JsonValue::Object(map) => {
                            map.get(&template_call.template_name).ok_or_else(|| {
                                (
                                    template_call.template_name.clone(),
                                    None,
                                    JsonValidationError::MissingTemplateResponse(
                                        template_call.template_name.clone(),
                                    ),
                                )
                            })?
                        }
                        _ => {
                            return Err((
                                template_call.template_name.clone(),
                                None,
                                JsonValidationError::ExpectedObject(),
                            ))
                        }
                    };

                    let template_output = &template.output;
                    template_output
                        .field_type
                        .validate_json(value)
                        .map_err(|e| (template_call.template_name.clone(), None, e))?;
                }
                MainStatement::Comment(_) => {}

                MainStatement::ObjectInstantiation {
                    variable,
                    object_type,
                    ..
                } => {}
            }
        }
        Ok(())
    }
}

impl MainStatement {
    pub fn as_template_call(&self) -> Option<&WAILTemplateCall> {
        match self {
            MainStatement::TemplateCall(call) => Some(call),
            _ => None,
        }
    }

    pub fn as_assignment(&self) -> Option<(&String, &WAILTemplateCall)> {
        match self {
            MainStatement::Assignment {
                variable,
                template_call,
            } => Some((variable, &template_call)),
            _ => None,
        }
    }

    pub fn as_object_instantiation(
        &self,
    ) -> Option<(&String, &String, &HashMap<String, TemplateArgument>)> {
        match self {
            MainStatement::ObjectInstantiation {
                variable,
                object_type,
                arguments,
            } => Some((variable, object_type, arguments)),
            _ => None,
        }
    }
}

fn count_leading_whitespace(s: &str) -> usize {
    s.chars().take_while(|c| c.is_whitespace()).count()
}

impl<'a> WAILTemplateDef<'a> {
    pub fn interpolate_prompt(
        &self,
        arguments: Option<&HashMap<String, TemplateArgument>>,
        object_instances: &HashMap<String, WAILObjectInstantiation>,
    ) -> Result<String, String> {
        let mut prompt = self.prompt_template.clone();

        // Handle input parameters

        for input in &self.inputs {
            let placeholder = format!("{{{{{}}}}}", input.name);

            if !prompt.contains(&placeholder) && !object_instances.contains_key(&input.name) {
                return Err(format!("Missing placeholder for input: {}", input.name));
            }

            if let Some(arguments) = arguments {
                let argument = arguments.get(&input.name).unwrap();

                match argument {
                    TemplateArgument::String(s) => {
                        prompt = prompt.replace(&placeholder, s);
                    }
                    TemplateArgument::Number(n) => {
                        prompt = prompt.replace(&placeholder, &n.to_string());
                    }
                    TemplateArgument::Float(f) => {
                        prompt = prompt.replace(&placeholder, &f.to_string());
                    }
                    TemplateArgument::TypeRef(t) => {
                        prompt = prompt.replace(&placeholder, t);
                    }
                    TemplateArgument::TemplateArgRef(t) => {
                        prompt = prompt.replace(&placeholder, &format!("${}", t));
                    }
                    TemplateArgument::ObjectRef(o) => {
                        let object = object_instances
                            .get(o)
                            .ok_or_else(|| format!("Object not found: {}", o))?;

                        // Replace the object reference placeholder with the object's fields
                        for (name, field) in &object.fields {
                            let field_placeholder = format!("{{{{{}.{}}}}}", o, name);

                            match field {
                                TemplateArgument::String(s) => {
                                    prompt = prompt.replace(&field_placeholder, s);
                                }
                                TemplateArgument::Number(n) => {
                                    prompt = prompt.replace(&field_placeholder, &n.to_string());
                                }
                                TemplateArgument::Float(f) => {
                                    prompt = prompt.replace(&field_placeholder, &f.to_string());
                                }
                                TemplateArgument::TypeRef(t) => {
                                    prompt = prompt.replace(&field_placeholder, t);
                                }
                                TemplateArgument::TemplateArgRef(t) => {
                                    prompt = prompt.replace(&field_placeholder, &format!("${}", t));
                                }
                                TemplateArgument::ObjectRef(_) => {
                                    return Err(
                                        "Nested object references are not supported".to_string()
                                    );
                                }
                            }
                        }
                    }
                }
            } else {
                let mut param_info = String::new();

                // Add schema
                param_info.push_str(&input.field_type.to_schema());

                // Group annotations by field
                let mut field_annotations: HashMap<String, Vec<&WAILAnnotation>> = HashMap::new();
                let mut general_annotations = Vec::new();

                for annotation in &input.annotations {
                    match annotation {
                        WAILAnnotation::Field { name, annotations } => {
                            field_annotations
                                .entry(name.clone())
                                .or_default()
                                .extend(annotations.iter());
                        }
                        _ => general_annotations.push(annotation),
                    }
                }

                if !general_annotations.is_empty() {
                    param_info.push_str("\n# General:\n");
                    for annotation in &general_annotations {
                        match annotation {
                            WAILAnnotation::Description(desc) => {
                                param_info.push_str(&format!("# {}\n", desc));
                            }
                            WAILAnnotation::Example(ex) => {
                                param_info.push_str(&format!("# Example: {}\n", ex));
                            }
                            WAILAnnotation::Validation(rule) => {
                                param_info.push_str(&format!("# Validation: {}\n", rule));
                            }
                            WAILAnnotation::Format(fmt) => {
                                param_info.push_str(&format!("# Format: {}\n", fmt));
                            }
                            WAILAnnotation::Important(note) => {
                                param_info.push_str(&format!("# Important: {}\n", note));
                            }
                            WAILAnnotation::Context(ctx) => {
                                param_info.push_str(&format!("# Context: {}\n", ctx));
                            }
                            WAILAnnotation::Default(def) => {
                                param_info.push_str(&format!("# Default: {}\n", def));
                            }
                            WAILAnnotation::Field { .. } => unreachable!(),
                        }
                    }
                }

                // Add field-specific annotations
                if !field_annotations.is_empty() {
                    param_info.push_str("\n# Field Requirements:\n");
                    for (field_name, annotations) in field_annotations {
                        param_info.push_str(&format!("# For {}:\n", field_name));
                        for annotation in annotations {
                            match annotation {
                                WAILAnnotation::Description(desc) => {
                                    param_info.push_str(&format!("#   {}\n", desc));
                                }
                                WAILAnnotation::Example(ex) => {
                                    param_info.push_str(&format!("#   Example: {}\n", ex));
                                }
                                WAILAnnotation::Validation(rule) => {
                                    param_info.push_str(&format!("#   Validation: {}\n", rule));
                                }
                                WAILAnnotation::Format(fmt) => {
                                    param_info.push_str(&format!("#   Format: {}\n", fmt));
                                }
                                WAILAnnotation::Important(note) => {
                                    param_info.push_str(&format!("#   Important: {}\n", note));
                                }
                                WAILAnnotation::Context(ctx) => {
                                    param_info.push_str(&format!("#   Context: {}\n", ctx));
                                }
                                WAILAnnotation::Default(def) => {
                                    param_info.push_str(&format!("#   Default: {}\n", def));
                                }
                                WAILAnnotation::Field { .. } => unreachable!(),
                            }
                        }
                    }
                }

                prompt = prompt.replace(&placeholder, &param_info);
            }
        }

        // Handle return type with proper indentation
        let re = regex::Regex::new(r"\{\{return_type\}\}").unwrap();
        if let Some(cap) = re.find(&prompt) {
            // Get the line containing return_type
            let line_start = prompt[..cap.start()].rfind('\n').map_or(0, |i| i + 1);
            let indent = count_leading_whitespace(&prompt[line_start..cap.start()]);

            let mut return_info = String::new();
            return_info.push_str(&self.output.field_type.to_schema());

            // Group annotations by field for return type
            let mut field_annotations: HashMap<String, Vec<&WAILAnnotation>> = HashMap::new();
            let mut general_annotations = Vec::new();

            for annotation in &self.output.annotations {
                match annotation {
                    WAILAnnotation::Field { name, annotations } => {
                        field_annotations
                            .entry(name.clone())
                            .or_default()
                            .extend(annotations.iter());
                    }
                    _ => general_annotations.push(annotation),
                }
            }

            // Add general annotations for return type
            if !general_annotations.is_empty() {
                return_info.push_str("\n# General:\n");
                for annotation in &general_annotations {
                    match annotation {
                        WAILAnnotation::Description(desc) => {
                            return_info.push_str(&format!("# {}\n", desc));
                        }
                        WAILAnnotation::Example(ex) => {
                            return_info.push_str(&format!("# Example: {}\n", ex));
                        }
                        WAILAnnotation::Validation(rule) => {
                            return_info.push_str(&format!("# Validation: {}\n", rule));
                        }
                        WAILAnnotation::Format(fmt) => {
                            return_info.push_str(&format!("# Format: {}\n", fmt));
                        }
                        WAILAnnotation::Important(note) => {
                            return_info.push_str(&format!("# Important: {}\n", note));
                        }
                        WAILAnnotation::Context(ctx) => {
                            return_info.push_str(&format!("# Context: {}\n", ctx));
                        }
                        WAILAnnotation::Default(def) => {
                            return_info.push_str(&format!("# Default: {}\n", def));
                        }
                        WAILAnnotation::Field { .. } => unreachable!(),
                    }
                }
            }

            // Add field-specific annotations for return type
            if !field_annotations.is_empty() {
                return_info.push_str("\n# Field Requirements:\n");
                for (field_name, annotations) in field_annotations {
                    return_info.push_str(&format!("# For {}:\n", field_name));
                    for annotation in annotations {
                        match annotation {
                            WAILAnnotation::Description(desc) => {
                                return_info.push_str(&format!("#   {}\n", desc));
                            }
                            WAILAnnotation::Example(ex) => {
                                return_info.push_str(&format!("#   Example: {}\n", ex));
                            }
                            WAILAnnotation::Validation(rule) => {
                                return_info.push_str(&format!("#   Validation: {}\n", rule));
                            }
                            WAILAnnotation::Format(fmt) => {
                                return_info.push_str(&format!("#   Format: {}\n", fmt));
                            }
                            WAILAnnotation::Important(note) => {
                                return_info.push_str(&format!("#   Important: {}\n", note));
                            }
                            WAILAnnotation::Context(ctx) => {
                                return_info.push_str(&format!("#   Context: {}\n", ctx));
                            }
                            WAILAnnotation::Default(def) => {
                                return_info.push_str(&format!("#   Default: {}\n", def));
                            }
                            WAILAnnotation::Field { .. } => unreachable!(),
                        }
                    }
                }
            }

            // Apply indentation to all lines including annotations
            let indented_schema = return_info
                .lines()
                .enumerate()
                .map(|(i, line)| {
                    if i == 0 {
                        line.to_string()
                    } else {
                        format!("{}{}", " ".repeat(indent), line)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            let return_prompt = format!(
                "\nAnswer using this schema:\n{}\nWrap your final result in <result> and </result> tags.",
                indented_schema
            );

            prompt = re.replace(&prompt, &return_prompt).to_string();
        }

        Ok(prompt)
    }
}

#[cfg(test)]
mod tests {
    use crate::wail_parser::{WAILFileType, WAILParser};

    #[test]
    fn test_parse_llm_output() {
        let wail_schema = r#"
    object Person {
        name: String
        age: Number
        interests: String[]
    }

    template GetPerson(description: String) -> Person {
        prompt: """{{description}}"""
    }

    main {
        let person = GetPerson(description: "test");
        prompt { 
            {{person}} 
        }
    }"#;

        let test_dir = std::env::current_dir().unwrap();
        let parser = WAILParser::new(test_dir);
        parser
            .parse_wail_file(wail_schema.to_string(), WAILFileType::Application, true)
            .unwrap();

        // Test relaxed JSON parsing features
        let cases = vec![
            // Unquoted keys
            r#"<result>{"person": {name: "Alice", age: 25, interests: ["coding"]}}</result>"#,
            // Single quotes
            r#"<result>{'person': {'name': 'Alice', 'age': 25, 'interests': ['coding']}}</result>"#,
            // Trailing commas
            r#"<result>{"person": {"name": "Alice", "age": 25, "interests": ["coding",],}}</result>"#,
            // Mixed quotes and unquoted identifiers
            r#"<result>{"person": {name: 'Alice', "age": 25, interests: ["coding"]}}</result>"#,
        ];

        for case in cases {
            assert!(
                parser.parse_llm_output(case).is_ok(),
                "Failed to parse: {}",
                case
            );
        }
    }

    use super::*;
    use crate::json_types::JsonValue;
    use std::collections::HashMap;

    // Helper function to create a test JSON object
    fn create_test_json() -> HashMap<String, JsonValue> {
        let mut json = HashMap::new();
        let mut user = HashMap::new();
        user.insert("name".to_string(), JsonValue::String("John".to_string()));
        user.insert("age".to_string(), JsonValue::Number(Number::Integer(30)));

        let mut address = HashMap::new();
        address.insert(
            "street".to_string(),
            JsonValue::String("123 Main St".to_string()),
        );
        address.insert(
            "city".to_string(),
            JsonValue::String("Springfield".to_string()),
        );
        user.insert("address".to_string(), JsonValue::Object(address));

        let hobbies = vec![
            JsonValue::String("reading".to_string()),
            JsonValue::String("gaming".to_string()),
        ];
        user.insert("hobbies".to_string(), JsonValue::Array(hobbies));

        json.insert("user".to_string(), JsonValue::Object(user));
        json
    }

    #[test]
    fn test_get_nested_value_basic() {
        let json = create_test_json();

        // Test basic property access
        let name = get_nested_value(&json, "user.name");
        assert_eq!(
            name.and_then(|v| v.as_string()),
            Some("John".to_string()).as_ref()
        );

        // Test nested object access
        let city = get_nested_value(&json, "user.address.city");
        assert_eq!(
            city.and_then(|v| v.as_string()),
            Some("Springfield".to_string()).as_ref()
        );

        // Test array access
        let hobby = get_nested_value(&json, "user.hobbies.0");
        assert_eq!(
            hobby.and_then(|v| v.as_string()),
            Some("reading".to_string()).as_ref()
        );
    }

    #[test]
    fn test_get_nested_value_error_cases() {
        let json = create_test_json();

        // Test invalid path
        assert_eq!(
            get_nested_value(&json, "invalid.path").and_then(|v| v.as_string()),
            None
        );

        // Test empty path
        assert_eq!(
            get_nested_value(&json, "").and_then(|v| v.as_string()),
            None
        );

        // Test invalid array index
        assert_eq!(
            get_nested_value(&json, "user.hobbies.99").and_then(|v| v.as_string()),
            None
        );

        // Test path to primitive as if it were an object
        assert_eq!(
            get_nested_value(&json, "user.name.invalid").and_then(|v| v.as_string()),
            None
        );
    }

    #[test]
    fn test_each_loop_basic() {
        let main_def = WAILMainDef::new(
            vec![],
            "{{#each user.hobbies}}Hobby: {{.}}{{/each}}".to_string(),
            None,
        );

        let template_registry = HashMap::new();
        let registry = HashMap::new();
        let arg_values = create_test_json();

        let result = main_def.interpolate_prompt(&template_registry, &registry, Some(&arg_values));
        assert_eq!(result.unwrap(), "Hobby: reading\nHobby: gaming");
    }

    #[test]
    fn test_each_loop_nested_properties() {
        // Create test data with nested objects in array
        let mut json = HashMap::new();
        let mut pets = Vec::new();

        let mut pet1 = HashMap::new();
        pet1.insert("name".to_string(), JsonValue::String("Fluffy".to_string()));
        pet1.insert("type".to_string(), JsonValue::String("cat".to_string()));
        pets.push(JsonValue::Object(pet1));

        let mut pet2 = HashMap::new();
        pet2.insert("name".to_string(), JsonValue::String("Rover".to_string()));
        pet2.insert("type".to_string(), JsonValue::String("dog".to_string()));
        pets.push(JsonValue::Object(pet2));

        json.insert("pets".to_string(), JsonValue::Array(pets));

        let main_def = WAILMainDef::new(
            vec![],
            "{{#each pets}}Pet: {{name}} is a {{type}}{{/each}}".to_string(),
            None,
        );

        let template_registry = HashMap::new();
        let registry = HashMap::new();
        let result = main_def.interpolate_prompt(&template_registry, &registry, Some(&json));
        assert_eq!(result.unwrap(), "Pet: Fluffy is a cat\nPet: Rover is a dog");
    }

    #[test]
    fn test_complex_template() {
        let json = create_test_json();

        let template = "User {{user.name}} ({{user.age}}) lives in {{user.address.city}}.\nHobbies:\n{{#each user.hobbies}}* {{.}}\n{{/each}}".to_string();

        let main_def = WAILMainDef::new(vec![], template, None);
        let template_registry = HashMap::new();
        let registry = HashMap::new();

        let result = main_def.interpolate_prompt(&template_registry, &registry, Some(&json));
        let expected = "User John (30) lives in Springfield.\nHobbies:\n* reading\n\n* gaming\n";

        assert_eq!(result.unwrap(), expected);
    }
}
