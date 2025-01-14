use crate::json_types::JsonValue;
use crate::types::*;
use regex;
use std::collections::HashMap;
use std::marker::PhantomData;

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct WAILUnionDef<'a> {
    pub name: String,
    pub members: Vec<WAILType<'a>>,
}

#[derive(Debug, Clone)]
pub enum TemplateArgument {
    String(String),
    Number(i64),
    Float(f64),
    TypeRef(String), // For when we reference a type like "String" or "Number"
    TemplateArgRef(String),
}

#[derive(Debug, Clone)]
pub struct WAILTemplateCall {
    pub template_name: String,
    pub arguments: HashMap<String, TemplateArgument>,
}

#[derive(Debug, Clone)]
pub enum MainStatement {
    Assignment {
        variable: String,
        template_call: WAILTemplateCall,
    },
    TemplateCall(WAILTemplateCall),
    Comment(String),
}

#[derive(Debug, Clone)]
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
pub struct WAILTemplateDef<'a> {
    pub name: String,
    pub inputs: Vec<WAILField<'a>>,
    pub output: WAILField<'a>,
    pub prompt_template: String,
    pub annotations: Vec<WAILAnnotation>,
}

#[derive(Debug, Clone)]
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
        }
    }
}

// Helper function to get nested JSON values using dot notation
fn get_nested_value<'a>(json: &'a HashMap<String, JsonValue>, path: &str) -> Option<&'a JsonValue> {
    let parts: Vec<&str> = path.split('.')
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
            },
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
        template_arg_values: Option<&HashMap<String, JsonValue>>,
    ) -> Result<String, String> {
        let mut result = self.prompt.clone();
        
        // Handle {{#each}} loops first
        let loop_re = regex::Regex::new(r"\{\{#each\s+([^}]+)\}\}(.*?)\{\{/each\}\}").unwrap();
        let mut replacements = Vec::new();
        
        // Collect all loop replacements first
        for cap in loop_re.captures_iter(&result) {
            let full_match = cap[0].to_string();
            let path = cap[1].trim();
            let template = cap[2].to_string();
            
            // Find the array to iterate over
            let mut loop_result = String::new();
            if let Some(arg_values) = template_arg_values {
                if let Some(array) = get_nested_value(arg_values, path) {
                    if let JsonValue::Array(items) = array {
                        for item in items {
                            let mut item_result = template.clone();
                            // Replace variables within the loop template
                            let var_re = regex::Regex::new(r"\{\{([^}]+)\}\}").unwrap();
                            for var_cap in var_re.captures_iter(&template) {
                                let var_match = var_cap[0].to_string();
                                let var_name = var_cap[1].trim();
                                // Handle nested property access within loop variables
                                let var_parts: Vec<&str> = var_name.split('.').collect();
                                let mut current_value = Some(item);
                                
                                // Traverse the property path
                                for part in var_parts {
                                    current_value = match current_value {
                                        Some(JsonValue::Object(obj)) => obj.get(part),
                                        _ => None,
                                    };
                                }
                                
                                if let Some(value) = current_value {
                                    let value_str = value.to_string();
                                    item_result = item_result.replace(&var_match, &value_str);
                                }
                            }
                            loop_result.push_str(&item_result);
                        }
                    }
                }
            }
            replacements.push((full_match, loop_result));
        }
        
        // Apply all replacements
        for (pattern, replacement) in replacements {
            result = result.replace(&pattern, &replacement);
        }

        // Handle regular variable interpolation
        let var_re = regex::Regex::new(r"\{\{([^}]+)\}\}").unwrap();
        let mut var_replacements = Vec::new();
        
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::json_types::JsonValue;

    // Helper function to create a test JSON object
    fn create_test_json() -> HashMap<String, JsonValue> {
        let mut json = HashMap::new();
        let mut user = HashMap::new();
        user.insert("name".to_string(), JsonValue::String("John".to_string()));
        user.insert("age".to_string(), JsonValue::Number(30));
        
        let mut address = HashMap::new();
        address.insert("street".to_string(), JsonValue::String("123 Main St".to_string()));
        address.insert("city".to_string(), JsonValue::String("Springfield".to_string()));
        user.insert("address".to_string(), JsonValue::Object(address));
        
        let hobbies = vec![
            JsonValue::String("reading".to_string()),
            JsonValue::String("gaming".to_string())
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
        assert_eq!(name, Some(&JsonValue::String("John".to_string())));
        
        // Test nested object access
        let city = get_nested_value(&json, "user.address.city");
        assert_eq!(city, Some(&JsonValue::String("Springfield".to_string())));
        
        // Test array access
        let hobby = get_nested_value(&json, "user.hobbies.0");
        assert_eq!(hobby, Some(&JsonValue::String("reading".to_string())));
    }

    #[test]
    fn test_get_nested_value_error_cases() {
        let json = create_test_json();
        
        // Test invalid path
        assert_eq!(get_nested_value(&json, "invalid.path"), None);
        
        // Test empty path
        assert_eq!(get_nested_value(&json, ""), None);
        
        // Test invalid array index
        assert_eq!(get_nested_value(&json, "user.hobbies.99"), None);
        
        // Test path to primitive as if it were an object
        assert_eq!(get_nested_value(&json, "user.name.invalid"), None);
    }

    #[test]
    fn test_each_loop_basic() {
        let mut main_def = WAILMainDef::new(
            vec![],
            "{{#each user.hobbies}}Hobby: {{.}}{{/each}}".to_string(),
            None,
        );
        
        let template_registry = HashMap::new();
        let arg_values = create_test_json();
        
        let result = main_def.interpolate_prompt(&template_registry, Some(&arg_values));
        assert_eq!(result.unwrap(), "Hobby: readingHobby: gaming");
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
        
        let mut main_def = WAILMainDef::new(
            vec![],
            "{{#each pets}}Pet: {{name}} is a {{type}}{{/each}}".to_string(),
            None,
        );
        
        let template_registry = HashMap::new();
        let result = main_def.interpolate_prompt(&template_registry, Some(&json));
        assert_eq!(result.unwrap(), "Pet: Fluffy is a catPet: Rover is a dog");
    }

    #[test]
    fn test_complex_template() {
        let mut json = create_test_json();
        
        let template = "User {{user.name}} ({{user.age}}) lives in {{user.address.city}}.\n\
                    Hobbies:\n\
                    {{#each user.hobbies}}* {{.}}\n{{/each}}".to_string();
        
        let mut main_def = WAILMainDef::new(vec![], template, None);
        let template_registry = HashMap::new();
        
        let result = main_def.interpolate_prompt(&template_registry, Some(&json));
        let expected = "User John (30) lives in Springfield.\n\
                    Hobbies:\n\
                    * reading\n\
                    * gaming\n";
        
        assert_eq!(result.unwrap(), expected);
    }
}
        
        // Collect all variable replacements first
        for cap in var_re.captures_iter(&result) {
            let full_match = cap[0].to_string();
            let var_name = &cap[1];

            // Find the template call for this variable
            let template_call = self
                .statements
                .iter()
                .find_map(|stmt| match stmt {
                    MainStatement::Assignment {
                        variable,
                        template_call,
                    } if variable == var_name => Some(template_call),
                    _ => None,
                })
                .ok_or_else(|| format!("No template call found for variable: {}", var_name))?;

            // Look up the template
            let template = template_registry
                .get(&template_call.template_name)
                .ok_or_else(|| format!("Template not found: {}", template_call.template_name))?;

            // Get the interpolated template prompt
            let replacement = template
                .interpolate_prompt(Some(&template_call.arguments))
                .unwrap();
                
            var_replacements.push((full_match, replacement));
        }
        
        // Apply all variable replacements
        for (pattern, replacement) in var_replacements {
            result = result.replace(&pattern, &replacement);
        }

        if let Some(arg_values) = template_arg_values {
            for (name, value) in arg_values {
                let value_str = if let Some(s) = value.as_string() {
                    s.to_string()
                } else {
                    value.to_string()
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
    ) -> Result<(), String> {
        // For each template call in statements, validate its output
        for statement in &self.statements {
            match statement {
                MainStatement::Assignment {
                    variable,
                    template_call,
                } => {
                    // Get the template's output type from registry
                    let template = registry.get(&template_call.template_name).ok_or_else(|| {
                        format!("Template not found: {}", template_call.template_name)
                    })?;

                    let template_output = &template.output;

                    // Get the corresponding value from JSON response
                    let value = match json {
                        JsonValue::Object(map) => map.get(variable).ok_or_else(|| {
                            format!("Missing output for template call: {}", variable)
                        })?,
                        _ => return Err("Expected object response from LLM".to_string()),
                    };

                    // Validate the value against the template's output type
                    template_output.field_type.validate_json(value)?;
                }
                MainStatement::TemplateCall(template_call) => {
                    // Similar validation for direct template calls
                    let template = registry.get(&template_call.template_name).ok_or_else(|| {
                        format!("Template not found: {}", template_call.template_name)
                    })?;

                    // Get the corresponding value from JSON response
                    let value = match json {
                        JsonValue::Object(map) => {
                            map.get(&template_call.template_name).ok_or_else(|| {
                                format!(
                                    "Missing output for template call: {}",
                                    template_call.template_name
                                )
                            })?
                        }
                        _ => return Err("Expected object response from LLM".to_string()),
                    };

                    let template_output = &template.output;
                    println!("Validating: {:?}", template_output.field_type);
                    println!("Value: {:?}", value);
                    template_output.field_type.validate_json(value)?;
                }
                MainStatement::Comment(_) => {}
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
            } => Some((variable, template_call)),
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
    ) -> Result<String, String> {
        let mut prompt = self.prompt_template.clone();

        // Handle input parameters
        for input in &self.inputs {
            let placeholder = format!("{{{{{}}}}}", input.name);
            if !prompt.contains(&placeholder) {
                return Err(format!("Missing placeholder for input: {}", input.name));
            }

            if let Some(arguments) = arguments {
                let argument = arguments.get(&input.name).unwrap();
                prompt = prompt.replace(&placeholder, &argument.to_string());
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

                // Add general annotations
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
                "\nReturn a JSON-like format wrapped in ```gasp fences:\n\n{}",
                indented_schema
            );
            prompt = re.replace(&prompt, &return_prompt).to_string();
        }

        Ok(prompt)
    }
}

#[cfg(test)]
mod tests {
    use crate::wail_parser::WAILParser;

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
        prompt { {{person}} }
    }"#;

        let parser = WAILParser::new();
        parser.parse_wail_file(wail_schema).unwrap();

        // Test relaxed JSON parsing features
        let cases = vec![
            // Unquoted keys
            r#"{"person": {name: "Alice", age: 25, interests: ["coding"]}}"#,
            // Single quotes
            r#"{'person': {'name': 'Alice', 'age': 25, 'interests': ['coding']}}"#,
            // Trailing commas
            r#"{"person": {"name": "Alice", "age": 25, "interests": ["coding",],}}"#,
            // Mixed quotes and unquoted identifiers
            r#"{"person": {name: 'Alice', "age": 25, interests: ["coding"]}}"#,
        ];

        for case in cases {
            assert!(
                parser.parse_llm_output(case).is_ok(),
                "Failed to parse: {}",
                case
            );
        }
    }
}