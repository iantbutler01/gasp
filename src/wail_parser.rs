use crate::json_types::{JsonValue, Number};
use crate::parser_types::*;
use crate::rd_json_stack_parser::Parser as JsonParser;
use crate::types::*;
use nom::error::ParseError;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while1},
    character::complete::{char, multispace0, multispace1},
    combinator::opt,
    multi::{many0, many1, separated_list0},
    sequence::{delimited, preceded, tuple},
    IResult,
};

use nom_supreme::final_parser::Location;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::env::var;
use std::path::PathBuf;
use std::sync::Arc;

use nom_supreme::error::{BaseErrorKind, ErrorTree};

fn error_location(e: nom::Err<ErrorTree<&str>>, original_input: &str) -> Location {
    let location = match e {
        nom::Err::Error(ErrorTree::Base { location, .. }) => location,
        nom::Err::Failure(ErrorTree::Base { location, .. }) => location,
        _ => "unknown",
    };

    if location == "unknown" {
        return Location { line: 0, column: 0 };
    }

    Location::locate_tail(original_input, location)
}

#[derive(Debug, Clone)]
pub enum WAILParseError {
    // Syntax errors
    UnexpectedToken {
        found: String,
        location: Location,
    },
    UnexpectedEOF {
        expected: String,
        location: Location,
    },
    InvalidIdentifier {
        found: String,
        location: Location,
    },

    // Static analysis errors
    UndefinedType {
        name: String,
        location: Location,
    },
    DuplicateDefinition {
        name: String,
        location: Location,
    },
    MissingMainBlock,
    InvalidTemplateCall {
        template_name: String,
        reason: String,
        location: Location,
    },
    CircularImport {
        path: String,
        chain: Vec<String>,
    },
    InvalidImportPath {
        path: String,
        error: String,
    },
    FileError {
        path: String,
        error: String,
    },
    ImportNotFound {
        name: String,
        path: String,
    },
}

impl std::fmt::Display for WAILParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WAILParseError::CircularImport { path, chain } => {
                write!(f, "Circular import detected: {} in chain {:?}", path, chain)
            }
            // ... handle other variants ...
            _ => write!(f, "{:?}", self),
        }
    }
}

impl std::error::Error for WAILParseError {}

fn count_leading_whitespace(line: &str) -> usize {
    line.chars().take_while(|c| c.is_whitespace()).count()
}

fn adjust_indentation(content: &str, target_indent: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return String::new();
    }

    // Find initial whitespace amount from first non-empty line
    let initial_indent = lines
        .iter()
        .find(|line| !line.trim().is_empty())
        .map(|line| count_leading_whitespace(line))
        .unwrap_or(0);

    // Calculate how much to adjust by
    let indent_adjustment = initial_indent.saturating_sub(target_indent);

    // Adjust each line
    lines
        .iter()
        .map(|line| {
            let current_indent = count_leading_whitespace(line);
            if current_indent >= indent_adjustment {
                &line[indent_adjustment..]
            } else {
                line.trim_start()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug)]
pub struct WAILParser<'a> {
    registry: RefCell<HashMap<String, WAILField<'a>>>,
    template_registry: RefCell<HashMap<String, WAILTemplateDef<'a>>>,
    adhoc_obj_ref_id_counter: RefCell<i64>,
    adhoc_obj_ids: RefCell<Vec<String>>,
    adhoc_obj_refs: RefCell<HashMap<String, WAILObject<'a>>>,
    main: RefCell<Option<WAILMainDef<'a>>>,
    // Track object instantiations with their variable names
    object_instances: RefCell<HashMap<String, WAILObjectInstantiation>>,
    import_chain: RefCell<ImportChain>,
    base_path: PathBuf,
}

#[derive(Debug)]
struct ImportChain {
    // Track the chain of imports to detect cycles
    chain: Vec<String>,
    // Current working directory for resolving relative paths
    base_path: PathBuf,
}

impl ImportChain {
    fn new(base_path: PathBuf) -> Self {
        Self {
            chain: Vec::new(),
            base_path,
        }
    }

    fn push(&mut self, path: &str) -> Result<(), WAILParseError> {
        let canonical_path = self.resolve_path(path)?;

        if self.chain.contains(&canonical_path) {
            return Err(WAILParseError::CircularImport {
                path: canonical_path,
                chain: self.chain.clone(),
            });
        }

        self.chain.push(canonical_path);
        Ok(())
    }

    fn pop(&mut self) {
        self.chain.pop();
    }

    fn resolve_path(&self, path: &str) -> Result<String, WAILParseError> {
        let path = PathBuf::from(path);

        let resolved = if path.is_absolute() {
            path
        } else {
            self.base_path.join(path)
        };

        // Clean up the path without checking file existence
        let cleaned = resolved
            .components()
            .fold(PathBuf::new(), |mut cleaned, component| {
                match component {
                    std::path::Component::ParentDir => {
                        cleaned.pop(); // Remove last component for ..
                    }
                    std::path::Component::Normal(part) => {
                        cleaned.push(part);
                    }
                    std::path::Component::RootDir => {
                        cleaned.push("/");
                    }
                    _ => {} // Skip . and prefix components
                }
                cleaned
            });

        Ok(cleaned.to_string_lossy().to_string())
    }
}

#[derive(Debug)]
pub enum WAILFileType {
    Library,     // .lib.wail - only definitions allowed
    Application, // .wail - requires main block
}

#[derive(Debug, Clone, PartialEq)]
pub struct WAILImport {
    pub items: Vec<String>,
    pub path: String,
}

impl<'a> WAILParser<'a> {
    fn detect_file_type(path: &'a str) -> WAILFileType {
        if path.ends_with(".lib.wail") {
            WAILFileType::Library
        } else {
            WAILFileType::Application
        }
    }

    fn collect_referenced_objects(
        &self,
        field_type: &WAILType<'a>,
        objects: &HashMap<String, WAILDefinition<'a>>,
        collected_defs: &mut Vec<WAILDefinition<'a>>,
    ) {
        match field_type {
            WAILType::Composite(composite) => match composite {
                WAILCompositeType::Object(obj) => {
                    // Check if this object type exists in our objects map
                    if let Some(def) = objects.get(obj.type_data.type_name) {
                        if !collected_defs.contains(def) {
                            collected_defs.push(def.clone());

                            // Recursively check fields of this object
                            if let Some(fields) = &obj.type_data.field_definitions {
                                for field in fields {
                                    self.collect_referenced_objects(
                                        &field.field_type,
                                        objects,
                                        collected_defs,
                                    );
                                }
                            }
                        }
                    }
                }
                WAILCompositeType::Array(array) => {
                    // Check element type if it exists
                    if let Some(element_type) = &array.type_data.element_type {
                        self.collect_referenced_objects(element_type, objects, collected_defs);
                    }
                }
                WAILCompositeType::Union(union) => {
                    // Check all union members
                    for member in &union.members {
                        self.collect_referenced_objects(
                            &member.field_type,
                            objects,
                            collected_defs,
                        );
                    }
                }
            },
            _ => (), // Simple types don't reference other objects
        }
    }

    fn resolve_imports(
        &'a self,
        definitions: &Vec<WAILDefinition<'a>>,
    ) -> Result<(), WAILParseError> {
        let mut all_imported_defs = vec![];

        for def in definitions {
            if let WAILDefinition::Import(import) = def {
                // Clean up import pollution since we use the same parser
                self.object_instances.borrow_mut().clear();
                self.registry.borrow_mut().clear();
                self.template_registry.borrow_mut().clear();
                // Load and parse the library file
                let file_path = self.import_chain.borrow().resolve_path(&import.path)?;

                let lib_content =
                    std::fs::read_to_string(&file_path).map_err(|e| WAILParseError::FileError {
                        path: import.path.clone(),
                        error: e.to_string(),
                    })?;

                let file_type = WAILFileType::Library;

                // Parse the library file
                let lib_defs = self.parse_wail_file(lib_content, file_type, false)?;
                let mut objects: HashMap<String, WAILDefinition> = HashMap::new();

                for lib_def in &lib_defs {
                    match lib_def {
                        WAILDefinition::Object(field) => {
                            objects.insert(field.name.clone(), lib_def.clone());
                        }
                        _ => continue,
                    }
                }

                // Extract requested definitions
                for item_name in &import.items {
                    let mut found = false;
                    for lib_def in &lib_defs {
                        match lib_def {
                            WAILDefinition::Object(field) if &field.name == item_name => {
                                found = true;
                                all_imported_defs.push(lib_def.clone());

                                // Collect referenced objects
                                self.collect_referenced_objects(
                                    &field.field_type,
                                    &objects,
                                    &mut all_imported_defs,
                                );
                                break;
                            }
                            WAILDefinition::Template(template) if &template.name == item_name => {
                                found = true;
                                all_imported_defs.push(lib_def.clone());

                                // Check input parameters
                                for param in &template.inputs {
                                    self.collect_referenced_objects(
                                        &param.field_type,
                                        &objects,
                                        &mut all_imported_defs,
                                    );
                                }

                                // Check return type
                                self.collect_referenced_objects(
                                    &template.output.field_type,
                                    &objects,
                                    &mut all_imported_defs,
                                );
                                break;
                            }
                            WAILDefinition::Union(field) if &field.name == item_name => {
                                found = true;
                                all_imported_defs.push(lib_def.clone());

                                // Process union members and their referenced types
                                self.collect_referenced_objects(
                                    &field.field_type,
                                    &objects,
                                    &mut all_imported_defs,
                                );
                                break;
                            }
                            _ => continue,
                        }
                    }
                    if !found {
                        return Err(WAILParseError::ImportNotFound {
                            name: item_name.clone(),
                            path: import.path.clone(),
                        });
                    }
                }
            }
        }
        // One final clean up of import pollution since we use the same parser
        self.object_instances.borrow_mut().clear();
        self.registry.borrow_mut().clear();
        self.template_registry.borrow_mut().clear();

        // Finally insert defs that were selected
        // This has to happen in two parts so we accumulate all defs first otherwise clearing would clobber.
        for def in all_imported_defs {
            match def {
                WAILDefinition::Object(field) => {
                    self.registry
                        .borrow_mut()
                        .insert(field.name.clone(), field.clone());
                }
                WAILDefinition::Template(template) => {
                    self.template_registry
                        .borrow_mut()
                        .insert(template.name.clone(), template.clone());
                }
                WAILDefinition::Union(field) => {
                    self.registry
                        .borrow_mut()
                        .insert(field.name.clone(), field.clone());
                }
                _ => continue,
            }
        }
        Ok(())
    }

    fn parse_import(
        &'a self,
        input: &'a str,
    ) -> IResult<&'a str, WAILDefinition<'a>, ErrorTree<&'a str>> {
        let (input, _) = tuple((tag("import"), multispace1))(input)?;

        let (input, items) = delimited(
            tuple((char('{'), multispace0)),
            separated_list0(tuple((multispace0, char(','), multispace0)), |f| {
                self.identifier(f)
            }),
            tuple((multispace0, char('}'))),
        )(input)?;

        let (input, _) = tuple((multispace0, tag("from"), multispace0))(input)?;
        let (input, path) = delimited(char('"'), take_until("\""), char('"'))(input)?;

        if let Err(e) = self.import_chain.borrow_mut().push(path) {
            match e {
                WAILParseError::CircularImport { path, chain } => {
                    // Propagate circular import error directly
                    return Err(nom::Err::Failure(ErrorTree::Stack {
                        base: Box::new(ErrorTree::Base {
                            location: input,
                            kind: BaseErrorKind::External(Box::new(
                                WAILParseError::CircularImport { path, chain },
                            )),
                        }),
                        contexts: vec![],
                    }));
                }
                _ => {
                    return Err(nom::Err::Failure(ErrorTree::from_error_kind(
                        input,
                        nom::error::ErrorKind::Verify,
                    )));
                }
            }
        }

        Ok((
            input,
            WAILDefinition::Import(WAILImport {
                items: items.iter().map(|s| s.to_string()).collect(),
                path: path.to_string(),
            }),
        ))
    }

    pub fn new(base_path: PathBuf) -> Self {
        Self {
            registry: RefCell::new(HashMap::new()),
            template_registry: RefCell::new(HashMap::new()),
            adhoc_obj_ref_id_counter: RefCell::new(0),
            adhoc_obj_refs: RefCell::new(HashMap::new()),
            adhoc_obj_ids: RefCell::new(Vec::new()),
            main: RefCell::new(None),
            object_instances: RefCell::new(HashMap::new()),
            import_chain: RefCell::new(ImportChain::new(base_path.clone())),
            base_path: base_path.clone(),
        }
    }

    fn parse_json_like_segment(&'a self, input: &'a str) -> IResult<&'a str, String> {
        let (input, _) = multispace0(input)?;

        if input.is_empty() {
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Eof,
            )));
        }

        // Find positions of `gasp` fence and JSON object in the input.
        let result_pos = input.find("<result>");

        match result_pos {
            Some(_) => self.parse_result_block(input), // Only <result> block is present.
            None => Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Tag,
            ))), // Neither pattern is found.
        }
    }

    /// Parse a <result></result> fenced block.
    fn parse_result_block(&'a self, input: &'a str) -> IResult<&'a str, String> {
        let (input, _) = take_until("<result>")(input)?;
        let (input, content) =
            delimited(tag("<result>"), take_until("</result>"), tag("</result>"))(input)?;

        let content = content.trim();
        Ok((input, content.to_string()))
    }

    pub fn parse_llm_output(&'a self, input: &'a str) -> Result<JsonValue, String> {
        // First get the ordered list of variable names from main statements
        let var_names: Vec<String> = self
            .main
            .borrow()
            .as_ref()
            .unwrap()
            .statements
            .iter()
            .filter_map(|stmt| match stmt {
                MainStatement::Assignment { variable, .. } => Some(variable.clone()),
                _ => None,
            })
            .collect();

        // Parse the JSON-like segments
        let (_input, segments) = many0(|input| self.parse_json_like_segment(input))(input)
            .map_err(|e| format!("Failed to parse segments: {:?}", e))?;

        if segments.len() != var_names.len() {
            return Err(format!(
                "Found {} JSON segments but expected {} based on template variables",
                segments.len(),
                var_names.len()
            ));
        }

        // Try to parse each segment and build the map
        let mut result = HashMap::new();
        for (var_name, segment) in var_names.into_iter().zip(segments) {
            let mut parser = JsonParser::new(segment.as_bytes().to_vec());
            let json_value = parser
                .parse()
                .map_err(|e| format!("Failed to parse JSON for {}: {}", var_name, e))?;
            result.insert(var_name, json_value);
        }

        Ok(JsonValue::Object(result))
    }

    fn instantiate_object(
        &'a self,
        name: &str,
        object_type: &str,
        args: HashMap<String, TemplateArgument>,
    ) -> Result<WAILObjectInstantiation, String> {
        // Get the object definition from registry
        let registry = self.registry.borrow();
        let field = registry
            .get(object_type)
            .ok_or_else(|| format!("Type not found: {}", object_type))?;

        if let WAILType::Composite(WAILCompositeType::Object(obj)) = &field.field_type {
            let mut field_map = HashMap::new();

            // Get field definitions
            if let Some(field_defs) = &obj.type_data.field_definitions {
                for field in field_defs {
                    if let Some(arg) = args.get(&field.name) {
                        field_map.insert(
                            WAILString {
                                value: field.name.clone(),
                                type_data: WAILTypeData {
                                    json_type: JsonValue::String(field.name.clone()),
                                    type_name: "String",
                                    field_definitions: None,
                                    element_type: None,
                                },
                            },
                            field.field_type.clone(),
                        );
                    }
                }
            }

            Ok(WAILObjectInstantiation {
                binding_name: name.to_string(),
                object_type: object_type.to_string(),
                fields: args,
            })
        } else {
            Err(format!("{} is not an object type", object_type))
        }
    }

    pub fn prepare_prompt(
        &'a self,
        template_arg_values: Option<&HashMap<String, JsonValue>>,
    ) -> String {
        let main = self.main.borrow();
        let main = main.as_ref().unwrap();

        // Now proceed with prompt interpolation
        main.interpolate_prompt(
            &self.template_registry.borrow(),
            &self.object_instances.borrow(),
            template_arg_values,
        )
        .unwrap()
    }

    pub fn parse_wail_file(
        &'a self,
        input_string: String,
        file_type: WAILFileType,
        clear: bool,
    ) -> Result<Vec<WAILDefinition<'a>>, WAILParseError> {
        let input: &str = Box::leak(Box::new(input_string)); // Yes I hate that I leak this, no I'm not redesigning things for the lifetimes to line up

        if clear {
            self.registry.borrow_mut().clear();
            self.template_registry.borrow_mut().clear();
            self.main.borrow_mut().take();
        }

        let original_input = input;

        let (input, _) = multispace0(input).map_err(|e: nom::Err<ErrorTree<&'a str>>| {
            WAILParseError::UnexpectedToken {
                found: "invalid whitespace".to_string(),
                location: error_location(e, original_input),
            }
        })?;

        let mut definitions: Vec<WAILDefinition<'a>> = vec![];

        let mut input = input;

        while !input.is_empty() && input.starts_with("import") {
            let (new_input, import) = self.parse_import(input).map_err(|e| {
                let emsg = e.to_string();
                match e {
                    nom::Err::Failure(ErrorTree::Stack { ref base, .. }) => {
                        if let ErrorTree::Base {
                            kind: BaseErrorKind::External(ref err),
                            ..
                        } = **base
                        {
                            if let Some(circular_err) = err.downcast_ref::<WAILParseError>() {
                                match circular_err {
                                    WAILParseError::CircularImport { path, chain } => {
                                        WAILParseError::CircularImport {
                                            path: path.clone(),
                                            chain: chain.clone(),
                                        }
                                    }
                                    _ => WAILParseError::UnexpectedToken {
                                        found: emsg.to_string(),
                                        location: error_location(e, original_input),
                                    },
                                }
                            } else {
                                WAILParseError::UnexpectedToken {
                                    found: e.to_string(),
                                    location: error_location(e, original_input),
                                }
                            }
                        } else {
                            WAILParseError::UnexpectedToken {
                                found: e.to_string(),
                                location: error_location(e, original_input),
                            }
                        }
                    }
                    _ => WAILParseError::UnexpectedToken {
                        found: e.to_string(),
                        location: error_location(e, original_input),
                    },
                }
            })?;

            input = new_input;
            definitions.push(import);

            let (new_input, _) =
                multispace0(input).map_err(|e: nom::Err<ErrorTree<&'a str>>| {
                    WAILParseError::UnexpectedToken {
                        found: "invalid whitespace".to_string(),
                        location: error_location(e, original_input),
                    }
                })?;
            input = new_input;
        }

        self.resolve_imports(&definitions)?;

        loop {
            let (new_input, _) =
                multispace0(input).map_err(|e: nom::Err<ErrorTree<&'a str>>| {
                    WAILParseError::UnexpectedEOF {
                        expected: "Continuation of definition or comment".to_string(),
                        location: error_location(e, original_input),
                    }
                })?;

            input = new_input;

            if input.is_empty() {
                break;
            }

            let (new_input, definition) = if input.starts_with("#") {
                self.parse_comment(input)
                    .map_err(|e| WAILParseError::UnexpectedToken {
                        found: e.to_string(),
                        location: error_location(e, original_input),
                    })?
            } else if input.starts_with("object")
                || input.starts_with("template")
                || input.starts_with("union")
            {
                self.parse_definition(input).map_err(|e| match e {
                    nom::Err::Failure(ErrorTree::Base {
                        location: _,
                        kind: BaseErrorKind::Kind(nom::error::ErrorKind::Verify),
                    }) => WAILParseError::DuplicateDefinition {
                        name: input
                            .split_whitespace()
                            .nth(1)
                            .unwrap_or("unknown")
                            .to_string(),
                        location: error_location(e, original_input),
                    },
                    nom::Err::Error(ErrorTree::Base { location, .. }) => {
                        WAILParseError::UnexpectedToken {
                            found: location.lines().next().unwrap_or("unknown").to_string(),
                            location: error_location(e, original_input),
                        }
                    }
                    e => WAILParseError::UnexpectedEOF {
                        expected: "Unexpected Error".to_string(),
                        location: error_location(e, original_input),
                    },
                })?
            } else {
                break;
            };

            input = new_input;

            definitions.push(definition);
        }

        match file_type {
            WAILFileType::Library => Ok(definitions),
            WAILFileType::Application => {
                if input.len() < 4 || &input[0..4] != "main" {
                    return Err(WAILParseError::MissingMainBlock);
                }

                // Parse required main block at the end
                match self.parse_main(input) {
                    Ok((_, main_def)) => {
                        definitions.push(WAILDefinition::Main(main_def));

                        Ok(definitions)
                    }
                    Err(e) => {
                        let err = match e {
                            nom::Err::Failure(ErrorTree::Base {
                                location: _,
                                kind: BaseErrorKind::Kind(nom::error::ErrorKind::Verify),
                            }) => WAILParseError::DuplicateDefinition {
                                name: input
                                    .split_whitespace()
                                    .nth(1)
                                    .unwrap_or("unknown")
                                    .to_string(),
                                location: error_location(e, original_input),
                            },
                            nom::Err::Error(ErrorTree::Base { location, .. }) => {
                                WAILParseError::UnexpectedToken {
                                    found: location.lines().next().unwrap_or("unknown").to_string(),
                                    location: error_location(e, original_input),
                                }
                            }
                            e => WAILParseError::UnexpectedEOF {
                                expected: "Unexpected Error".to_string(),
                                location: error_location(e, original_input),
                            },
                        };

                        Err(err)
                    }
                }
            }
        }
    }

    fn parse_comment(
        &'a self,
        input: &'a str,
    ) -> IResult<&'a str, WAILDefinition<'a>, ErrorTree<&'a str>> {
        let (input, _) = tuple((multispace0, tag("#"), multispace0, take_until("\n")))(input)?;
        Ok((input, WAILDefinition::Comment(input.to_string())))
    }

    fn parse_object(
        &'a self,
        input: &'a str,
    ) -> IResult<&'a str, WAILDefinition<'a>, ErrorTree<&'a str>> {
        // Parse: Object Name { ... }
        let (input, _) = tuple((tag("object"), multispace1))(input)?;
        let (input, name) = self.identifier(input)?;

        if self.registry.borrow().contains_key(name) {
            return Err(nom::Err::Failure(ErrorTree::from_error_kind(
                input,
                nom::error::ErrorKind::Verify, // Using Verify since this is a validation error
            )));
        }

        let (input, _) = multispace0(input)?;
        let (input, mut fields) = delimited(
            char('{'),
            many1(delimited(multispace0, |i| self.parse_field(i), multispace0)),
            char('}'),
        )(input)?;

        // Convert fields into HashMap
        let mut field_map = HashMap::new();
        for field in &fields {
            field_map.insert(
                WAILString {
                    value: field.name.clone(),
                    type_data: WAILTypeData {
                        json_type: JsonValue::String(field.name.clone()),
                        type_name: "String",
                        field_definitions: None,
                        element_type: None,
                    },
                },
                field.field_type.clone(),
            );
        }

        field_map.insert(
            WAILString {
                value: "_type".to_string(),
                type_data: WAILTypeData {
                    json_type: JsonValue::String("_type".to_string()),
                    type_name: "String",
                    field_definitions: None,
                    element_type: None,
                },
            },
            WAILType::Simple(WAILSimpleType::String(WAILString {
                value: name.to_string(),
                type_data: WAILTypeData {
                    json_type: JsonValue::String("_type".to_string()),
                    type_name: "String",
                    field_definitions: None,
                    element_type: None,
                },
            })),
        );

        fields.push(WAILField {
            name: "_type".to_string(),
            field_type: WAILType::Simple(WAILSimpleType::String(WAILString {
                value: name.to_string(),
                type_data: WAILTypeData {
                    json_type: JsonValue::String("_type".to_string()),
                    type_name: "String",
                    field_definitions: None,
                    element_type: None,
                },
            })),
            annotations: vec![],
        });

        let (input, annotations) = many0(|i| self.parse_annotation(i))(input)?;

        let object = WAILObject {
            value: field_map,
            type_data: WAILTypeData {
                json_type: JsonValue::Object(HashMap::new()), // Placeholder empty object
                type_name: name,
                field_definitions: Some(fields),
                element_type: None,
            },
        };

        let field = WAILField {
            name: name.to_string(),
            field_type: WAILType::Composite(WAILCompositeType::Object(object)),
            annotations: annotations,
        };

        let definition = WAILDefinition::Object(field.clone());

        self.registry
            .borrow_mut()
            .insert(name.to_string(), field.clone());

        Ok((input, definition))
    }

    fn parse_field(&'a self, input: &'a str) -> IResult<&str, WAILField, ErrorTree<&'a str>> {
        let (input, (name, _, _, (field_type, _), _)) = tuple((
            |i| self.identifier(i),
            char(':'),
            multispace0,
            |i| self.parse_type(i, None),
            opt(|i| self.parse_comment(i)),
        ))(input)?;

        let (input, annotations) = many0(|i| self.parse_annotation(i))(input)?;

        Ok((
            input,
            WAILField {
                name: name.to_string(),
                field_type,
                annotations: annotations,
            },
        ))
    }

    fn parse_adhoc_object_type(
        &'a self,
        input: &'a str,
    ) -> IResult<&str, (WAILType<'a>, String), ErrorTree<&'a str>> {
        let adhoc_id = self.adhoc_obj_ref_id_counter.borrow().clone() + 1;
        let (input, _) = multispace0(input)?;
        let (input, fields) = delimited(
            char('{'),
            many1(delimited(multispace0, |i| self.parse_field(i), multispace0)),
            char('}'),
        )(input)?;

        // Create object type as before...
        let mut field_map = HashMap::new();
        for field in &fields {
            field_map.insert(
                WAILString {
                    value: field.name.clone(),
                    type_data: WAILTypeData {
                        json_type: JsonValue::String(field.name.clone()),
                        type_name: "String",
                        field_definitions: None,
                        element_type: None,
                    },
                },
                field.field_type.clone(),
            );
        }

        self.adhoc_obj_ids.borrow_mut().push(adhoc_id.to_string());

        let object = WAILObject {
            value: field_map,
            type_data: WAILTypeData {
                json_type: JsonValue::Object(HashMap::new()),
                type_name: Box::leak(adhoc_id.to_string().into_boxed_str()),
                field_definitions: Some(fields),
                element_type: None,
            },
        };

        self.adhoc_obj_refs
            .borrow_mut()
            .insert(adhoc_id.to_string(), object.clone());

        let field = WAILField {
            name: adhoc_id.to_string(),
            field_type: WAILType::Composite(WAILCompositeType::Object(object)),
            annotations: vec![],
        };

        self.registry
            .borrow_mut()
            .insert(adhoc_id.to_string(), field.clone());

        *self.adhoc_obj_ref_id_counter.borrow_mut() += 1;

        Ok((input, (field.field_type, adhoc_id.to_string().clone())))
    }

    fn parse_type(
        &'a self,
        input: &'a str,
        complex_type_name: Option<&'a str>,
    ) -> IResult<&str, (WAILType<'a>, String), ErrorTree<&'a str>> {
        if input.starts_with('{') {
            let (input, (obj_type, id)) = self.parse_adhoc_object_type(input)?;
            return Ok((input, (obj_type, id)));
        }

        // Parse first type identifier
        let (input, base_type) = self.identifier(input)?;
        let (input, _) = multispace0(input)?;

        // Check if base type is an array
        let (mut input, base_is_array) = opt(tag("[]"))(input)?;

        // Look for union syntax
        let mut union_members = vec![];
        let mut is_union = false;

        while let Ok((remaining, _)) =
            tuple((char('|'), multispace0::<&str, nom::error::Error<&str>>))(input)
        {
            is_union = true;
            // Parse type identifier for union member
            match self.identifier(remaining) {
                Ok((new_input, type_name)) => {
                    input = new_input;
                    union_members.push(type_name);
                }
                Err(_) => break,
            }
            match multispace0::<&str, nom::error::Error<&str>>(input) {
                Ok((new_input, _)) => input = new_input,
                Err(_) => break,
            }
        }

        // Create base type value
        let base_type_val = self.create_type_value(base_type, base_is_array.is_some())?;

        // If we found union syntax, create a union type
        let final_type = if is_union {
            let mut members = vec![base_type_val];

            // Process additional union members
            for type_name in union_members {
                let member_type = self.create_type_value(type_name, false)?;
                members.push(member_type);
            }

            let mut wail_fields = vec![];

            let reg_borrow = self.registry.borrow();

            for type_data in members.iter() {
                let field_opt = reg_borrow.get(type_data.type_name());

                match field_opt {
                    Some(field) => wail_fields.push(field.clone()),
                    None => {
                        let field = WAILField {
                            name: type_data.type_name().to_string(),
                            field_type: type_data.clone(),
                            annotations: vec![],
                        };

                        wail_fields.push(field)
                    }
                }
            }

            WAILType::Composite(WAILCompositeType::Union(WAILUnion {
                members: wail_fields,
                type_data: WAILTypeData {
                    json_type: JsonValue::Object(HashMap::new()),
                    type_name: &complex_type_name.unwrap_or("Union"),
                    field_definitions: None,
                    element_type: None,
                },
            }))
        } else {
            base_type_val
        };

        // Handle array type wrapping for the whole type/union

        Ok((input, (final_type, base_type.to_string())))
    }

    // Helper function to create type values
    fn create_type_value(
        &'a self,
        type_name: &'a str,
        is_array: bool,
    ) -> Result<WAILType<'a>, nom::Err<ErrorTree<&'a str>>> {
        let inner_type = match type_name {
            "String" => WAILType::Simple(WAILSimpleType::String(WAILString {
                value: String::new(),
                type_data: WAILTypeData {
                    json_type: JsonValue::String(String::new()),
                    type_name: type_name,
                    field_definitions: None,
                    element_type: None,
                },
            })),
            "Number" => {
                WAILType::Simple(WAILSimpleType::Number(WAILNumber::Integer(WAILInteger {
                    value: 0,
                    type_data: WAILTypeData {
                        json_type: JsonValue::Number(Number::Integer(0)),
                        type_name: type_name,
                        field_definitions: None,
                        element_type: None,
                    },
                })))
            }
            // Handle both registered and unregistered types
            _ => {
                if let Some(field) = self.registry.borrow().get(type_name) {
                    match &field.field_type {
                        WAILType::Composite(WAILCompositeType::Object(_)) => {
                            field.field_type.clone()
                        }
                        WAILType::Composite(WAILCompositeType::Union(_)) => {
                            field.field_type.clone()
                        }
                        _ => {
                            // If it's not an Object or Union type, treat as unregistered
                            WAILType::Composite(WAILCompositeType::Object(WAILObject {
                                value: HashMap::new(),
                                type_data: WAILTypeData {
                                    json_type: JsonValue::Object(HashMap::new()),
                                    type_name: type_name,
                                    field_definitions: None,
                                    element_type: None,
                                },
                            }))
                        }
                    }
                } else {
                    // Create a placeholder object type for unregistered types
                    WAILType::Composite(WAILCompositeType::Object(WAILObject {
                        value: HashMap::new(),
                        type_data: WAILTypeData {
                            json_type: JsonValue::Object(HashMap::new()),
                            type_name: type_name,
                            field_definitions: None,
                            element_type: None,
                        },
                    }))
                }
            }
        };

        if is_array {
            Ok(WAILType::Composite(WAILCompositeType::Array(WAILArray {
                values: vec![],
                type_data: WAILTypeData {
                    json_type: JsonValue::Array(vec![]),
                    type_name: "Array",
                    field_definitions: None,
                    element_type: Some(Box::new(inner_type)),
                },
            })))
        } else {
            Ok(inner_type)
        }
    }

    fn identifier(&'a self, input: &'a str) -> IResult<&'a str, &'a str, ErrorTree<&'a str>> {
        take_while1(|c: char| c.is_alphanumeric() || c == '_')(input)
    }

    fn parse_template(
        &'a self,
        input: &'a str,
    ) -> IResult<&'a str, WAILDefinition<'a>, ErrorTree<&'a str>> {
        // Parse: template Name(param: Type) -> ReturnType { prompt: """ ... """ }
        let (input, _) = tuple((tag("template"), multispace1))(input)?;

        // Parse template name
        let (input, name) = self.identifier(input)?;

        if self.template_registry.borrow().contains_key(name) {
            return Err(nom::Err::Failure(ErrorTree::from_error_kind(
                input,
                nom::error::ErrorKind::Verify,
            )));
        }

        let (input, _) = multispace0(input)?;

        // Parse parameters in parentheses
        let (input, params) = delimited(
            char('('),
            preceded(
                multispace0,
                separated_list0(tuple((multispace0, char(','), multispace0)), |i| {
                    self.parse_parameter(i)
                }),
            ),
            preceded(multispace0, char(')')),
        )(input)?;

        // Parse return type
        let (input, _) = tuple((multispace0, tag("->"), multispace0))(input)?;

        let (input, (return_type, identifier)) = self.parse_type(input, None)?;

        let (input, _) = multispace0(input)?;
        let (input, annotations) = many0(|input| self.parse_annotation(input))(input)?;

        // Parse template body with prompt template
        let (input, _) = tuple((multispace0, char('{'), multispace0))(input)?;
        let (input, _) = tuple((tag("prompt:"), multispace0))(input)?;
        let (input, template) =
            delimited(tag(r#"""""#), take_until(r#"""""#), tag(r#"""""#))(input)?;

        let template_adjusted = adjust_indentation(&template, 0);

        let (input, _) = tuple((multispace0, char('}')))(input)?;

        // Create output field for both registered and unregistered types
        let output_field = WAILField {
            name: identifier.clone(),
            field_type: return_type,
            annotations: vec![],
        };

        let template_def = WAILTemplateDef {
            name: name.to_string(),
            inputs: params,
            output: output_field,
            prompt_template: template_adjusted,
            annotations,
        };

        self.template_registry
            .borrow_mut()
            .insert(name.to_string(), template_def.clone());

        Ok((input, WAILDefinition::Template(template_def)))
    }

    fn parse_template_call(
        &'a self,
        input: &'a str,
    ) -> IResult<&'a str, WAILTemplateCall, ErrorTree<&'a str>> {
        let (input, template_name) = self.identifier(input)?;
        let (input, _) = tuple((multispace0, char('('), multispace0))(input)?;

        // Parse arguments as key-value pairs
        let (input, args) = separated_list0(tuple((multispace0, char(','), multispace0)), |i| {
            self.parse_argument(i)
        })(input)?;

        let (input, _) = tuple((multispace0, char(')')))(input)?;

        let mut arguments = HashMap::new();
        for (name, value) in args {
            arguments.insert(name, value);
        }

        Ok((
            input,
            WAILTemplateCall {
                template_name: template_name.to_string(),
                arguments,
            },
        ))
    }

    fn parse_string_literal(
        &'a self,
        input: &'a str,
    ) -> IResult<&'a str, TemplateArgument, ErrorTree<&'a str>> {
        let (input, _) = char('"')(input)?;
        let (input, content) = take_until("\"")(input)?;
        let (input, _) = char('"')(input)?;
        Ok((input, TemplateArgument::String(content.to_string())))
    }

    fn parse_number(
        &'a self,
        input: &'a str,
    ) -> IResult<&'a str, TemplateArgument, ErrorTree<&'a str>> {
        let (input, num_str) = take_while1(|c: char| c.is_ascii_digit())(input)?;
        let num = num_str.parse::<i64>().map_err(|_| {
            nom::Err::Error(ErrorTree::from_error_kind(
                input,
                nom::error::ErrorKind::Digit,
            ))
        })?;
        Ok((input, TemplateArgument::Number(num)))
    }

    fn parse_type_ref(
        &'a self,
        input: &'a str,
    ) -> IResult<&'a str, TemplateArgument, ErrorTree<&'a str>> {
        let (input, type_name) = self.identifier(input)?;
        Ok((input, TemplateArgument::TypeRef(type_name.to_string())))
    }

    fn parse_value(
        &'a self,
        input: &'a str,
    ) -> IResult<&'a str, TemplateArgument, ErrorTree<&'a str>> {
        // First check if this is a variable reference
        if let Some(var_name) = self.object_instances.borrow().keys().find(|k| *k == input) {
            return Ok(("", TemplateArgument::TemplateArgRef(var_name.clone())));
        }

        alt((
            |i| self.parse_string_literal(i),
            |i| self.parse_number(i),
            |i| self.parse_type_ref(i),
        ))(input)
    }

    fn parse_argument(
        &'a self,
        input: &'a str,
    ) -> IResult<&'a str, (String, TemplateArgument), ErrorTree<&'a str>> {
        let (input, name) = self.identifier(input)?;
        let (input, _) = tuple((multispace0, char(':'), multispace0))(input)?;
        // First try to parse a template arg reference with $ prefix
        if let Ok((remaining, _)) = char::<&str, ErrorTree<&str>>('$')(input) {
            let (remaining, arg_name) = self.identifier(remaining)?;
            return Ok((
                remaining,
                (
                    name.to_string(),
                    TemplateArgument::TemplateArgRef(arg_name.to_string()),
                ),
            ));
        }

        // Try to parse identifier first
        let res = self.identifier(input);
        if res.is_ok() {
            let (input, value_str) = res.unwrap();
            // Check if this is an object instance reference
            if self.object_instances.borrow().contains_key(value_str) {
                return Ok((
                    input,
                    (
                        name.to_string(),
                        TemplateArgument::ObjectRef(value_str.to_string()),
                    ),
                ));
            }

            // Check if this is an object type that exists in the registry
            if self.registry.borrow().contains_key(value_str) {
                if let Some(field) = self.registry.borrow().get(value_str) {
                    if let WAILType::Composite(WAILCompositeType::Object(_)) = &field.field_type {
                        return Ok((
                            input,
                            (
                                name.to_string(),
                                TemplateArgument::TypeRef(value_str.to_string()),
                            ),
                        ));
                    }
                }
            }
        }

        // If not an object reference or type, try parsing as a literal value
        let (input, value) = alt((
            |i| self.parse_string_literal(i),
            |i| self.parse_number(i),
            |i| self.parse_type_ref(i),
        ))(input)?;

        return Ok((input, (name.to_string(), value)));
    }

    fn parse_main(
        &'a self,
        input: &'a str,
    ) -> IResult<&'a str, WAILMainDef<'a>, ErrorTree<&'a str>> {
        if self.main.borrow().is_some() {
            return Err(nom::Err::Error(ErrorTree::from_error_kind(
                input,
                nom::error::ErrorKind::Verify,
            )));
        }

        // Parse main opening
        let (input, _) = tuple((tag("main"), multispace0, char('{'), multispace0))(input)?;

        let (input, template_args) = opt(|input| {
            let (input, _) =
                tuple((tag("template_args"), multispace0, char('{'), multispace0))(input)?;
            let (input, args) =
                separated_list0(tuple((multispace0, char(','), multispace0)), |i| {
                    self.parse_template_arg(i)
                })(input)?;
            let (input, _) = tuple((multispace0, char('}'), multispace0))(input)?;

            let args_map: HashMap<_, _> = args.into_iter().collect();
            Ok((input, args_map))
        })(input)?;

        // Parse statements (assignments, template calls, and object instantiations)
        let (input, statements) = many0(|i| {
            let (i, statement) = alt((
                |input| {
                    // Parse assignment: let var = template_call;
                    let (input, _) = tuple((multispace0, tag("let"), multispace1))(input)?;
                    let (input, var_name) = self.identifier(input)?;
                    let (input, _) = tuple((multispace0, char('='), multispace0))(input)?;

                    // Parse the template call first
                    let (input, template_call) = self.parse_template_call(input)?;

                    // First check if it's an object instantiation
                    if let Some(obj) = self.registry.borrow().get(&template_call.template_name) {
                        if let WAILType::Composite(WAILCompositeType::Object(_)) = &obj.field_type {
                            let (input, _) = tuple((multispace0, char(';'), multispace0))(input)?;
                            let obj = self
                                .instantiate_object(
                                    &var_name,
                                    &template_call.template_name,
                                    template_call.arguments.clone(),
                                )
                                .unwrap();

                            self.object_instances
                                .borrow_mut()
                                .insert(var_name.to_string(), obj);

                            return Ok((
                                input,
                                MainStatement::ObjectInstantiation {
                                    variable: var_name.to_string(),
                                    object_type: template_call.template_name,
                                    arguments: template_call.arguments,
                                },
                            ));
                        }
                    }

                    // Then check if it's a template call
                    if self
                        .template_registry
                        .borrow()
                        .contains_key(&template_call.template_name)
                    {
                        let (input, _) = tuple((multispace0, char(';'), multispace0))(input)?;
                        Ok((
                            input,
                            MainStatement::Assignment {
                                variable: var_name.to_string(),
                                template_call: template_call.clone(),
                            },
                        ))
                    } else {
                        // Neither a valid object type nor template
                        Err(nom::Err::Error(ErrorTree::from_error_kind(
                            input,
                            nom::error::ErrorKind::Tag,
                        )))
                    }
                },
                |input| {
                    // Parse the template call first
                    let (input, template_call) = self.parse_template_call(input)?;
                    // Then check if it exists in registry
                    if self
                        .template_registry
                        .borrow()
                        .contains_key(&template_call.template_name)
                    {
                        let (input, _) = tuple((multispace0, char(';'), multispace0))(input)?;
                        Ok((input, MainStatement::TemplateCall(template_call)))
                    } else {
                        // Not a valid template call
                        Err(nom::Err::Error(ErrorTree::from_error_kind(
                            input,
                            nom::error::ErrorKind::Tag,
                        )))
                    }
                },
                |input: &'a str| {
                    // Parse comment: # comment
                    let (input, (_, _, _, comment)) =
                        tuple((multispace0, tag("#"), multispace0, take_until("\n")))(input)?;

                    Ok((input, MainStatement::Comment(comment.to_string())))
                },
            ))(i)?;
            Ok((i, statement))
        })(input)?;

        // Parse prompt block
        let (input, _) = tuple((
            multispace0,
            tag("prompt"),
            multispace0,
            char('{'),
            // multispace0,
        ))(input)?;

        // Take everything until the closing brace of prompt, handling nested braces
        let mut brace_count = 1;
        let mut prompt_end = 0;
        let chars: Vec<_> = input.chars().collect();

        for (i, &c) in chars.iter().enumerate() {
            match c {
                '{' => brace_count += 1,
                '}' => {
                    brace_count -= 1;
                    if brace_count == 0 {
                        prompt_end = i;
                        break;
                    }
                }
                _ => {}
            }
        }

        let (prompt_str, input) = input.split_at(prompt_end);
        let (input, _) = tuple((char('}'), multispace0))(input)?;

        // Parse main's closing brace
        let (input, _) = tuple((char('}'), multispace0))(input)?;

        let prompt_str_trimmed = prompt_str
            .to_string()
            .lines()
            .map(|line| line.trim())
            .collect::<Vec<&str>>()
            .join("\n");

        let main = WAILMainDef::new(statements, prompt_str_trimmed, template_args);

        self.main.borrow_mut().replace(main.clone());

        Ok((input, main))
    }

    fn parse_template_arg(
        &'a self,
        input: &'a str,
    ) -> IResult<&'a str, (String, WAILType<'a>), ErrorTree<&'a str>> {
        let (input, name) = self.identifier(input)?;
        let (input, _) = tuple((multispace0, char(':'), multispace0))(input)?;
        let (input, (arg_type, _)) = self.parse_type(input, None)?;

        Ok((input, (name.to_string(), arg_type)))
    }

    fn parse_annotation(
        &'a self,
        input: &'a str,
    ) -> IResult<&'a str, WAILAnnotation, ErrorTree<&'a str>> {
        let (input, _) = tuple((char('@'), tag("description"), char('('), char('"')))(input)?;
        let (input, desc) = take_until("\"")(input)?;
        let (input, _) = char('"')(input)?;
        let (input, _) = char(')')(input)?;
        let (input, _) = multispace0(input)?;

        Ok((input, WAILAnnotation::Description(desc.to_string())))
    }

    fn parse_parameter(
        &'a self,
        input: &'a str,
    ) -> IResult<&'a str, WAILField, ErrorTree<&'a str>> {
        let (input, (name, _, _, (param_type, _))) = tuple((
            |i| self.identifier(i),
            char(':'),
            multispace0,
            |i| self.parse_type(i, None),
        ))(input)?;

        Ok((
            input,
            WAILField {
                name: name.to_string(),
                field_type: param_type,
                annotations: vec![],
            },
        ))
    }

    fn parse_definition(
        &'a self,
        input: &'a str,
    ) -> IResult<&'a str, WAILDefinition<'a>, ErrorTree<&'a str>> {
        let (_, tag) = alt((tag("object"), tag("template"), tag("union")))(input)?;

        let (input, res) = match tag {
            "object" => self.parse_object(input),
            "template" => self.parse_template(input),
            "union" => self.parse_union(input),
            _ => unreachable!(),
        }?;

        Ok((input, res))
    }

    pub fn get_error_location(&self, error: &JsonValidationError) -> Vec<PathSegment> {
        match error {
            JsonValidationError::ObjectMissingMetaType => {
                vec![PathSegment::MissingMetaType]
            }
            JsonValidationError::ObjectNestedTypeValidation((field, nested_error)) => {
                let mut path = vec![PathSegment::Field(field.clone())];
                path.extend(self.get_error_location(nested_error));
                path
            }
            JsonValidationError::ArrayElementTypeError((index, nested_error)) => {
                let mut path = vec![PathSegment::ArrayIndex(*index)];
                path.extend(self.get_error_location(nested_error));
                path
            }
            JsonValidationError::NotMemberOfUnion((field, validation_errors)) => {
                vec![PathSegment::UnionType(
                    field.clone(),
                    validation_errors.clone(),
                )]
            }
            JsonValidationError::ObjectMissingRequiredField(field) => {
                vec![PathSegment::Field(field.clone())]
            }
            JsonValidationError::ExpectedTypeError((field, expected_type)) => {
                vec![PathSegment::ExpectedType(
                    field.clone(),
                    expected_type.clone(),
                )]
            }
            _ => vec![],
        }
    }

    fn infer_type_from_fields(&self, fields: &HashMap<String, JsonValue>) -> Option<String> {
        let mut matches: Vec<String> = vec![];

        let reg_borrow = self.registry.borrow();

        for (name, field) in reg_borrow.clone().into_iter() {
            match field.field_type.field_definitions() {
                Some(obj_fields) => {
                    let fieldset: HashSet<String> =
                        obj_fields.iter().map(|field| field.name.clone()).collect();

                    let original_set: HashSet<String> = fields.keys().cloned().collect();

                    let diff = original_set.difference(&fieldset);
                    let diff_vec: Vec<String> = diff.cloned().collect();

                    if (diff_vec.len() == 1 && diff_vec.first().unwrap() == "_type")
                        || diff_vec.len() == 0
                    {
                        matches.push(name.clone())
                    }
                }
                None => continue,
            }
        }

        if matches.len() > 1 || matches.len() == 0 {
            None
        } else {
            Some(matches.first().unwrap().clone())
        }
    }

    // Helper function to apply fixes based on the path
    pub fn fix_json_value(&self, json: &mut JsonValue, path: &[PathSegment]) -> Result<(), String> {
        match path.split_first() {
            Some((PathSegment::MissingMetaType, _)) => {
                if let JsonValue::Object(map) = json {
                    if let Some(inferred_type) = self.infer_type_from_fields(map) {
                        map.insert("_type".to_string(), JsonValue::String(inferred_type));
                        Ok(())
                    } else {
                        Err("Could not unambiguously determine type from fields".to_string())
                    }
                } else {
                    Err("Expected object".to_string())
                }
            }
            Some((PathSegment::ArrayIndex(idx), rest)) => {
                if let JsonValue::Array(arr) = json {
                    if let Some(element) = arr.get_mut(*idx) {
                        self.fix_json_value(element, rest)
                    } else {
                        Err(format!("Expected element in array at index {idx}"))
                    }
                } else {
                    Err("Expected to be array at this level.".to_string())
                }
            }
            Some((PathSegment::Field(field), rest)) => {
                if rest.is_empty() {
                    Err(format!("Missing field {field} from object."))
                } else {
                    if let JsonValue::Object(map) = json {
                        match map.get_mut(field) {
                            None => Err(format!("Missing field {field} from object.")),
                            Some(json2) => self.fix_json_value(json2, rest),
                        }
                    } else {
                        Err("Expected object".to_string())
                    }
                }
            }
            Some((PathSegment::UnionType(field, validation_errors), _)) => {
                match json {
                    JsonValue::Object(map) => {
                        // Try each possible union type and its validation errors
                        for (type_name, errors) in validation_errors {
                            // Clone the object to try fixes without modifying original
                            let mut test_json = json.clone();

                            // Get a new path from these errors to try fixing
                            let error_path = self.get_error_location(errors);

                            // Try to fix the validation errors for this type
                            let res = self.fix_json_value(&mut test_json, &error_path);
                            if res.is_ok() {
                                // If successful, apply the changes back to original
                                *json = test_json;
                                return Ok(());
                            }
                        }
                        Err("Could not coerce to any union type".to_string())
                    }
                    _ => Err("Expected object for union type".to_string()),
                }
            }
            Some((PathSegment::ExpectedType(field, expected), rest)) => match field {
                None => {
                    match expected.as_str() {
                        "Array" => {
                            *json = JsonValue::Array(vec![json.to_owned()]);
                            Ok(())
                        }
                        "String" => match json {
                            JsonValue::Number(n) => {
                                *json = JsonValue::String(n.to_string());
                                Ok(())
                            }
                            JsonValue::Boolean(b) => {
                                *json = JsonValue::String(b.to_string());
                                Ok(())
                            }
                            JsonValue::Null => {
                                *json = JsonValue::String("null".to_string());
                                Ok(())
                            }
                            JsonValue::String(_) => Ok(()), // Already a string
                            _ => Err("Cannot convert to string".to_string()),
                        },
                        "Number" => match json {
                            JsonValue::String(s) => {
                                if let Ok(n) = s.parse::<i64>() {
                                    *json = JsonValue::Number(Number::Integer(n.into()));
                                    Ok(())
                                } else {
                                    Err("String is not a valid number".to_string())
                                }
                            }
                            JsonValue::Boolean(b) => {
                                *json = JsonValue::Number(Number::Integer(
                                    if *b { 1 } else { 0 }.into(),
                                ));
                                Ok(())
                            }
                            JsonValue::Number(_) => Ok(()), // Already a number
                            _ => Err("Cannot convert to number".to_string()),
                        },
                        "Boolean" => match json {
                            JsonValue::String(s) => match s.to_lowercase().as_str() {
                                "true" | "1" => {
                                    *json = JsonValue::Boolean(true);
                                    Ok(())
                                }
                                "false" | "0" => {
                                    *json = JsonValue::Boolean(false);
                                    Ok(())
                                }
                                _ => Err("String is not a valid boolean".to_string()),
                            },
                            JsonValue::Number(n) => {
                                if n.is_i64() {
                                    match n.as_i64() {
                                        0 => {
                                            *json = JsonValue::Boolean(false);
                                            Ok(())
                                        }
                                        1 => {
                                            *json = JsonValue::Boolean(true);
                                            Ok(())
                                        }
                                        _ => Err("Number is not 0 or 1".to_string()),
                                    }
                                } else {
                                    Err("Cannot convert float to boolean".to_string())
                                }
                            }
                            JsonValue::Boolean(_) => Ok(()), // Already a bool
                            _ => Err("Cannot convert to boolean".to_string()),
                        },
                        "Null" => {
                            *json = JsonValue::Null;
                            Ok(())
                        }
                        _ => Err("Unrecognized type for conversion".to_string()),
                    }
                }
                Some(field_name) => match json {
                    JsonValue::Object(map) => {
                        if let Some(value) = map.get_mut(field_name) {
                            self.fix_json_value(value, rest)?;
                        }
                        Ok(())
                    }
                    _ => Err(format!(
                        "Cannot access field {} on non-object value",
                        field_name
                    )),
                },
            },
            Some((PathSegment::Root((_template, variable)), rest)) => match variable {
                Some(field_name) => match json {
                    JsonValue::Object(map) => {
                        if let Some(value) = map.get_mut(field_name) {
                            self.fix_json_value(value, rest)?;
                        }
                        Ok(())
                    }
                    _ => Err(format!(
                        "Cannot access field {} on non-object value",
                        field_name
                    )),
                },
                None => self.fix_json_value(json, rest),
            },
            None => Err("Invalid error path -no segments".to_string()),
        }
    }

    pub fn validate_json(
        &self,
        json: &str,
    ) -> Result<(), (String, Option<String>, JsonValidationError)> {
        let mut parser = JsonParser::new(json.as_bytes().to_vec());
        let value = parser.parse().map_err(|e| {
            (
                "".to_string(),
                None,
                JsonValidationError::JsonParserError(e),
            )
        })?;

        self.main
            .borrow()
            .as_ref()
            .unwrap()
            .validate_llm_response(&value, &self.template_registry.borrow())
    }

    pub fn validate_and_fix(&self, json: &mut JsonValue) -> Result<(), String> {
        loop {
            match self.validate_json(&json.to_string()) {
                Ok(_) => return Ok(()),
                Err((template, variable, err)) => {
                    let mut path = self.get_error_location(&err);
                    path.insert(0, PathSegment::Root((template, variable)));

                    self.fix_json_value(json, &path)?;
                }
            }
        }
    }

    pub fn validate(&self) -> (Vec<ValidationWarning>, Vec<ValidationError>) {
        let mut warnings = Vec::new();
        let mut errors = Vec::new();
        let registry = self.registry.borrow();
        let template_registry = self.template_registry.borrow();

        // Check if there's a main block
        if self.main.borrow().is_none() {
            warnings.push(ValidationWarning::NoMainBlock);
        }

        // Check all templates
        for (template_name, template) in template_registry.iter() {
            // Check input parameters
            for param in &template.inputs {
                self.validate_type(
                    &param.field_type,
                    &registry,
                    template_name,
                    &mut warnings,
                    &mut errors,
                    false,
                );
            }

            // Check return type
            self.validate_type(
                &template.output.field_type,
                &registry,
                template_name,
                &mut warnings,
                &mut errors,
                true,
            );
        }

        (warnings, errors)
    }

    fn parse_union(
        &'a self,
        input: &'a str,
    ) -> IResult<&'a str, WAILDefinition<'a>, ErrorTree<&'a str>> {
        // Parse: union Name = Type1 | Type2 | Type3
        let (input, _) = tuple((tag("union"), multispace1))(input)?;
        let (input, name) = self.identifier(input)?;

        if self.registry.borrow().contains_key(name) {
            return Err(nom::Err::Failure(ErrorTree::from_error_kind(
                input,
                nom::error::ErrorKind::Verify,
            )));
        }

        let (input, _) = tuple((multispace0, char('='), multispace0))(input)?;

        let (input, (union, _)) = self.parse_type(input, Some(name))?;

        let (input, _) = tuple((multispace0, char(';')))(input)?;

        let field = WAILField {
            name: name.to_string(),
            field_type: union,
            annotations: Vec::new(),
        };

        self.registry
            .borrow_mut()
            .insert(name.to_string(), field.clone());

        Ok((input, WAILDefinition::Union(field)))
    }

    fn validate_type(
        &self,
        wail_type: &WAILType,
        registry: &HashMap<String, WAILField>,
        template_name: &str,
        warnings: &mut Vec<ValidationWarning>,
        errors: &mut Vec<ValidationError>,
        is_return_type: bool,
    ) {
        match wail_type {
            WAILType::Simple(_) => (), // Built-in types are always valid
            WAILType::Composite(composite) => match composite {
                WAILCompositeType::Array(array) => {
                    // Check if the element type exists if it's a custom type
                    if let Some(element_type) = &array.type_data.element_type {
                        let element_type_str = element_type.type_name().to_string();
                        if element_type_str != "String"
                            && element_type_str != "Number"
                            && !registry.contains_key(&element_type_str)
                        {
                            // For array element types in templates, undefined types are errors
                            errors.push(ValidationError::UndefinedTypeInTemplate {
                                template_name: template_name.to_string(),
                                type_name: element_type_str.clone(),
                                is_return_type,
                            });

                            // Check for possible typos
                            for known_type in registry.keys() {
                                if known_type.len() > 2
                                    && element_type_str.len() > 2
                                    && known_type
                                        .chars()
                                        .zip(element_type_str.chars())
                                        .filter(|(a, b)| a != b)
                                        .count()
                                        <= 2
                                {
                                    warnings.push(ValidationWarning::PossibleTypo {
                                        type_name: element_type_str.clone(),
                                        similar_to: known_type.to_string(),
                                        location: format!(
                                            "array element type in template {}",
                                            template_name
                                        ),
                                    });
                                }
                            }
                        }
                    }
                }
                WAILCompositeType::Union(union) => {
                    // Validate each member type
                    for member_type in &union.members {
                        self.validate_type(
                            &member_type.field_type,
                            registry,
                            template_name,
                            warnings,
                            errors,
                            is_return_type,
                        );
                    }
                }
                WAILCompositeType::Object(object) => {
                    let type_name = object.type_data.type_name.to_string();
                    if type_name != "String"
                        && type_name != "Number"
                        && !registry.contains_key(&type_name)
                    {
                        // For return types and input parameters in templates, undefined types are errors
                        errors.push(ValidationError::UndefinedTypeInTemplate {
                            template_name: template_name.to_string(),
                            type_name: type_name.clone(),
                            is_return_type,
                        });

                        // Check for possible typos
                        for known_type in registry.keys() {
                            if known_type.len() > 2
                                && type_name.len() > 2
                                && known_type
                                    .chars()
                                    .zip(type_name.chars())
                                    .filter(|(a, b)| a != b)
                                    .count()
                                    <= 2
                            {
                                warnings.push(ValidationWarning::PossibleTypo {
                                    type_name: type_name.clone(),
                                    similar_to: known_type.to_string(),
                                    location: format!(
                                        "{} type in template {}",
                                        if is_return_type {
                                            "return"
                                        } else {
                                            "parameter"
                                        },
                                        template_name
                                    ),
                                });
                            }
                        }
                    }
                }
            },
            WAILType::Value(_) => (), // Literal values are always valid
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WAILDefinition<'a> {
    Object(WAILField<'a>),
    Template(WAILTemplateDef<'a>),
    Union(WAILField<'a>),
    Main(WAILMainDef<'a>),
    Comment(String),
    Import(WAILImport),
}

impl<'a> WAILDefinition<'a> {
    fn get_name(&self) -> Option<&str> {
        match self {
            WAILDefinition::Object(field) => Some(&field.name),
            WAILDefinition::Template(template) => Some(&template.name),
            WAILDefinition::Union(field) => Some(&field.name),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ValidationWarning {
    UndefinedType {
        type_name: String,
        location: String,
    },
    PossibleTypo {
        type_name: String,
        similar_to: String,
        location: String,
    },
    NoMainBlock,
}

#[derive(Debug, Clone)]
pub enum ValidationError {
    UndefinedTypeInTemplate {
        template_name: String,
        type_name: String,
        is_return_type: bool,
    },
    // SyntaxError {
    //     statement: String,
    //     error: String,
    // },
}

// Add test that tries parsing a basic object
#[cfg(test)]
mod tests {
    use std::hash::Hash;

    use super::*;

    #[test]
    fn test_parse_basic_object() {
        let input = r#"object Person {
            name: String
            age: Number
      }"#;

        let test_dir = std::env::current_dir().unwrap();
        let parser = WAILParser::new(test_dir);

        let (_, object_def) = parser.parse_object(input).unwrap();

        match object_def {
            WAILDefinition::Object(object) => {
                assert_eq!(
                    object
                        .field_type
                        .type_data()
                        .field_definitions
                        .as_ref()
                        .unwrap()
                        .len(),
                    3 // 2 for fields above and then implicity _type metadata field
                );
            }
            _ => panic!("Expected object definition"),
        }
    }

    #[test]
    fn test_parse_template() {
        // First create a parser
        let test_dir = std::env::current_dir().unwrap();
        let mut parser = WAILParser::new(test_dir);

        // Create and register the DateInfo type
        let date_info_fields = vec![
            WAILField {
                name: "day".to_string(),
                field_type: WAILType::Simple(WAILSimpleType::Number(WAILNumber::Integer(
                    WAILInteger {
                        value: 0,
                        type_data: WAILTypeData {
                            json_type: JsonValue::Number(Number::Integer(0)),
                            type_name: "Number",
                            field_definitions: None,
                            element_type: None,
                        },
                    },
                ))),
                annotations: vec![],
            },
            WAILField {
                name: "month".to_string(),
                field_type: WAILType::Simple(WAILSimpleType::String(WAILString {
                    value: String::new(),
                    type_data: WAILTypeData {
                        json_type: JsonValue::String(String::new()),
                        type_name: "String",
                        field_definitions: None,
                        element_type: None,
                    },
                })),
                annotations: vec![],
            },
        ];

        let date_info = WAILObject {
            value: HashMap::new(),
            type_data: WAILTypeData {
                json_type: JsonValue::Object(HashMap::new()),
                type_name: "DateInfo",
                field_definitions: Some(date_info_fields),
                element_type: None,
            },
        };

        let date_info_field = WAILField {
            name: "DateInfo".to_string(),
            field_type: WAILType::Composite(WAILCompositeType::Object(date_info)),
            annotations: vec![],
        };

        parser
            .registry
            .borrow_mut()
            .insert("DateInfo".to_string(), date_info_field);

        // Now parse the template
        let input = r#"template ParseDate(date_string: String) -> DateInfo {
            prompt: """
            Extract structured date information from the following date string.
            Date:
            ---
            {{date_string}}
            ---
            Return a structured result matching: {{return_type}}
            """
      }"#;

        let (_, template_def) = parser.parse_template(input).unwrap();

        match template_def {
            WAILDefinition::Template(template) => {
                assert_eq!(template.name, "ParseDate");
                assert_eq!(template.inputs.len(), 1);
                assert_eq!(template.inputs[0].name, "date_string");
                assert!(template.prompt_template.contains("{{date_string}}"));
                assert!(template.prompt_template.contains("{{return_type}}"));
                assert!(template.output.name == "DateInfo");
            }
            _ => panic!("Expected template definition"),
        }
    }

    #[test]
    fn test_parse_complex_template() {
        let input = r#"template AnalyzeBookClub(
      discussion_log: String,
      participant_names: String[],
      book_details: BookInfo
   ) -> BookClubAnalysis @description("Analyzes book club discussion patterns") {
      prompt: """
      Analyze the following book club discussion, tracking participation and key themes.

      Book Details:
      {{book_details}}

      Participants:
      {{participant_names}}

      Discussion:
      ---
      {{discussion_log}}
      ---

      Analyze the discussion and return a structured analysis following this format: {{return_type}}

      Focus on:
      - Speaking time per participant
      - Key themes discussed
      - Questions raised
      - Book-specific insights
      """
   }"#;

        let test_dir = std::env::current_dir().unwrap();
        let parser = WAILParser::new(test_dir);

        let (_, template_def) = parser.parse_template(input).unwrap();

        match template_def {
            WAILDefinition::Template(template) => {
                assert_eq!(template.name, "AnalyzeBookClub");
                assert_eq!(template.inputs.len(), 3);

                // Test input parameters
                let inputs = &template.inputs;
                assert_eq!(inputs[0].name, "discussion_log");
                assert!(matches!(inputs[0].field_type, WAILType::Simple(_)));

                assert_eq!(inputs[1].name, "participant_names");
                assert!(matches!(
                    inputs[1].field_type,
                    WAILType::Composite(WAILCompositeType::Array(_))
                ));

                assert_eq!(inputs[2].name, "book_details");
                assert!(matches!(
                    inputs[2].field_type,
                    WAILType::Composite(WAILCompositeType::Object(_))
                ));

                // Test output type
                assert_eq!(template.output.name, "BookClubAnalysis");

                // Test annotation
                assert_eq!(template.annotations.len(), 1);
                assert!(matches!(
                   template.annotations[0],
                   WAILAnnotation::Description(ref s) if s == "Analyzes book club discussion patterns"
                ));

                // Test template content
                let prompt = &template.prompt_template;
                assert!(prompt.contains("{{discussion_log}}"));
                assert!(prompt.contains("{{participant_names}}"));
                assert!(prompt.contains("{{book_details}}"));
                assert!(prompt.contains("{{return_type}}"));
            }
            _ => panic!("Expected template definition"),
        }
    }

    #[test]
    fn test_prompt_interpolation() {
        let test_dir = std::env::current_dir().unwrap();
        let parser = WAILParser::new(test_dir);

        // Define DateInfo type
        let fields = vec![
            WAILField {
                name: "day".to_string(),
                field_type: WAILType::Simple(WAILSimpleType::Number(WAILNumber::Integer(
                    WAILInteger {
                        value: 0,
                        type_data: WAILTypeData {
                            json_type: JsonValue::Number(Number::Integer(0)),
                            type_name: "Number",
                            field_definitions: None,
                            element_type: None,
                        },
                    },
                ))),
                annotations: vec![],
            },
            WAILField {
                name: "month".to_string(),
                field_type: WAILType::Simple(WAILSimpleType::String(WAILString {
                    value: String::new(),
                    type_data: WAILTypeData {
                        json_type: JsonValue::String(String::new()),
                        type_name: "String",
                        field_definitions: None,
                        element_type: None,
                    },
                })),
                annotations: vec![],
            },
        ];

        let date_info = WAILObject {
            value: HashMap::new(),
            type_data: WAILTypeData {
                json_type: JsonValue::Object(HashMap::new()),
                type_name: "DateInfo",
                field_definitions: Some(fields),
                element_type: None,
            },
        };

        let field = WAILField {
            name: "DateInfo".to_string(),
            field_type: WAILType::Composite(WAILCompositeType::Object(date_info)),
            annotations: vec![],
        };

        parser
            .registry
            .borrow_mut()
            .insert("DateInfo".to_string(), field.clone());

        let template = WAILTemplateDef {
            name: "ParseDate".to_string(),
            inputs: vec![WAILField {
                name: "date_string".to_string(),
                field_type: WAILType::Simple(WAILSimpleType::String(WAILString {
                    value: String::new(),
                    type_data: WAILTypeData {
                        json_type: JsonValue::String(String::new()),
                        type_name: "String",
                        field_definitions: None,
                        element_type: None,
                    },
                })),
                annotations: vec![],
            }],
            output: field,
            prompt_template: r#"Parse this date: {{date_string}}
Return in this format: {{return_type}}"#
                .to_string(),
            annotations: vec![],
        };

        let mut inputs = HashMap::new();
        inputs.insert(
            "date_string".to_string(),
            TemplateArgument::String("January 1st, 2024".to_string()),
        );

        let final_prompt = template
            .interpolate_prompt(Some(&inputs), &parser.object_instances.borrow().clone())
            .unwrap();
        println!("Final prompt:\n{}", final_prompt);
    }

    #[test]
    fn test_wail_parsing() {
        let test_dir = std::env::current_dir().unwrap();
        let parser = WAILParser::new(test_dir);

        let input = r#"
      object Person {
            name: String
            age: Number
      }

      template GetPersonFromDescription(description: String) -> Person {
            prompt: """
            Given this description of a person: {{description}}
            Create a Person object with their name and age.
            Return in this format: {{return_type}}
            """
      }

      main {
            let person1_template = GetPersonFromDescription(description: "John Doe is 30 years old");
            let person2_template = GetPersonFromDescription(description: "Jane Smith is 25 years old");

            prompt  {
               Here is the first person you need to create: {{person1_template}}
               And here is the second person you need to create: {{person2_template}}
            }
      }
      "#;

        let definitions = parser
            .parse_wail_file(input.to_string(), WAILFileType::Application, true)
            .unwrap();
        assert_eq!(
            definitions.len(),
            3,
            "Should parse object, template and main"
        );

        // Verify Person object
        match &definitions[0] {
            WAILDefinition::Object(obj) => {
                assert_eq!(obj.name, "Person");
                if let WAILType::Composite(WAILCompositeType::Object(obj)) = &obj.field_type {
                    let fields = obj.type_data.field_definitions.as_ref().unwrap();
                    assert_eq!(fields.len(), 3); // 3 because objects have added _type fields
                    assert_eq!(fields[0].name, "name");
                    assert_eq!(fields[1].name, "age");
                } else {
                    panic!("Expected Person to be an object type");
                }
            }
            _ => panic!("First definition should be an Object"),
        }

        // Verify GetPersonFromDescription template
        let _template = match &definitions[1] {
            WAILDefinition::Template(template) => {
                assert_eq!(template.name, "GetPersonFromDescription");
                assert_eq!(template.inputs.len(), 1);
                assert_eq!(template.inputs[0].name, "description");
                assert!(template.prompt_template.contains("{{description}}"));
                assert!(template.prompt_template.contains("{{return_type}}"));
                template
            }
            _ => panic!("Second definition should be a Template"),
        };

        // Verify main block
        match &definitions[2] {
            WAILDefinition::Main(main) => {
                assert_eq!(main.statements.len(), 2);

                // Check first assignment
                let (var1, call1) = main.statements[0].as_assignment().unwrap();
                assert_eq!(var1, "person1_template");
                assert_eq!(call1.template_name, "GetPersonFromDescription");
                assert_eq!(call1.arguments.len(), 1);

                // Check second assignment
                let (var2, call2) = main.statements[1].as_assignment().unwrap();
                assert_eq!(var2, "person2_template");
                assert_eq!(call2.template_name, "GetPersonFromDescription");
                assert_eq!(call2.arguments.len(), 1);

                // Test prompt interpolation
                let template_registry = parser.template_registry.borrow().clone();
                let objects = parser.object_instances.borrow().clone();
                let interpolated = main
                    .interpolate_prompt(&template_registry, &objects, None)
                    .unwrap();

                println!("Interpolated prompt:\n{}", interpolated);
                assert!(interpolated.contains("Given this description of a person:"));
                assert!(interpolated.contains("Create a Person object with their name and age."));
                assert!(interpolated.contains("Here is the first person you need to create:"));
                assert!(interpolated.contains("And here is the second person you need to create:"));
            }
            _ => panic!("Third definition should be Main"),
        }
    }

    #[test]
    fn test_circular_imports() {
        use std::fs;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();

        // Create a.lib.wail that imports from b
        fs::write(
            tmp.path().join("a.lib.wail"),
            r#"import { B } from "b.lib.wail"
        object A { field: String }"#,
        )
        .unwrap();

        // Create b.lib.wail that imports from a
        fs::write(
            tmp.path().join("b.lib.wail"),
            r#"import { A } from "a.lib.wail"
        object B { field: String }"#,
        )
        .unwrap();

        let parser = WAILParser::new(tmp.path().to_path_buf());

        // Try to parse a file that starts the circular chain
        let input = r#"import { A } from "a.lib.wail"
    main { prompt { } }"#;

        let result = parser.parse_wail_file(input.to_string(), WAILFileType::Application, true);
        println!("{:?}", result);
        assert!(matches!(result, Err(WAILParseError::CircularImport { .. })));
    }

    #[test]
    fn test_union_types() {
        let input = r#"
   object ErrorResult {
      error: String
      code: Number
   }

   object SuccessResult {
      data: String
   }

   union ApiResponse = ErrorResult | SuccessResult | String;

   template TestNamedUnionArray(test: String) -> ApiResponse[] {
      prompt: """
      Process this test case: {{test}}
      {{return_type}}
      """
   }

   template TestNamedUnion(test: String) -> ApiResponse {
      prompt: """
      Process this test case: {{test}}
      {{return_type}}
      """
   }

   template TestInlineUnion(test: String) -> ErrorResult | String {
      prompt: """
      Process this inline test: {{test}}
      {{return_type}}
      """
   }

   main {
      let named_test = TestNamedUnion(test: "test case 1");
      let named_test_array = TestNamedUnionArray(test: "test case 1");
      let inline_test = TestInlineUnion(test: "test case 2");

      prompt {
            Named union result: {{named_test}}
            Named union array result: {{named_test_array}}
            Inline union result: {{inline_test}}
      }
   }
   "#;

        let test_dir = std::env::current_dir().unwrap();
        let parser = WAILParser::new(test_dir);
        let result = parser.parse_wail_file(input.to_string(), WAILFileType::Application, true);
        assert!(result.is_ok());

        let prompt = parser.prepare_prompt(None);
        println!("Generated prompt:\n{}", prompt);

        // Verify the schema formatting for both types of unions
        assert!(prompt.contains("Any of these JSON-like formats:"));
        assert!(prompt.contains("Format 1:"));
        assert!(prompt.contains("Format 2:"));
        assert!(prompt.contains("ErrorResult"));
        assert!(prompt.contains("SuccessResult"));
        assert!(prompt.contains("string"));
        assert!(prompt.contains("-- OR --"));

        // Verify validation passes
        let (warnings, errors) = parser.validate();
        assert!(
            errors.is_empty(),
            "Unexpected validation errors: {:?}",
            errors
        );
    }

    #[test]
    fn test_validation() {
        let test_dir = std::env::current_dir().unwrap();
        let parser = WAILParser::new(test_dir);

        // First parse a template with undefined types
        let input = r#"template ProcessData(
            raw_data: DataInput,
            config: ProcessConfig[]
      ) -> DataOutput {
            prompt: """
            Process the data according to the configuration.
            Input: {{raw_data}}
            Config: {{config}}
            Output format: {{return_type}}
            """
      }"#;

        let (_, _) = parser.parse_template(input).unwrap();

        // Now validate - should get errors for undefined types and warning for no main block
        let (warnings, errors) = parser.validate();

        // Should have errors for DataInput, ProcessConfig, and DataOutput
        assert_eq!(errors.len(), 3);
        let error_types: Vec<_> = errors
            .iter()
            .map(|e| match e {
                ValidationError::UndefinedTypeInTemplate { type_name, .. } => type_name.as_str(),
            })
            .collect();
        assert!(error_types.contains(&"DataInput"));
        assert!(error_types.contains(&"DataOutput"));
        assert!(error_types.contains(&"ProcessConfig"));

        // Should have warning for no main block
        assert!(warnings
            .iter()
            .any(|w| matches!(w, ValidationWarning::NoMainBlock)));

        // Now define one of the types with a similar name to test typo detection
        let type_def = r#"object DataInputs {
            field1: String
            field2: Number
      }"#;
        let (_, _) = parser.parse_object(type_def).unwrap();

        // Validate again - should now get a typo warning for DataInput vs DataInputs
        let (warnings, errors) = parser.validate();
        assert!(warnings.iter().any(|w| matches!(w,
              ValidationWarning::PossibleTypo {
                 type_name,
                 similar_to,
                 ..
              } if type_name == "DataInput" && similar_to == "DataInputs"
        )));
    }

    #[test]
    fn test_template_args_interpolation() {
        let input = r#"
   object Person {
      name: String
      age: Number
   }

   template CreatePerson(info: String) -> Person {
      prompt: """
      Given this info: $info
      Create a person with the provided info.
      """
   }

   main {
      template_args {
            str_arg: String,
            num_arg: Number,
            bool_arg: Boolean,
            arr_arg: String[],
            obj_arg: Person,
            null_arg: String
      }

      let person = CreatePerson(info: "test");

      prompt {
            String arg: $str_arg
            Number arg: $num_arg
            Boolean arg: $bool_arg
            Array arg: $arr_arg
            Object arg: $obj_arg
            Null arg: $null_arg
      }
   }
   "#;

        let test_dir = std::env::current_dir().unwrap();
        let parser = WAILParser::new(test_dir);
        parser
            .parse_wail_file(input.to_string(), WAILFileType::Application, true)
            .unwrap();

        // Create test template args
        let mut obj = HashMap::new();
        obj.insert("name".to_string(), JsonValue::String("John".to_string()));
        obj.insert("age".to_string(), JsonValue::Number(Number::Integer(30)));

        let mut template_args = HashMap::new();
        template_args.insert(
            "str_arg".to_string(),
            JsonValue::String("hello".to_string()),
        );
        template_args.insert(
            "num_arg".to_string(),
            JsonValue::Number(Number::Integer(42)),
        );
        template_args.insert("bool_arg".to_string(), JsonValue::Boolean(true));
        template_args.insert(
            "arr_arg".to_string(),
            JsonValue::Array(vec![
                JsonValue::String("one".to_string()),
                JsonValue::String("two".to_string()),
            ]),
        );
        template_args.insert("obj_arg".to_string(), JsonValue::Object(obj));
        template_args.insert("null_arg".to_string(), JsonValue::Null);

        let prompt = parser.prepare_prompt(Some(&template_args));
        let result = prompt;

        println!("Result: {}", result);

        // Verify each type of argument was interpolated correctly
        assert!(result.contains("String arg: hello"));
        assert!(result.contains("Number arg: 42"));
        assert!(result.contains("Boolean arg: true"));
        assert!(result.contains("Array arg: [\"one\", \"two\"]"));

        assert!(
            result.contains("Object arg: {\"name\": \"John\", \"age\": 30}")
                || result.contains("Object arg: {\"age\": 30, \"name\": \"John\"}")
        );
        assert!(result.contains("Null arg: null"));
    }

    #[test]
    fn test_json_segment_parsing() {
        let schema = r#"
   template Test() -> String {
      prompt: """Test"""
   }
   main {
      let result = Test();
      prompt { {{result}} }
   }"#;

        let test_dir = std::env::current_dir().unwrap();
        let parser = WAILParser::new(test_dir);
        parser
            .parse_wail_file(schema.to_string(), WAILFileType::Application, true)
            .unwrap();

        // Test gasp fence parsing
        let gasp_fence = r#"
   Some text before
   <result>
   "hello"
   </result>
   Some text after
   "#;

        let res = parser.parse_llm_output(gasp_fence);
        println!("{:?}", res);
        assert!(res.is_ok());
        let schema = r#"
   template Test() -> String {
      prompt: """Test"""
   }
   main {
      let result = Test();
      let result2 = Test();
      prompt { {{result}} {{result2}} }
   }"#;

        let test_dir = std::env::current_dir().unwrap();
        let parser = WAILParser::new(test_dir);
        parser
            .parse_wail_file(schema.to_string(), WAILFileType::Application, true)
            .unwrap();
        // Test multiple gasp fences
        let multiple_fences = r#"
         First result:
         <result>
         "hello"
         </result>
         Second result:
         <result>
         "world"
         </result>
         "#;

        let res = parser.parse_llm_output(multiple_fences);
        println!("{:?}", res);
        assert!(res.is_ok());

        // Test mixed traditional and fence
        let mixed = r#"
         Fence:
         <result>
         "world"
         </result>
         <result>
         "world"
         </result>
         dd
         "#;
        let res = parser.parse_llm_output(mixed);
        println!("{:?}", res);
        assert!(res.is_ok());

        // Test different types in fences
        let types_schema = r#"
         template Test() -> Number {
               prompt: """Test"""
         }
         main {
               let result = Test();
               prompt { {{result}} }
         }"#;

        parser
            .parse_wail_file(types_schema.to_string(), WAILFileType::Application, true)
            .unwrap();

        let number_fence = r#"
        <result>
         43
        </result>
         "#;

        let res = parser.parse_llm_output(number_fence);
        println!("{:?}", res);
        assert!(res.is_ok());

        let types_schema = r#"
         template Test() -> Number[] {
               prompt: """Test"""
         }
         main {
               let result = Test();
               prompt { {{result}} }
         }"#;

        parser
            .parse_wail_file(types_schema.to_string(), WAILFileType::Application, true)
            .unwrap();

        let array_fence = r#"
         <result>
         [1, 2, 3]
         </result>
         "#;
        let res = parser.parse_llm_output(array_fence);
        println!("{:?}", res);
        assert!(res.is_ok());

        let result = parser.validate_json(&res.unwrap().to_string());
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_object_as_argument_to_func() {
        let test_dir = std::env::current_dir().unwrap();
        let parser = WAILParser::new(test_dir);
        let input = r#"
        object Article {
            content: String
            url: String
        }

        object ArticleMetadata {
            authors: String[]
            headline: String
            publishDate: String
            categories: String[]
            keywords: String[]
            summary: String
            sentiment: String
        }

        object ProcessedArticle {
            article: Article
            metadata: ArticleMetadata
        }

        template ExtractInformation(article: Article) -> ProcessedArticle {
            prompt: """
            You are an AI assistant specialized in analyzing news articles.
            Please extract and structure the following information from this article:

            Article Information:
            URL: {{article.url}}

            Content to analyze:
            {{article.content}}

            Extract key information including:
            1. Main categories/topics
            2. Important keywords
            3. Brief summary (2-3 sentences)
            4. Overall sentiment (positive/negative/neutral)

            Provide structured output following the ArticleMetadata format.
            {{return_type}}
            """
        }

        main {
            template_args {
                url: String,
                content: String
            }

            let article = Article(
                content: $content,
                url: $url
            );

            let res = ExtractInformation(article: article);

            prompt {
                Process the following news article and extract structured information:
                {{res}}
            }
        }
        "#;
        let result = parser.parse_wail_file(input.to_string(), WAILFileType::Application, true);
        assert!(
            result.is_ok(),
            "Failed to parse ideal.wail: {:?}",
            result.err()
        );

        let template_args = HashMap::from([
            (
                "content".to_string(),
                JsonValue::String("I am an article".to_string()),
            ),
            (
                "url".to_string(),
                JsonValue::String("www.example.com".to_string()),
            ),
        ]);

        let prompt = parser.prepare_prompt(Some(&template_args));

        println!("Prompt:\n{}", prompt);
    }

    #[test]
    fn test_parse_ideal_wail() {
        let test_dir = std::env::current_dir().unwrap();
        let parser = WAILParser::new(test_dir);
        let input = r#"object RealtimeEvent {
   type: String  # response.create, response.chunk, response.end
   response: {
      modalities: String[]  # ["text", "speech"]
      instructions: String
      voice: String
   }
}

object Message {
   content: String
   role: String  # user or assistant
}

object Conversation {
   messages: Message[]
   model: String
   temperature: Number
   max_tokens: Number
}

template GeneratePrompt(conversation: Conversation) -> RealtimeEvent {
   prompt: """
   You are a helpful AI assistant engaging in a conversation with the user.
   Previous messages:
   {{#each conversation.messages}}
   {{this.role}}: {{this.content}}
   {{/each}}

   Respond naturally and conversationally. Your response will be delivered through both text and speech.
   Use the voice specified in the response object.

   {{return_type}}
   """
}

main {
   template_args {
      messages: Message[],
      model: String,
      temperature: Number,
      max_tokens: Number
   }

   let conversation = Conversation(
      messages: $messages,
      model: $model,
      temperature: $temperature,
      max_tokens: $max_tokens
   );

   let prompt = GeneratePrompt(conversation: conversation);

   prompt {
      Create a response that will be delivered through both text and speech:
      {{prompt}}
   }
}"#;

        let result = parser.parse_wail_file(input.to_string(), WAILFileType::Application, true);
        assert!(
            result.is_ok(),
            "Failed to parse ideal.wail: {:?}",
            result.err()
        );

        let definitions = result.unwrap();

        // Verify RealtimeEvent object
        let realtime_event = match &definitions[0] {
            WAILDefinition::Object(obj) => obj,
            _ => panic!("First definition should be RealtimeEvent object"),
        };
        assert_eq!(realtime_event.name, "RealtimeEvent");
        if let WAILType::Composite(WAILCompositeType::Object(obj)) = &realtime_event.field_type {
            let fields = obj.type_data.field_definitions.as_ref().unwrap();
            assert_eq!(fields.len(), 3); // type and response fields + _type metadata
            assert_eq!(fields[0].name, "type");
            assert_eq!(fields[1].name, "response");
        } else {
            panic!("RealtimeEvent should be an object type");
        }

        // Verify Message object
        let message = match &definitions[1] {
            WAILDefinition::Object(obj) => obj,
            _ => panic!("Second definition should be Message object"),
        };
        assert_eq!(message.name, "Message");
        if let WAILType::Composite(WAILCompositeType::Object(obj)) = &message.field_type {
            let fields = obj.type_data.field_definitions.as_ref().unwrap();
            assert_eq!(fields.len(), 3); // content and role fields + implicit _type metadata
            assert_eq!(fields[0].name, "content");
            assert_eq!(fields[1].name, "role");
        } else {
            panic!("Message should be an object type");
        }

        // Verify Conversation object
        let conversation = match &definitions[2] {
            WAILDefinition::Object(obj) => obj,
            _ => panic!("Third definition should be Conversation object"),
        };
        assert_eq!(conversation.name, "Conversation");
        if let WAILType::Composite(WAILCompositeType::Object(obj)) = &conversation.field_type {
            let fields = obj.type_data.field_definitions.as_ref().unwrap();
            assert_eq!(fields.len(), 5); // messages, model, temperature, max_tokens + implicit _type field
            assert_eq!(fields[0].name, "messages");
            assert_eq!(fields[1].name, "model");
            assert_eq!(fields[2].name, "temperature");
            assert_eq!(fields[3].name, "max_tokens");
        } else {
            panic!("Conversation should be an object type");
        }

        // Verify GeneratePrompt template
        let template = match &definitions[3] {
            WAILDefinition::Template(template) => template,
            _ => panic!("Fourth definition should be GeneratePrompt template"),
        };
        assert_eq!(template.name, "GeneratePrompt");
        assert_eq!(template.inputs.len(), 1); // conversation parameter
        assert_eq!(template.inputs[0].name, "conversation");
        assert!(template
            .prompt_template
            .contains("{{#each conversation.messages}}"));
        assert!(template
            .prompt_template
            .contains("{{this.role}}: {{this.content}}"));
        assert!(template.prompt_template.contains("{{return_type}}"));

        // Verify main section
        let main = match &definitions[4] {
            WAILDefinition::Main(main) => main,
            _ => panic!("Fifth definition should be Main section"),
        };

        // Verify template args
        assert_eq!(main.template_args.len(), 4); // messages, model, temperature, max_tokens
        assert!(main.template_args.contains_key("messages"));
        assert!(main.template_args.contains_key("model"));
        assert!(main.template_args.contains_key("temperature"));
        assert!(main.template_args.contains_key("max_tokens"));

        // Verify statements
        assert_eq!(main.statements.len(), 2); // conversation and prompt assignments

        let (var1, call1, args) = main.statements[0].as_object_instantiation().unwrap();
        assert_eq!(var1, "conversation");
        assert_eq!(call1, "Conversation");
        println!("{:?}", args);

        let (var2, call2) = main.statements[1].as_assignment().unwrap();
        assert_eq!(var2, "prompt");
        assert_eq!(call2.template_name, "GeneratePrompt");
        assert_eq!(call2.arguments.len(), 1);
    }

    #[test]
    fn test_parse_imports() {
        let input = r#"
    import { Person, Address } from "types.lib.wail"
    import { GetPerson } from "templates.lib.wail"

    object Config {
        name: String
    }

    main {
        prompt { }
    }
    "#;

        use std::fs;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();

        // Create a.lib.wail that imports from b
        fs::write(
            tmp.path().join("types.lib.wail"),
            r#"
            object Person {
                name: String
            }
            object Address {
                street: String
            }
        "#,
        )
        .unwrap();

        fs::write(
            tmp.path().join("templates.lib.wail"),
            r#"
            template GetPerson(name: String) -> String {
                prompt: """
                {{return_type}}
                """
            }"#,
        )
        .unwrap();

        let parser = WAILParser::new(tmp.path().to_path_buf());
        let definitions = parser
            .parse_wail_file(input.to_string(), WAILFileType::Application, true)
            .unwrap();

        // First two definitions should be imports
        match &definitions[0] {
            WAILDefinition::Import(import) => {
                assert_eq!(import.items, vec!["Person", "Address"]);
                assert_eq!(import.path, "types.lib.wail");
            }
            _ => panic!("Expected first definition to be import"),
        }

        match &definitions[1] {
            WAILDefinition::Import(import) => {
                assert_eq!(import.items, vec!["GetPerson"]);
                assert_eq!(import.path, "templates.lib.wail");
            }
            _ => panic!("Expected second definition to be import"),
        }
    }

    #[test]
    fn test_wail_errors() {
        let test_dir = std::env::current_dir().unwrap();
        let parser = WAILParser::new(test_dir);

        // Test duplicate object definition
        let input = r#"
      object Person {
            name: String
      }
      object Person {
            age: Number
      }
      main {
            prompt { }
      }
      "#;

        let err = parser
            .parse_wail_file(input.to_string(), WAILFileType::Application, true)
            .unwrap_err();
        assert!(
            matches!(err, WAILParseError::DuplicateDefinition { name, .. } if name == "Person")
        );

        // Test missing main block
        let input = r#"
      object Person {
            name: String
      }
      "#;

        let err = parser
            .parse_wail_file(input.to_string(), WAILFileType::Application, true)
            .unwrap_err();
        assert!(matches!(err, WAILParseError::MissingMainBlock));

        // Test unexpected token
        let input = r#"
      object Person {
            name: String
            age: @ Number
      }
      main {
            prompt { }
      }
      "#;

        let err = parser
            .parse_wail_file(input.to_string(), WAILFileType::Application, true)
            .unwrap_err();

        assert!(
            matches!(err, WAILParseError::UnexpectedToken { found, .. } if found == "age: @ Number")
        );

        //     // Test EOF
        //     let input = r#"
        //     object Person {
        //         name: String
        // "#;

        //     let err = parser.parse_wail_file(input).unwrap_err();
        //     println!("{:?}", err);
        //     assert!(matches!(err, WAILParseError::UnexpectedEOF { .. }));
    }
}
