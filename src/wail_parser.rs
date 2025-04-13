use crate::json_types::{JsonValue, Number};
use crate::parser_types::*;
use crate::rd_json_stack_parser::Parser as JsonParser;
use crate::types::*;
use nom::{
    bytes::complete::{tag, take_until},
    character::complete::multispace0,
    multi::many0,
    sequence::delimited,
    IResult,
};

use nom_supreme::final_parser::Location;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum WAILParseError {
    // Syntax errors
    UnexpectedToken {
        found: String,
        location: Location,
    },
    UnexpectedKeyword {
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

    SymbolNotFound {
        name: String,
    },

    AmbiguousSymbol {
        name: String,
        matches: Vec<String>,
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
    incremental_parser: RefCell<Option<Parser<'a>>>,
    current_module: RefCell<Vec<String>>,
}

#[derive(Debug)]
pub struct ImportChain {
    // Stack of current import resolution
    stack: Vec<String>,
    // Set of all visited/imported files
    visited: HashSet<String>,
    // Base directory for resolving relative paths
    base_path: PathBuf,
}

impl ImportChain {
    pub fn new(base_path: PathBuf) -> Self {
        Self {
            stack: Vec::new(),
            visited: HashSet::new(),
            base_path,
        }
    }

    pub fn push(&mut self, path: &str) -> Result<bool, WAILParseError> {
        let canonical_path = self.resolve_path(path)?;

        if self.stack.contains(&canonical_path) {
            return Err(WAILParseError::CircularImport {
                path: canonical_path.clone(),
                chain: self.stack.clone(),
            });
        }

        let is_new = self.visited.insert(canonical_path.clone());
        self.stack.push(canonical_path);

        Ok(is_new) // true = first time seen, false = already visited
    }

    pub fn pop(&mut self) {
        self.stack.pop();
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

#[derive(Debug, PartialEq)]
pub enum WAILFileType {
    Library,     // .lib.wail - only definitions allowed
    Application, // .wail - requires main block
}

#[derive(Debug, Clone, PartialEq)]
pub struct WAILImport {
    pub items: Vec<String>,
    pub path: String,
}

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Token<'a> {
    Identifier(&'a str),
    String(&'a str),
    Number(i64),
    Float(f64),
    OpenBrace,
    CloseBrace,
    OpenParen,
    CloseParen,
    OpenBracket,
    CloseBracket,
    Colon,
    Semicolon,
    Comma,
    Equals,
    Arrow,
    Pipe,
    At,
    Dollar,
    Keyword(&'a str), // object, template, union, main, let, prompt, etc.
    Hash,
    TripleQuoteStart,
    TripleQuoteEnd,
    Whitespace(&'a str),
    Newline,
    TripleQuoteContent(&'a str),
    Comment(&'a str),
    Eof,
}

impl<'a> fmt::Display for Token<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Token::Identifier(s) => write!(f, "{}", s),
            Token::String(s) => write!(f, "\"{}\"", s),
            Token::Number(n) => write!(f, "{}", n),
            Token::Float(n) => write!(f, "{}", n),
            Token::OpenBrace => write!(f, "{{"),
            Token::CloseBrace => write!(f, "}}"),
            Token::OpenParen => write!(f, "("),
            Token::CloseParen => write!(f, ")"),
            Token::OpenBracket => write!(f, "["),
            Token::CloseBracket => write!(f, "]"),
            Token::Colon => write!(f, ":"),
            Token::Semicolon => write!(f, ";"),
            Token::Comma => write!(f, ","),
            Token::Equals => write!(f, "="),
            Token::Arrow => write!(f, "->"),
            Token::Pipe => write!(f, "|"),
            Token::At => write!(f, "@"),
            Token::Hash => write!(f, "#"),
            Token::Dollar => write!(f, "$"),
            Token::Keyword(s) => write!(f, "{}", s),
            Token::TripleQuoteStart => write!(f, "\"\"\""),
            Token::TripleQuoteEnd => write!(f, "\"\"\""),
            Token::TripleQuoteContent(s) => write!(f, "{}", s),
            Token::Whitespace(s) => write!(f, "{}", s),
            Token::Comment(s) => write!(f, "#{}", s),
            Token::Newline => write!(f, "\n"),
            Token::Eof => write!(f, "EOF"),
        }
    }
}

#[derive(Debug)]
pub struct Tokenizer<'a> {
    input: &'a str,
    position: usize,
    line: usize,
    column: usize,
    last_position: usize,
    last_line: usize,
    last_column: usize,
}

impl<'a> Tokenizer<'a> {
    pub fn new(input: &'a str) -> Self {
        Tokenizer {
            input,
            position: 0,
            line: 1,
            column: 1,
            last_position: 0,
            last_line: 1,
            last_column: 1,
        }
    }

    pub fn current_location(&self) -> Location {
        Location {
            line: self.line,
            column: self.column,
        }
    }

    pub fn last_location(&self) -> Location {
        Location {
            line: self.last_line,
            column: self.last_column,
        }
    }

    fn save_position(&mut self) {
        self.last_position = self.position;
        self.last_line = self.line;
        self.last_column = self.column;
    }

    fn advance(&mut self) -> Option<char> {
        if self.position >= self.input.len() {
            return None;
        }

        let c = self.input[self.position..].chars().next().unwrap();
        self.position += c.len_utf8();

        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }

        Some(c)
    }

    fn peek(&self) -> Option<char> {
        if self.position >= self.input.len() {
            return None;
        }
        self.input[self.position..].chars().next()
    }

    fn peek_n(&self, n: usize) -> Option<&'a str> {
        if self.position + n <= self.input.len() {
            Some(&self.input[self.position..self.position + n])
        } else {
            None
        }
    }

    fn read_triple_quoted_string(&mut self) -> Token<'a> {
        // Skip the opening """
        self.advance();
        self.advance();
        self.advance();

        let start_pos = self.position;
        let mut found_end = false;

        while let Some(c) = self.peek() {
            if c == '"' && self.peek_n(3) == Some("\"\"\"") {
                found_end = true;
                break;
            }
            self.advance();
        }

        let content = &self.input[start_pos..self.position];

        if found_end {
            // Skip the closing """
            self.advance();
            self.advance();
            self.advance();
            Token::TripleQuoteContent(content)
        } else {
            // Unclosed triple quote - treat what we have as content
            Token::TripleQuoteContent(content)
        }
    }

    fn read_string(&mut self) -> Token<'a> {
        // Skip the opening quote
        self.advance();

        let start_pos = self.position;
        let mut found_end = false;

        while let Some(c) = self.peek() {
            if c == '"' {
                found_end = true;
                break;
            } else if c == '\\' {
                // Skip escape character and the escaped character
                self.advance();
                if self.peek().is_some() {
                    self.advance();
                }
            } else {
                self.advance();
            }
        }

        let content = &self.input[start_pos..self.position];

        if found_end {
            // Skip the closing quote
            self.advance();
            Token::String(content)
        } else {
            // Unclosed string - treat what we have as content
            Token::String(content)
        }
    }

    fn read_number(&mut self) -> Token<'a> {
        let start_pos = self.position;
        let mut is_float = false;

        // Handle negative sign
        if self.peek() == Some('-') {
            self.advance();
        }

        // Read integer part
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.advance();
            } else if c == '.' {
                is_float = true;
                self.advance();
                break;
            } else {
                break;
            }
        }

        // Read decimal part if this is a float
        if is_float {
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    self.advance();
                } else {
                    break;
                }
            }
        }

        let number_str = &self.input[start_pos..self.position];

        if is_float {
            match number_str.parse::<f64>() {
                Ok(n) => Token::Float(n),
                Err(_) => {
                    // Handle parsing error - return 0.0 as a fallback
                    Token::Float(0.0)
                }
            }
        } else {
            match number_str.parse::<i64>() {
                Ok(n) => Token::Number(n),
                Err(_) => {
                    // Handle parsing error - return 0 as a fallback
                    Token::Number(0)
                }
            }
        }
    }

    fn read_identifier_or_keyword(&mut self) -> Token<'a> {
        let start_pos = self.position;

        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }

        let text = &self.input[start_pos..self.position];

        match text {
            "object" | "template" | "union" | "main" | "let" | "prompt" | "import" | "from"
            | "template_args" => Token::Keyword(text),
            _ => Token::Identifier(text),
        }
    }

    fn read_comment(&mut self) -> Token<'a> {
        // Skip the #
        self.advance();

        let start_pos = self.position;

        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.advance();
        }

        Token::Comment(&self.input[start_pos..self.position])
    }

    fn read_arrow(&mut self) -> Token<'a> {
        // Skip the -
        self.advance();

        // Skip the >
        if self.peek() == Some('>') {
            self.advance();
            Token::Arrow
        } else {
            // Not an arrow, just a minus sign (which we'll handle as part of a number)
            // Go back to the minus position
            self.position -= 1;
            self.column -= 1;
            self.read_number()
        }
    }
    pub fn next_token(&mut self) -> Token<'a> {
        self.save_position();
        if self.position >= self.input.len() {
            return Token::Eof;
        }
        // Check for whitespace
        let c = self.peek().unwrap();

        if c.is_whitespace() {
            let start_pos = self.position;

            // Consume whitespace
            while let Some(c) = self.peek() {
                if c.is_whitespace() {
                    self.advance();
                } else {
                    break;
                }
            }

            // Return whitespace as a token
            return Token::Whitespace(&self.input[start_pos..self.position]);
        }

        // Check for triple-quoted string
        if self.peek_n(3) == Some("\"\"\"") {
            return self.read_triple_quoted_string();
        }

        match self.peek().unwrap() {
            '{' => {
                self.advance();
                Token::OpenBrace
            }
            '}' => {
                self.advance();
                Token::CloseBrace
            }
            '(' => {
                self.advance();
                Token::OpenParen
            }
            ')' => {
                self.advance();
                Token::CloseParen
            }
            '[' => {
                self.advance();
                Token::OpenBracket
            }
            ']' => {
                self.advance();
                Token::CloseBracket
            }
            ':' => {
                self.advance();
                Token::Colon
            }
            ';' => {
                self.advance();
                Token::Semicolon
            }
            ',' => {
                self.advance();
                Token::Comma
            }
            '=' => {
                self.advance();
                Token::Equals
            }
            '|' => {
                self.advance();
                Token::Pipe
            }
            '@' => {
                self.advance();
                Token::At
            }
            '#' => self.read_comment(),
            '$' => {
                self.advance();
                Token::Dollar
            }
            '"' => self.read_string(),
            '-' => self.read_arrow(),
            c if c.is_ascii_digit() => self.read_number(),
            c if c.is_alphabetic() || c == '_' => self.read_identifier_or_keyword(),
            _ => {
                // Skip unrecognized character
                self.advance();
                self.next_token()
            }
        }
    }
}

#[derive(Debug)]
pub struct Parser<'a> {
    tokenizer: Tokenizer<'a>,
    current_token: Token<'a>,
    peek_token: Token<'a>,
    registry: &'a RefCell<HashMap<String, WAILField<'a>>>,
    template_registry: &'a RefCell<HashMap<String, WAILTemplateDef<'a>>>,
    adhoc_obj_ref_id_counter: &'a RefCell<i64>,
    adhoc_obj_ids: &'a RefCell<Vec<String>>,
    adhoc_obj_refs: &'a RefCell<HashMap<String, WAILObject<'a>>>,
    object_instances: &'a RefCell<HashMap<String, WAILObjectInstantiation>>,
    main: &'a RefCell<Option<WAILMainDef<'a>>>,
    import_chain: &'a RefCell<ImportChain>,
    in_prompt_block: RefCell<bool>,
    current_module: &'a RefCell<Vec<String>>,
}

impl<'a> Parser<'a> {
    pub fn new(
        input: &'a str,
        registry: &'a RefCell<HashMap<String, WAILField<'a>>>,
        template_registry: &'a RefCell<HashMap<String, WAILTemplateDef<'a>>>,
        adhoc_obj_ref_id_counter: &'a RefCell<i64>,
        adhoc_obj_ids: &'a RefCell<Vec<String>>,
        adhoc_obj_refs: &'a RefCell<HashMap<String, WAILObject<'a>>>,
        object_instances: &'a RefCell<HashMap<String, WAILObjectInstantiation>>,
        main: &'a RefCell<Option<WAILMainDef<'a>>>,
        import_chain: &'a RefCell<ImportChain>,
        current_module: &'a RefCell<Vec<String>>,
    ) -> Self {
        let mut tokenizer = Tokenizer::new(input);
        let current_token = tokenizer.next_token();
        let peek_token = tokenizer.next_token();

        Parser {
            tokenizer,
            current_token,
            peek_token,
            registry,
            template_registry,
            adhoc_obj_ref_id_counter,
            adhoc_obj_ids,
            adhoc_obj_refs,
            object_instances,
            main,
            import_chain,
            in_prompt_block: RefCell::new(false),
            current_module: current_module,
        }
    }

    // Update parse_object to better handle fields
    fn parse_object(&mut self) -> Result<WAILDefinition<'a>, WAILParseError> {
        // Expect "object" keyword
        self.expect_keyword("object")?;
        // Parse object name
        let name = self.expect_identifier()?;

        let namespaced_id = self.namespaced_identifier(name.to_string());

        {
            if self.registry.borrow().contains_key(&namespaced_id) {
                return Ok(WAILDefinition::Object(
                    self.lookup_symbol_in_registry(&name)?.clone(),
                ));
            }
        }

        // Expect opening brace
        self.expect(Token::OpenBrace)?;

        // Parse fields
        let mut fields = Vec::new();

        while !matches!(self.current_token, Token::CloseBrace) {
            if matches!(self.current_token, Token::Identifier(_)) {
                let field = self.parse_field()?;
                fields.push(field);
            } else if matches!(self.current_token, Token::Eof) {
                return Err(WAILParseError::UnexpectedEOF {
                    expected: "field or closing brace".to_string(),
                    location: self.tokenizer.last_location(),
                });
            } else {
                // Skip any non-identifier tokens (this helps with unexpected whitespace/comments)
                self.next_token();
            }
        }

        // Expect closing brace
        self.expect(Token::CloseBrace)?;

        // Parse annotations
        let annotations = self.parse_annotations()?;

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

        // Add _type field
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

        // Add _type field to fields list
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

        // Create object
        let object = WAILObject {
            value: field_map,
            type_data: WAILTypeData {
                json_type: JsonValue::Object(HashMap::new()), // Placeholder empty object
                type_name: name,
                field_definitions: Some(fields.clone()),
                element_type: None,
            },
        };

        let field = WAILField {
            name: name.to_string(),
            field_type: WAILType::Composite(WAILCompositeType::Object(object)),
            annotations,
        };

        let definition = WAILDefinition::Object(field.clone());

        let namespaced_id = self.namespaced_identifier(name.to_string());

        {
            // Add object to registry
            self.registry.borrow_mut().insert(namespaced_id, field);
        }

        Ok(definition)
    }

    // Update the parse_template method to handle triple-quoted strings better
    fn parse_template(&mut self) -> Result<WAILDefinition<'a>, WAILParseError> {
        // Expect "template" keyword
        self.expect_keyword("template")?;

        // Parse template name
        let name = self.expect_identifier()?;

        let namespaced_id = self.namespaced_identifier(name.to_string());

        {
            // Check for duplicate definition
            if self.template_registry.borrow().contains_key(&namespaced_id) {
                return Ok(WAILDefinition::Template(
                    self.lookup_template_in_registry(&name)?.clone(),
                ));
            }
        }

        // Expect opening parenthesis
        self.expect(Token::OpenParen)?;

        // Parse parameters
        let mut parameters = Vec::new();

        if !matches!(self.current_token, Token::CloseParen) {
            loop {
                // Parse parameter
                let param = self.parse_parameter()?;
                parameters.push(param);

                // Check for more parameters
                if matches!(self.current_token, Token::Comma) {
                    self.next_token();
                } else {
                    break;
                }
            }
        }

        // Expect closing parenthesis
        self.expect(Token::CloseParen)?;

        // Expect arrow token
        self.expect(Token::Arrow)?;

        // Parse return type
        let return_type = self.parse_type()?;

        // Parse annotations
        let annotations = self.parse_annotations()?;

        // Expect opening brace
        self.expect(Token::OpenBrace)?;

        // Parse prompt keyword and colon
        if let Token::Keyword("prompt") = self.current_token {
            self.next_token();
        } else {
            return Err(WAILParseError::UnexpectedToken {
                found: format!("{}", self.current_token),
                location: self.tokenizer.last_location(),
            });
        }

        self.expect(Token::Colon)?;

        // Parse prompt template (triple quoted string)
        match self.current_token {
            Token::TripleQuoteContent(content) => {
                let prompt_template = {
                    let content_str = content.to_string();
                    self.next_token(); // Consume the content
                                       // Adjust indentation for multi-line prompt
                    adjust_indentation(&content_str, 0)
                };

                // Expect closing brace
                self.expect(Token::CloseBrace)?;

                // Create output field
                let output_field = WAILField {
                    name: match &return_type {
                        WAILType::Composite(WAILCompositeType::Object(obj)) => {
                            obj.type_data.type_name.to_string()
                        }
                        _ => "output".to_string(), // Default name if not an object type
                    },
                    field_type: return_type,
                    annotations: Vec::new(),
                };

                let template_def = WAILTemplateDef {
                    name: name.to_string(),
                    inputs: parameters,
                    output: output_field,
                    prompt_template,
                    annotations,
                };

                let namespaced_id = self.namespaced_identifier(name.to_string());

                // Add to template registry
                self.template_registry
                    .borrow_mut()
                    .insert(namespaced_id, template_def.clone());

                Ok(WAILDefinition::Template(template_def))
            }
            _ => Err(WAILParseError::UnexpectedToken {
                found: format!("{}", self.current_token),
                location: self.tokenizer.last_location(),
            }),
        }
    }
    fn parse_wail_file(
        &mut self,
        file_type: WAILFileType,
    ) -> Result<Vec<WAILDefinition<'a>>, WAILParseError> {
        let mut definitions = Vec::new();
        let mut imports = Vec::new();

        self.optional_whitespace();

        // Parse imports first
        while let Token::Keyword("import") = self.current_token {
            let import = self.parse_import()?;
            imports.push(import.clone());
            definitions.push(import);
        }

        // Process imports after they're all parsed
        if !imports.is_empty() {
            self.resolve_imports(&imports)?;
        }

        // Parse regular definitions
        while self.current_token != Token::Eof {
            match self.current_token {
                Token::Keyword("object") => {
                    let object = self.parse_object()?;
                    definitions.push(object);
                }
                Token::Keyword("template") => {
                    let template = self.parse_template()?;
                    definitions.push(template);
                }
                Token::Keyword("union") => {
                    let union = self.parse_union()?;
                    definitions.push(union);
                }
                Token::Keyword("main") => {
                    if file_type == WAILFileType::Library {
                        return Err(WAILParseError::UnexpectedToken {
                            found: "main block in library file".to_string(),
                            location: self.tokenizer.last_location(),
                        });
                    }

                    let main = self.parse_main()?;
                    definitions.push(WAILDefinition::Main(main));
                    // Main should be the last definition, but we'll continue parsing
                    // to catch any trailing content errors
                }
                Token::Eof => break,
                _ => {
                    // Skip any non-keyword tokens
                    self.next_token();
                }
            }
        }

        if file_type == WAILFileType::Application
            && !definitions
                .iter()
                .any(|def| matches!(def, WAILDefinition::Main(_)))
        {
            return Err(WAILParseError::MissingMainBlock);
        }

        Ok(definitions)
    }

    fn resolve_imports(&mut self, imports: &[WAILDefinition<'a>]) -> Result<(), WAILParseError> {
        for def in imports {
            if let WAILDefinition::Import(import) = def {
                // Resolve the import path
                let file_path = self.import_chain.borrow().resolve_path(&import.path)?;
                self.current_module.borrow_mut().push(import.path.clone());

                // Check for circular imports
                if let Err(e) = self.import_chain.borrow_mut().push(&file_path) {
                    return Err(e);
                }
                // Read the file content
                let lib_content =
                    std::fs::read_to_string(&file_path).map_err(|e| WAILParseError::FileError {
                        path: import.path.clone(),
                        error: e.to_string(),
                    })?;

                // Make the string live for 'a lifetime
                let lib_content = Box::leak(lib_content.into_boxed_str());

                // Create a new parser for this import with the same shared state
                let mut import_parser = Parser::new(
                    lib_content,
                    self.registry,
                    self.template_registry,
                    self.adhoc_obj_ref_id_counter,
                    self.adhoc_obj_ids,
                    self.adhoc_obj_refs,
                    self.object_instances,
                    self.main,
                    self.import_chain,
                    self.current_module,
                );

                // Parse the library file
                let lib_defs = import_parser.parse_wail_file(WAILFileType::Library)?;

                // Create a map of definitions from the library
                let mut objects = HashMap::new();
                for lib_def in &lib_defs {
                    if let Some(name) = lib_def.get_name() {
                        objects.insert(name.to_string(), lib_def.clone());
                    }
                }

                // Process each requested item
                for item_name in &import.items {
                    let mut found = false;
                    if let Some(lib_def) = objects.get(item_name) {
                        found = true;

                        // Add the definition to the appropriate registry
                        match lib_def {
                            WAILDefinition::Object(field) => {
                                let namespaced_id = self.namespaced_identifier(field.name.clone());

                                let mut reg_borrow = self.registry.borrow_mut();

                                match reg_borrow.get(&namespaced_id) {
                                    Some(_) => continue,
                                    None => {
                                        reg_borrow.insert(namespaced_id, field.clone());
                                        drop(reg_borrow)
                                    }
                                }
                            }
                            WAILDefinition::Template(template) => {
                                let namespaced_id =
                                    self.namespaced_identifier(template.name.clone());
                                let mut reg_borrow = self.template_registry.borrow_mut();

                                match reg_borrow.get(&namespaced_id) {
                                    Some(_) => continue,
                                    None => {
                                        reg_borrow.insert(namespaced_id, template.clone());
                                        drop(reg_borrow)
                                    }
                                }
                            }
                            WAILDefinition::Union(field) => {
                                let namespaced_id = self.namespaced_identifier(field.name.clone());
                                let mut reg_borrow = self.registry.borrow_mut();

                                match reg_borrow.get(&namespaced_id) {
                                    Some(_) => continue,
                                    None => {
                                        reg_borrow.insert(namespaced_id, field.clone());
                                        drop(reg_borrow)
                                    }
                                }
                            }
                            _ => {}
                        }

                        // Also process referenced types
                        match lib_def {
                            WAILDefinition::Object(field) => {
                                self.add_referenced_types(&field.field_type, &objects);
                            }
                            WAILDefinition::Template(template) => {
                                for param in &template.inputs {
                                    self.add_referenced_types(&param.field_type, &objects);
                                }
                                self.add_referenced_types(&template.output.field_type, &objects);
                            }
                            WAILDefinition::Union(field) => {
                                self.add_referenced_types(&field.field_type, &objects);
                            }
                            _ => {}
                        }
                    }

                    if !found {
                        return Err(WAILParseError::ImportNotFound {
                            name: item_name.clone(),
                            path: import.path.clone(),
                        });
                    }
                }

                // Pop from import chain after processing
                self.import_chain.borrow_mut().pop();
                self.current_module.borrow_mut().pop();
            }
        }

        Ok(())
    }

    // Helper method to add referenced types
    fn add_referenced_types(
        &self,
        field_type: &WAILType<'a>,
        objects: &HashMap<String, WAILDefinition<'a>>,
    ) {
        match field_type {
            WAILType::Composite(composite) => match composite {
                WAILCompositeType::Object(obj) => {
                    let type_name = obj.type_data.type_name;

                    {
                        // If this type exists in objects map and not already in registry
                        if objects.contains_key(type_name)
                            && !self.registry.borrow().contains_key(type_name)
                        {
                            if let Some(WAILDefinition::Object(field)) = objects.get(type_name) {
                                {
                                    self.registry
                                        .borrow_mut()
                                        .insert(field.name.clone(), field.clone());
                                }

                                // Recursively add fields
                                if let Some(fields) = &obj.type_data.field_definitions {
                                    for field in fields {
                                        self.add_referenced_types(&field.field_type, objects);
                                    }
                                }
                            }
                        }
                    }
                }
                WAILCompositeType::Array(array) => {
                    if let Some(element_type) = &array.type_data.element_type {
                        self.add_referenced_types(element_type, objects);
                    }
                }
                WAILCompositeType::Union(union) => {
                    for member in &union.members {
                        self.add_referenced_types(&member.field_type, objects);
                    }
                }
            },
            _ => {} // Simple types don't reference other objects
        }
    }

    // Update other parsing methods that work with newlines and comments
    fn parse_field(&mut self) -> Result<WAILField<'a>, WAILParseError> {
        // Parse field name
        let name = self.expect_identifier()?;

        // Expect colon
        self.expect(Token::Colon)?;

        // Parse field type
        let field_type = self.parse_type()?;

        // Parse annotations
        let annotations = self.parse_annotations()?;

        Ok(WAILField {
            name: name.to_string(),
            field_type,
            annotations,
        })
    }

    fn lookup_template_in_registry(
        &self,
        name: &str,
    ) -> Result<WAILTemplateDef<'a>, WAILParseError> {
        let mut matches = vec![];

        {
            for (key, def) in self.template_registry.borrow().iter() {
                if let Some(actual_name) = key.split('.').last() {
                    if actual_name == name {
                        matches.push((key.clone(), def.clone()));
                    }
                }
            }
        }

        match matches.len() {
            0 => Err(WAILParseError::SymbolNotFound {
                name: name.to_string(),
            }),
            1 => Ok(matches.remove(0).1),
            _ => Err(WAILParseError::AmbiguousSymbol {
                name: name.to_string(),
                matches: matches.into_iter().map(|(k, _)| k).collect(),
            }),
        }
    }

    fn lookup_adhoc_obj_in_registry(&self, name: &str) -> Result<WAILObject, WAILParseError> {
        let mut matches = vec![];

        {
            for (key, def) in self.adhoc_obj_refs.borrow().iter() {
                if let Some(actual_name) = key.split('.').last() {
                    if actual_name == name {
                        matches.push((key.clone(), def.clone()));
                    }
                }
            }
        }

        match matches.len() {
            0 => Err(WAILParseError::SymbolNotFound {
                name: name.to_string(),
            }),
            1 => Ok(matches.remove(0).1),
            _ => Err(WAILParseError::AmbiguousSymbol {
                name: name.to_string(),
                matches: matches.into_iter().map(|(k, _)| k).collect(),
            }),
        }
    }

    fn lookup_symbol_in_registry(&self, name: &str) -> Result<WAILField<'a>, WAILParseError> {
        let mut matches = vec![];

        {
            for (key, def) in self.registry.borrow().iter() {
                if let Some(actual_name) = key.split('.').last() {
                    if actual_name == name {
                        matches.push((key.clone(), def.clone()));
                    }
                }
            }
        }

        match matches.len() {
            0 => Err(WAILParseError::SymbolNotFound {
                name: name.to_string(),
            }),
            1 => Ok(matches.remove(0).1.clone()),
            _ => Err(WAILParseError::AmbiguousSymbol {
                name: name.to_string(),
                matches: matches.into_iter().map(|(k, _)| k).collect(),
            }),
        }
    }

    fn parse_union(&mut self) -> Result<WAILDefinition<'a>, WAILParseError> {
        // Expect "union" keyword
        self.expect_keyword("union")?;

        // Parse union name
        let name = self.expect_identifier()?;

        let namespaced_id = self.namespaced_identifier(name.to_string());

        {
            if self.registry.borrow().contains_key(&namespaced_id) {
                return Ok(WAILDefinition::Object(
                    self.lookup_symbol_in_registry(&name)?.clone(),
                ));
            }
        }

        // Expect equals sign
        self.expect(Token::Equals)?;

        // Parse first type
        let first_type_name = self.expect_identifier()?;

        // Check for array syntax
        let first_is_array = if matches!(self.current_token, Token::OpenBracket) {
            self.next_token();
            self.expect(Token::CloseBracket)?;
            true
        } else {
            false
        };

        // Create first type
        let first_type = self.create_type_value(first_type_name, first_is_array)?;

        // Parse additional union members
        let mut member_types = vec![first_type];

        while matches!(self.current_token, Token::Pipe) {
            self.next_token();

            // Parse member type
            let member_name = self.expect_identifier()?;

            // Check for array syntax
            let member_is_array = if matches!(self.current_token, Token::OpenBracket) {
                self.next_token();
                self.expect(Token::CloseBracket)?;
                true
            } else {
                false
            };

            // Create member type
            let member_type = self.create_type_value(member_name, member_is_array)?;
            member_types.push(member_type);
        }

        // Expect semicolon
        self.expect(Token::Semicolon)?;

        // Create union members from types
        let mut members = Vec::new();
        for (i, type_val) in member_types.into_iter().enumerate() {
            members.push(WAILField {
                name: format!("member_{}", i),
                field_type: type_val,
                annotations: Vec::new(),
            });
        }

        // Create union type
        let union = WAILUnion {
            members,
            type_data: WAILTypeData {
                json_type: JsonValue::Object(HashMap::new()),
                type_name: name,
                field_definitions: None,
                element_type: None,
            },
        };

        let field = WAILField {
            name: name.to_string(),
            field_type: WAILType::Composite(WAILCompositeType::Union(union)),
            annotations: Vec::new(),
        };

        let namespaced_id = self.namespaced_identifier(name.to_string());

        // Add to registry
        self.registry
            .borrow_mut()
            .insert(namespaced_id, field.clone());

        Ok(WAILDefinition::Union(field))
    }

    fn parse_main(&mut self) -> Result<WAILMainDef<'a>, WAILParseError> {
        // Check if main block already exists
        if self.main.borrow().is_some() {
            return Err(WAILParseError::DuplicateDefinition {
                name: "main".to_string(),
                location: self.tokenizer.last_location(),
            });
        }

        // Expect "main" keyword
        self.expect_keyword("main")?;

        // Expect opening brace
        self.expect(Token::OpenBrace)?;

        // Parse optional template_args
        let template_args = if matches!(self.current_token, Token::Keyword("template_args")) {
            self.next_token();
            self.parse_template_args()?
        } else {
            HashMap::new()
        };

        // Parse statements
        let mut statements = Vec::new();

        while !matches!(self.current_token, Token::Keyword("prompt"))
            && !matches!(self.current_token, Token::CloseBrace)
        {
            match self.current_token {
                Token::Keyword("let") => {
                    let statement = self.parse_assignment_statement()?;
                    statements.push(statement);
                }
                Token::Identifier(_) => {
                    // Template call without assignment
                    let template_call = self.parse_template_call()?;
                    statements.push(MainStatement::TemplateCall(template_call));
                    self.expect(Token::Semicolon)?;
                }
                Token::Eof => {
                    return Err(WAILParseError::UnexpectedEOF {
                        expected: "prompt block or closing brace".to_string(),
                        location: self.tokenizer.last_location(),
                    });
                }
                _ => {
                    // Skip any non-statement tokens
                    self.next_token();
                }
            }
        }

        // Parse prompt block
        let prompt_str = if matches!(self.current_token, Token::Keyword("prompt")) {
            self.next_token();
            self.parse_prompt_block()?
        } else {
            return Err(WAILParseError::UnexpectedToken {
                found: "Expected prompt block".to_string(),
                location: self.tokenizer.last_location(),
            });
        };
        self.optional_whitespace();

        // Expect closing brace
        self.expect(Token::CloseBrace)?;

        let main_def = WAILMainDef::new(statements, prompt_str, Some(template_args));

        // Add to main reference
        self.main.borrow_mut().replace(main_def.clone());

        Ok(main_def)
    }

    fn parse_prompt_block(&mut self) -> Result<String, WAILParseError> {
        self.in_prompt_block.replace(true);
        // Expect opening brace
        self.expect(Token::OpenBrace)?;

        // Collect all tokens until the closing brace, preserving whitespace
        let mut prompt_content = String::new();
        let mut brace_count = 1;

        while brace_count > 0 && self.current_token != Token::Eof {
            match &self.current_token {
                Token::OpenBrace => {
                    brace_count += 1;
                    prompt_content.push('{');
                    self.next_token();
                }
                Token::CloseBrace => {
                    brace_count -= 1;
                    if brace_count > 0 {
                        prompt_content.push('}');
                    }
                    self.next_token();
                }
                Token::Whitespace(s) => {
                    prompt_content.push_str(s);
                    self.next_token();
                }
                Token::Newline => {
                    prompt_content.push('\n');
                    self.next_token();
                }
                Token::Identifier(s) => {
                    prompt_content.push_str(s);
                    self.next_token();
                }
                Token::String(s) => {
                    prompt_content.push_str(&format!("\"{}\"", s));
                    self.next_token();
                }
                Token::Number(n) => {
                    prompt_content.push_str(&n.to_string());
                    self.next_token();
                }
                Token::Eof => {
                    return Err(WAILParseError::UnexpectedEOF {
                        expected: "Closing brace for prompt block".to_string(),
                        location: self.tokenizer.last_location(),
                    });
                }
                _ => {
                    // Add token content to prompt
                    prompt_content.push_str(&format!("{}", self.current_token));
                    self.next_token();
                }
            }
        }

        self.in_prompt_block.replace(false);

        Ok(prompt_content)
    }

    //-------------------------------###########

    fn next_token(&mut self) -> Token<'a> {
        let result = self.current_token.clone();
        self.current_token = self.peek_token.clone();

        // Get the next token
        let mut next_token = self.tokenizer.next_token();

        // Skip whitespace and comments by default
        let in_prompt = self.in_prompt_block.borrow();

        while !*in_prompt
            && (matches!(next_token, Token::Whitespace(_))
                || matches!(next_token, Token::Comment(_)))
        {
            next_token = self.tokenizer.next_token();
        }

        self.peek_token = next_token;

        result
    }

    fn peek(&self) -> &Token<'a> {
        &self.peek_token
    }

    fn expect(&mut self, expected: Token<'a>) -> Result<(), WAILParseError> {
        if std::mem::discriminant(&self.current_token) == std::mem::discriminant(&expected) {
            self.next_token();
            Ok(())
        } else {
            Err(WAILParseError::UnexpectedToken {
                found: format!("{}", self.current_token),
                location: self.tokenizer.last_location(),
            })
        }
    }

    fn expect_identifier(&mut self) -> Result<&'a str, WAILParseError> {
        self.optional_whitespace();

        match self.current_token {
            Token::Identifier(name) => {
                let result = name;
                self.next_token();
                Ok(result)
            }
            Token::Keyword(name) => Err(WAILParseError::UnexpectedKeyword {
                found: format!("{}", name),
                location: self.tokenizer.last_location(),
            }),
            _ => Err(WAILParseError::UnexpectedToken {
                found: format!("{}", self.current_token),
                location: self.tokenizer.last_location(),
            }),
        }
    }

    fn expect_keyword(&mut self, keyword: &str) -> Result<(), WAILParseError> {
        match self.current_token {
            Token::Keyword(k) if k == keyword => {
                self.next_token();
                Ok(())
            }
            _ => Err(WAILParseError::UnexpectedToken {
                found: format!("{}", self.current_token),
                location: self.tokenizer.last_location(),
            }),
        }
    }

    fn expect_string(&mut self) -> Result<&'a str, WAILParseError> {
        match self.current_token {
            Token::String(s) => {
                let result = s;
                self.next_token();
                Ok(result)
            }
            _ => Err(WAILParseError::UnexpectedToken {
                found: format!("{}", self.current_token),
                location: self.tokenizer.last_location(),
            }),
        }
    }

    fn parse_import(&mut self) -> Result<WAILDefinition<'a>, WAILParseError> {
        self.optional_whitespace();
        self.expect_keyword("import")?;

        self.optional_whitespace();
        self.expect(Token::OpenBrace)?;

        // Parse imported item names
        let mut items = Vec::new();

        while self.current_token != Token::CloseBrace {
            match self.current_token {
                Token::Identifier(name) => {
                    items.push(name.to_string());
                    self.next_token();

                    // Expect comma or closing brace
                    if self.current_token == Token::Comma {
                        self.next_token();
                    } else if self.current_token != Token::CloseBrace {
                        return Err(WAILParseError::UnexpectedToken {
                            found: format!("{}", self.current_token),
                            location: self.tokenizer.last_location(),
                        });
                    }
                }
                _ => {
                    return Err(WAILParseError::UnexpectedToken {
                        found: format!("{}", self.current_token),
                        location: self.tokenizer.last_location(),
                    });
                }
            }
        }

        self.expect(Token::CloseBrace)?;

        self.expect_keyword("from")?;

        // Parse path string
        let path = self.expect_string()?;

        Ok(WAILDefinition::Import(WAILImport {
            items: items.iter().map(|s| s.to_string()).collect(),
            path: path.to_string(),
        }))
    }

    fn parse_type(&mut self) -> Result<WAILType<'a>, WAILParseError> {
        // Check for adhoc object type (opening brace without a type name)
        if matches!(self.current_token, Token::OpenBrace) {
            return self.parse_adhoc_object_type();
        }

        // Parse base type name
        let base_type = self.expect_identifier()?;

        // Check for array suffix
        let is_array = if matches!(self.current_token, Token::OpenBracket) {
            self.next_token();
            self.expect(Token::CloseBracket)?;
            true
        } else {
            false
        };

        // Check for union type with pipe operator
        let mut union_members = Vec::new();
        let mut is_union = false;

        if matches!(self.current_token, Token::Pipe) {
            is_union = true;

            // Create base type value for first union member
            let base_type_val = self.create_type_value(base_type, is_array)?;
            union_members.push(base_type_val);

            // Parse additional union members
            while matches!(self.current_token, Token::Pipe) {
                self.next_token();

                // Parse member type name
                let member_type = self.expect_identifier()?;

                // Check for array suffix for this member
                let member_is_array = if matches!(self.current_token, Token::OpenBracket) {
                    self.next_token();
                    self.expect(Token::CloseBracket)?;
                    true
                } else {
                    false
                };

                // Create type value for this member
                let member_type_val = self.create_type_value(member_type, member_is_array)?;
                union_members.push(member_type_val);
            }

            // Create union type
            let mut wail_fields = Vec::new();

            for (i, type_data) in union_members.iter().enumerate() {
                let field = WAILField {
                    name: format!("member_{}", i),
                    field_type: type_data.clone(),
                    annotations: Vec::new(),
                };

                wail_fields.push(field);
            }

            return Ok(WAILType::Composite(WAILCompositeType::Union(WAILUnion {
                members: wail_fields,
                type_data: WAILTypeData {
                    json_type: JsonValue::Object(HashMap::new()),
                    type_name: "Union", // Default name
                    field_definitions: None,
                    element_type: None,
                },
            })));
        }

        // If not a union, create the base type (possibly wrapped in array)
        self.create_type_value(base_type, is_array)
    }

    fn namespaced_identifier(&self, id: String) -> String {
        let module_prefix = self.current_module.borrow().last().cloned();
        match module_prefix {
            Some(prefix) => format!("{}.{}", prefix, id),
            None => id.to_string(), // top-level file, no prefix
        }
    }

    fn parse_adhoc_object_type(&mut self) -> Result<WAILType<'a>, WAILParseError> {
        // Generate a unique ID for the adhoc object
        let adhoc_id = {
            let mut counter = self.adhoc_obj_ref_id_counter.borrow_mut();
            *counter += 1;
            format!("adhoc_{}", *counter)
        };

        // Expect opening brace
        self.expect(Token::OpenBrace)?;

        // Parse fields
        let mut fields = Vec::new();

        while !matches!(self.current_token, Token::CloseBrace) {
            if matches!(self.current_token, Token::Identifier(_)) {
                let field = self.parse_field()?;
                fields.push(field);
            } else if matches!(self.current_token, Token::Newline)
                || matches!(self.current_token, Token::Whitespace(_))
            {
                self.next_token(); // Skip newlines and whitespace between fields
            } else if matches!(self.current_token, Token::Hash) {
                self.parse_comment()?; // Skip comments between fields
            } else {
                return Err(WAILParseError::UnexpectedToken {
                    found: format!("{}", self.current_token),
                    location: self.tokenizer.last_location(),
                });
            }
        }

        // Expect closing brace
        self.expect(Token::CloseBrace)?;

        // Create field map
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

        self.adhoc_obj_ids.borrow_mut().push(adhoc_id.clone());

        // Create the adhoc object
        let adhoc_type_name = Box::leak(adhoc_id.clone().into_boxed_str());
        let object = WAILObject {
            value: field_map,
            type_data: WAILTypeData {
                json_type: JsonValue::Object(HashMap::new()),
                type_name: adhoc_type_name,
                field_definitions: Some(fields.clone()),
                element_type: None,
            },
        };

        let namespaced_id = self.namespaced_identifier(adhoc_id.clone());

        self.adhoc_obj_refs
            .borrow_mut()
            .insert(namespaced_id.clone(), object.clone());

        let field = WAILField {
            name: adhoc_id.clone(),
            field_type: WAILType::Composite(WAILCompositeType::Object(object.clone())),
            annotations: vec![],
        };

        self.registry
            .borrow_mut()
            .insert(namespaced_id.clone(), field.clone());

        Ok(WAILType::Composite(WAILCompositeType::Object(object)))
    }

    fn create_type_value(
        &self,
        type_name: &'a str,
        is_array: bool,
    ) -> Result<WAILType<'a>, WAILParseError> {
        // Create inner type based on name
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
            "Boolean" => WAILType::Simple(WAILSimpleType::Boolean(WAILBoolean {
                value: "false".to_string(),
                type_data: WAILTypeData {
                    json_type: JsonValue::Boolean(false),
                    type_name: type_name,
                    field_definitions: None,
                    element_type: None,
                },
            })),
            // For other types, check if it's registered or assume it's an object/custom type
            _ => match self.lookup_symbol_in_registry(&type_name) {
                Ok(field) => field.field_type.clone(),
                Err(e) => match e {
                    WAILParseError::AmbiguousSymbol { .. } => return Err(e),
                    _ => WAILType::Composite(WAILCompositeType::Object(WAILObject {
                        value: HashMap::new(),
                        type_data: WAILTypeData {
                            json_type: JsonValue::Object(HashMap::new()),
                            type_name: type_name,
                            field_definitions: None,
                            element_type: None,
                        },
                    })),
                },
            },
        };

        // Wrap in array if needed
        if is_array {
            Ok(WAILType::Composite(WAILCompositeType::Array(WAILArray {
                values: Vec::new(),
                type_data: WAILTypeData {
                    json_type: JsonValue::Array(Vec::new()),
                    type_name: "Array",
                    field_definitions: None,
                    element_type: Some(Box::new(inner_type)),
                },
            })))
        } else {
            Ok(inner_type)
        }
    }

    fn parse_annotations(&mut self) -> Result<Vec<WAILAnnotation>, WAILParseError> {
        let mut annotations = Vec::new();

        while matches!(self.current_token, Token::At) {
            self.next_token();

            // Parse annotation name
            match self.current_token {
                Token::Identifier("description") => {
                    self.next_token();

                    // Expect opening parenthesis
                    self.expect(Token::OpenParen)?;

                    // Parse string literal
                    let desc = self.expect_string()?;

                    // Expect closing parenthesis
                    self.expect(Token::CloseParen)?;

                    annotations.push(WAILAnnotation::Description(desc.to_string()));
                }
                _ => {
                    return Err(WAILParseError::UnexpectedToken {
                        found: format!("{}", self.current_token),
                        location: self.tokenizer.last_location(),
                    });
                }
            }
        }

        Ok(annotations)
    }

    fn parse_comment(&mut self) -> Result<String, WAILParseError> {
        match self.current_token {
            Token::Hash => {
                self.next_token(); // Skip the # token

                match self.current_token {
                    Token::Comment(comment) => {
                        let result = comment.to_string();
                        self.next_token();
                        Ok(result)
                    }
                    _ => Ok(String::new()), // Empty comment
                }
            }
            _ => Err(WAILParseError::UnexpectedToken {
                found: format!("{}", self.current_token),
                location: self.tokenizer.last_location(),
            }),
        }
    }

    fn parse_parameter(&mut self) -> Result<WAILField<'a>, WAILParseError> {
        // Parse parameter name
        let name = self.expect_identifier()?;

        // Expect colon
        self.expect(Token::Colon)?;

        // Parse parameter type
        let param_type = self.parse_type()?;

        Ok(WAILField {
            name: name.to_string(),
            field_type: param_type,
            annotations: Vec::new(),
        })
    }

    fn parse_template_args(&mut self) -> Result<HashMap<String, WAILType<'a>>, WAILParseError> {
        let mut args = HashMap::new();

        // Expect opening brace
        self.expect(Token::OpenBrace)?;

        while !matches!(self.current_token, Token::CloseBrace) {
            if matches!(self.current_token, Token::Newline)
                || matches!(self.current_token, Token::Whitespace(_))
            {
                self.next_token(); // Skip whitespace and newlines
                continue;
            }

            if matches!(self.current_token, Token::Identifier(_)) {
                let arg_name = self.expect_identifier()?.to_string();

                // Expect colon
                self.expect(Token::Colon)?;

                // Parse type
                let arg_type = self.parse_type()?;

                args.insert(arg_name, arg_type);

                // Expect comma or closing brace
                if matches!(self.current_token, Token::Comma) {
                    self.next_token();
                } else if !matches!(self.current_token, Token::CloseBrace) {
                    return Err(WAILParseError::UnexpectedToken {
                        found: format!("{}", self.current_token),
                        location: self.tokenizer.last_location(),
                    });
                }
            } else {
                return Err(WAILParseError::UnexpectedToken {
                    found: format!("{}", self.current_token),
                    location: self.tokenizer.last_location(),
                });
            }
        }

        // Expect closing brace
        self.expect(Token::CloseBrace)?;

        Ok(args)
    }

    fn optional_whitespace(&mut self) {
        // Consume any whitespace tokens if they exist
        while matches!(self.current_token, Token::Whitespace(_))
            || matches!(self.current_token, Token::Newline)
        {
            self.next_token();
        }
    }

    fn parse_assignment_statement(&mut self) -> Result<MainStatement, WAILParseError> {
        // Expect "let" keyword
        self.expect_keyword("let")?;

        // Parse variable name
        let var_name = self.expect_identifier()?.to_string();

        // Expect equals sign
        self.expect(Token::Equals)?;

        // Parse template call (could be either a template call or object instantiation)
        let template_call = self.parse_template_call()?;

        // Check if this is an object instantiation
        let registry = self.registry.borrow();
        if let Some(field) = registry.get(&template_call.template_name) {
            if let WAILType::Composite(WAILCompositeType::Object(_)) = &field.field_type {
                // This is an object instantiation

                // Expect semicolon
                self.expect(Token::Semicolon)?;

                // Create object instantiation
                let obj_instantiation = self.instantiate_object(
                    &var_name,
                    &template_call.template_name,
                    template_call.arguments.clone(),
                )?;

                let namespaced_id = self.namespaced_identifier(var_name.clone());

                // Add to object instances
                self.object_instances
                    .borrow_mut()
                    .insert(namespaced_id.clone(), obj_instantiation);

                return Ok(MainStatement::ObjectInstantiation {
                    variable: var_name,
                    object_type: template_call.template_name.clone(),
                    arguments: template_call.arguments,
                });
            }
        }
        drop(registry);
        match self.lookup_template_in_registry(&template_call.template_name) {
            Err(WAILParseError::SymbolNotFound { .. }) => {
                return Err(WAILParseError::InvalidTemplateCall {
                    template_name: template_call.template_name.clone(),
                    reason: format!("Template '{}' not found", template_call.template_name),
                    location: self.tokenizer.last_location(),
                });
            }
            Err(e) => return Err(e),
            _ => (),
        }

        // Expect semicolon
        self.expect(Token::Semicolon)?;

        Ok(MainStatement::Assignment {
            variable: var_name,
            template_call,
        })
    }

    fn parse_template_call(&mut self) -> Result<WAILTemplateCall, WAILParseError> {
        // Parse template name
        let template_name = self.expect_identifier()?.to_string();

        // Expect opening parenthesis
        self.expect(Token::OpenParen)?;

        // Parse arguments
        let mut arguments = HashMap::new();

        while !matches!(self.current_token, Token::CloseParen) {
            if matches!(self.current_token, Token::Newline)
                || matches!(self.current_token, Token::Whitespace(_))
            {
                self.next_token(); // Skip whitespace and newlines
                continue;
            }

            if matches!(self.current_token, Token::Identifier(_)) {
                let arg_name = self.expect_identifier()?.to_string();

                // Expect colon
                self.expect(Token::Colon)?;

                // Parse argument value
                let arg_value = self.parse_template_argument()?;

                arguments.insert(arg_name, arg_value);

                // Expect comma or closing parenthesis
                if matches!(self.current_token, Token::Comma) {
                    self.next_token();
                } else if !matches!(self.current_token, Token::CloseParen) {
                    return Err(WAILParseError::UnexpectedToken {
                        found: format!("{}", self.current_token),
                        location: self.tokenizer.last_location(),
                    });
                }
            } else {
                return Err(WAILParseError::UnexpectedToken {
                    found: format!("{}", self.current_token),
                    location: self.tokenizer.last_location(),
                });
            }
        }

        // Expect closing parenthesis
        self.expect(Token::CloseParen)?;

        Ok(WAILTemplateCall {
            template_name,
            arguments,
        })
    }

    fn parse_template_argument(&mut self) -> Result<TemplateArgument, WAILParseError> {
        match self.current_token {
            Token::Dollar => {
                // Template argument reference with $ prefix
                self.next_token();

                let name = self.expect_identifier()?.to_string();
                Ok(TemplateArgument::TemplateArgRef(name))
            }
            Token::Identifier(name) => {
                // Object reference or type reference
                self.next_token();

                {
                    // Check if it's an object instance reference
                    if self.object_instances.borrow().contains_key(name) {
                        Ok(TemplateArgument::ObjectRef(name.to_string()))
                    } else if self.registry.borrow().contains_key(name) {
                        // Check if it's a type reference
                        Ok(TemplateArgument::TypeRef(name.to_string()))
                    } else {
                        // Treat as a variable reference
                        Ok(TemplateArgument::TemplateArgRef(name.to_string()))
                    }
                }
            }
            Token::String(s) => {
                // String literal
                self.next_token();
                Ok(TemplateArgument::String(s.to_string()))
            }
            Token::Number(n) => {
                // Number literal
                self.next_token();
                Ok(TemplateArgument::Number(n))
            }
            _ => Err(WAILParseError::UnexpectedToken {
                found: format!("{}", self.current_token),
                location: self.tokenizer.last_location(),
            }),
        }
    }

    fn instantiate_object(
        &self,
        name: &str,
        object_type: &str,
        args: HashMap<String, TemplateArgument>,
    ) -> Result<WAILObjectInstantiation, WAILParseError> {
        // Get the object definition from registry
        let registry = self.registry.borrow();
        let field = match registry.get(object_type) {
            Some(f) => f,
            None => {
                drop(registry);
                return Err(WAILParseError::UndefinedType {
                    name: object_type.to_string(),
                    location: self.tokenizer.last_location(),
                });
            }
        };

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

            drop(registry);
            Ok(WAILObjectInstantiation {
                binding_name: name.to_string(),
                object_type: object_type.to_string(),
                fields: args,
            })
        } else {
            drop(registry);
            Err(WAILParseError::InvalidTemplateCall {
                template_name: object_type.to_string(),
                reason: format!("{} is not an object type", object_type),
                location: self.tokenizer.last_location(),
            })
        }
    }
}

impl<'a> WAILParser<'a> {
    pub fn new(base_path: PathBuf) -> Self {
        Self {
            registry: RefCell::new(HashMap::new()),
            template_registry: RefCell::new(HashMap::new()),
            adhoc_obj_ref_id_counter: RefCell::new(0),
            adhoc_obj_ids: RefCell::new(Vec::new()),
            adhoc_obj_refs: RefCell::new(HashMap::new()),
            main: RefCell::new(None),
            object_instances: RefCell::new(HashMap::new()),
            import_chain: RefCell::new(ImportChain::new(base_path.clone())),
            base_path: base_path.clone(),
            incremental_parser: RefCell::new(None),
            current_module: RefCell::new(vec![]),
        }
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

    pub fn incremental_parser(&'a self, input: &'a str) -> Parser<'a> {
        Parser::new(
            input,
            &self.registry,
            &self.template_registry,
            &self.adhoc_obj_ref_id_counter,
            &self.adhoc_obj_ids,
            &self.adhoc_obj_refs,
            &self.object_instances,
            &self.main,
            &self.import_chain,
            &self.current_module,
        )
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
        let result_pos = input.find("<action>");

        match result_pos {
            Some(_) => self.parse_result_block(input), // Only <action> block is present.
            None => Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Tag,
            ))), // Neither pattern is found.
        }
    }

    /// Parse a <action></action> fenced block.
    fn parse_result_block(&'a self, input: &'a str) -> IResult<&'a str, String> {
        let (input, _) = take_until("<action>")(input)?;
        let (input, content) =
            delimited(tag("<action>"), take_until("</action>"), tag("</action>"))(input)?;

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
                None => {
                    continue;
                }
            }
        }
        drop(reg_borrow);

        if matches.len() > 1 || matches.len() == 0 {
            None
        } else {
            Some(matches.first().unwrap().clone())
        }
    }

    // Helper function to apply fixes based on the path
    pub fn fix_json_value(&self, json: &mut JsonValue, path: &[PathSegment]) -> Result<(), String> {
        if path.is_empty() {
            return Ok(()); // Empty path means no fixes needed
        }

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
                        println!("{:?}", map.keys());
                        // Try each possible union type and its validation errors
                        for (type_name, errors) in validation_errors {
                            // Clone the object to try fixes without modifying original
                            let mut test_json = json.clone();

                            // Get a new path from these errors to try fixing
                            let error_path = self.get_error_location(errors);

                            // Try to fix the validation errors for this type
                            let res = self.fix_json_value(&mut test_json, &error_path);
                            println!("{:?}", res);
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

    pub fn validate_and_fix(&self, json: &mut JsonValue) -> Result<(), String> {
        let mut c = 0;

        loop {
            match self.validate_json(&json.to_string()) {
                Ok(_) => return Ok(()),
                Err((template, variable, err)) => {
                    if c > 2 {
                        return Ok(());
                    }

                    let mut path = self.get_error_location(&err);
                    path.insert(0, PathSegment::Root((template, variable)));

                    self.fix_json_value(json, &path)?;

                    c += 1;
                }
            }
        }
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
                        let namespaced_id = self.lookup_symbol_in_registry(&element_type_str);
                        if element_type_str != "String"
                            && element_type_str != "Number"
                            && namespaced_id.is_err()
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
                    let namespaced_id = self.lookup_symbol_in_registry(&type_name);
                    if type_name != "String" && type_name != "Number" && namespaced_id.is_err() {
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

    fn lookup_template_in_registry(
        &self,
        name: &str,
    ) -> Result<WAILTemplateDef<'a>, WAILParseError> {
        let mut matches = vec![];

        {
            for (key, def) in self.template_registry.borrow().iter() {
                if let Some(actual_name) = key.split('.').last() {
                    if actual_name == name {
                        matches.push((key.clone(), def.clone()));
                    }
                }
            }
        }

        match matches.len() {
            0 => Err(WAILParseError::SymbolNotFound {
                name: name.to_string(),
            }),
            1 => Ok(matches.remove(0).1),
            _ => Err(WAILParseError::AmbiguousSymbol {
                name: name.to_string(),
                matches: matches.into_iter().map(|(k, _)| k).collect(),
            }),
        }
    }

    fn lookup_adhoc_obj_in_registry(&self, name: &str) -> Result<WAILObject, WAILParseError> {
        let mut matches = vec![];

        {
            for (key, def) in self.adhoc_obj_refs.borrow().iter() {
                if let Some(actual_name) = key.split('.').last() {
                    if actual_name == name {
                        matches.push((key.clone(), def.clone()));
                    }
                }
            }
        }

        match matches.len() {
            0 => Err(WAILParseError::SymbolNotFound {
                name: name.to_string(),
            }),
            1 => Ok(matches.remove(0).1),
            _ => Err(WAILParseError::AmbiguousSymbol {
                name: name.to_string(),
                matches: matches.into_iter().map(|(k, _)| k).collect(),
            }),
        }
    }

    fn lookup_symbol_in_registry(&self, name: &str) -> Result<WAILField<'a>, WAILParseError> {
        let mut matches = vec![];

        {
            for (key, def) in self.registry.borrow().iter() {
                if let Some(actual_name) = key.split('.').last() {
                    if actual_name == name {
                        matches.push((key.clone(), def.clone()));
                    }
                }
            }
        }

        match matches.len() {
            0 => Err(WAILParseError::SymbolNotFound {
                name: name.to_string(),
            }),
            1 => Ok(matches.remove(0).1.clone()),
            _ => Err(WAILParseError::AmbiguousSymbol {
                name: name.to_string(),
                matches: matches.into_iter().map(|(k, _)| k).collect(),
            }),
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

        drop(template_registry);
        drop(registry);

        (warnings, errors)
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

    // Add this method to use our token-based parser
    pub fn parse_wail_file_with_tokens(
        &'a self,
        input_string: String,
        file_type: WAILFileType,
        clear: bool,
    ) -> Result<Vec<WAILDefinition<'a>>, WAILParseError> {
        let input: &str = Box::leak(Box::new(input_string));

        if clear {
            self.registry.replace(HashMap::new());
            self.template_registry.replace(HashMap::new());
            self.main.take();
            self.object_instances.replace(HashMap::new());
            self.adhoc_obj_ids.replace(Vec::new());
            self.adhoc_obj_refs.replace(HashMap::new());
        }

        let mut parser = self.incremental_parser(input);

        let definitions = parser.parse_wail_file(file_type)?;

        Ok(definitions)
    }

    // Replace the original parse_wail_file method to use our token-based implementation
    pub fn parse_wail_file(
        &'a self,
        input_string: String,
        file_type: WAILFileType,
        clear: bool,
    ) -> Result<Vec<WAILDefinition<'a>>, WAILParseError> {
        self.parse_wail_file_with_tokens(input_string, file_type, clear)
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
        let mut incremental = parser.incremental_parser(input);

        let object_def = incremental.parse_object().unwrap();

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
    fn test_array_with_annotations() {
        let input = r#"
        object Test {
            items: String[] @description("An array of strings")
            mix: Number[] @description("Array with numbers") #Comment
        }
        "#;

        let test_dir = std::env::current_dir().unwrap();
        let parser = WAILParser::new(test_dir);

        match parser.parse_wail_file(input.to_string(), WAILFileType::Library, true) {
            Ok(definitions) => {
                match &definitions[0] {
                    WAILDefinition::Object(field) => {
                        if let WAILType::Composite(WAILCompositeType::Object(obj)) =
                            &field.field_type
                        {
                            let fields = obj.type_data.field_definitions.as_ref().unwrap();

                            // Add debug prints
                            println!("Number of fields: {}", fields.len());
                            for (i, field) in fields.iter().enumerate() {
                                println!(
                                    "Field {}: {} annotations: {}",
                                    i,
                                    field.name,
                                    field.annotations.len()
                                );
                                for ann in &field.annotations {
                                    println!("  Annotation: {:?}", ann);
                                }
                            }

                            // First field test
                            assert_eq!(fields[0].name, "items");
                            assert!(matches!(
                                fields[0].field_type,
                                WAILType::Composite(WAILCompositeType::Array(_))
                            ));
                            assert_eq!(
                                fields[0].annotations.len(),
                                1,
                                "items field should have 1 annotation"
                            );

                            // Second field test
                            assert_eq!(fields[1].name, "mix");
                            assert!(matches!(
                                fields[1].field_type,
                                WAILType::Composite(WAILCompositeType::Array(_))
                            ));
                            assert_eq!(
                                fields[1].annotations.len(),
                                1,
                                "mix field should have 1 annotation"
                            );
                        } else {
                            panic!("Expected object type");
                        }
                    }
                    _ => panic!("Expected object definition"),
                }
            }
            Err(e) => panic!("Failed to parse: {:?}", e),
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

        let mut incremental = parser.incremental_parser(input);

        let template_def = incremental.parse_template().unwrap();

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

        let mut incremental = parser.incremental_parser(input);
        let template_def = incremental.parse_template().unwrap();

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

        // Use incremental parser for template
        let mut incremental = parser.incremental_parser(input);
        incremental.parse_template().unwrap();

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

        // Use incremental parser for object
        let mut incremental = parser.incremental_parser(type_def);
        incremental.parse_object().unwrap();

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
   <action>
   "hello"
   </action>
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
         <action>
         "hello"
         </action>
         Second result:
         <action>
         "world"
         </action>
         "#;

        let res = parser.parse_llm_output(multiple_fences);
        println!("{:?}", res);
        assert!(res.is_ok());

        // Test mixed traditional and fence
        let mixed = r#"
         Fence:
         <action>
         "world"
         </action>
         <action>
         "world"
         </action>
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
        <action>
         43
        </action>
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
         <action>
         [1, 2, 3]
         </action>
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

   let prompt_stmnt = GeneratePrompt(conversation: conversation);

   prompt {
      Create a response that will be delivered through both text and speech:
      {{prompt_stmnt}}
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
        assert_eq!(var2, "prompt_stmnt");
        assert_eq!(call2.template_name, "GeneratePrompt");
        assert_eq!(call2.arguments.len(), 1);
    }

    #[test]
    fn test_parse_imports() {
        let input = r#"
    import { Person, Address, Thing } from "types.lib.wail"
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

            union Thing = Person | Address;
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
        println!("{:?}", definitions);
        match &definitions[0] {
            WAILDefinition::Import(import) => {
                assert_eq!(import.items, vec!["Person", "Address", "Thing"]);
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

        // Test duplicate object definition (first wins)
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

        let _result = parser
            .parse_wail_file(input.to_string(), WAILFileType::Application, true)
            .unwrap();

        // Ensure the first definition was used
        let registry = parser.registry.borrow();
        let person = registry.get("Person").expect("Person not found");

        if let WAILType::Composite(WAILCompositeType::Object(obj)) = &person.field_type {
            let fields = obj.type_data.field_definitions.as_ref().unwrap();
            let field_names: Vec<_> = fields.iter().map(|f| f.name.as_str()).collect();
            assert!(field_names.contains(&"name"));
            assert!(!field_names.contains(&"age")); // second definition was ignored
        } else {
            panic!("Person should be an object type");
        }

        drop(registry);
        // Test missing main block
        let input = r#"
      object Person {
            name: String
      }
      "#;

        let err = parser
            .parse_wail_file(input.to_string(), WAILFileType::Application, true)
            .unwrap_err();

        println!("HERER");
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

        assert!(matches!(err, WAILParseError::UnexpectedToken { found, .. } if found == "@"));

        //     // Test EOF
        //     let input = r#"
        //     object Person {
        //         name: String
        // "#;

        //     let err = parser.parse_wail_file(input).unwrap_err();
        //     println!("{:?}", err);
        //     assert!(matches!(err, WAILParseError::UnexpectedEOF { .. }));
    }
    #[test]
    fn test_resolve_imports_comprehensive() {
        use std::fs;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();

        // Create types.lib.wail with objects and a union
        fs::write(
            tmp.path().join("types.lib.wail"),
            r#"
        object Person {
            name: String
            age: Number
        }
        
        object Address {
            street: String
            city: String
            country: String
        }
        
        union ContactInfo = Person | Address;
        "#,
        )
        .unwrap();

        // Create templates.lib.wail with templates that use the types
        fs::write(
            tmp.path().join("templates.lib.wail"),
            r#"
        import { Person, Address, ContactInfo } from "types.lib.wail"
        
        template GetPerson(name: String, age: Number) -> Person {
            prompt: """
            Create a person with name {{name}} and age {{age}}.
            {{return_type}}
            """
        }
        
        template GetContactInfo(info: String) -> ContactInfo {
            prompt: """
            Parse this info: {{info}}
            Return in this format: {{return_type}}
            """
        }
        "#,
        )
        .unwrap();

        // Create nested.lib.wail that imports from templates.lib.wail
        fs::write(
            tmp.path().join("nested.lib.wail"),
            r#"
        import { GetPerson } from "templates.lib.wail"
        
        object ExtendedPerson {
            person: Person
            notes: String
        }
        "#,
        )
        .unwrap();

        // Create main file that imports from all libraries
        let input = r#"
    import { Person, ContactInfo } from "types.lib.wail"
    import { GetContactInfo } from "templates.lib.wail"
    import { ExtendedPerson } from "nested.lib.wail"
    
    main {
        let person_info = GetContactInfo(info: "John Doe, 30 years old");
        
        prompt {
            Parse this information: {{person_info}}
        }
    }
    "#;

        let parser = WAILParser::new(tmp.path().to_path_buf());
        let result = parser.parse_wail_file(input.to_string(), WAILFileType::Application, true);
        assert!(
            result.is_ok(),
            "Failed to parse imports: {:?}",
            result.err()
        );

        // Verify all types were imported correctly
        let registry = parser.registry.borrow();
        let template_registry = parser.template_registry.borrow();

        println!("{:?}", registry.keys());

        // Check objects
        assert!(
            registry.contains_key("types.lib.wail.Person"),
            "Person object not imported"
        );
        assert!(
            registry.contains_key("types.lib.wail.ContactInfo"),
            "ContactInfo union not imported"
        );
        assert!(
            registry.contains_key("nested.lib.wail.ExtendedPerson"),
            "ExtendedPerson not imported"
        );

        // Check that Person has the right fields
        if let WAILType::Composite(WAILCompositeType::Object(obj)) =
            &registry.get("types.lib.wail.Person").unwrap().field_type
        {
            let fields = obj.type_data.field_definitions.as_ref().unwrap();
            assert_eq!(fields.len(), 3); // name, age, _type

            let field_names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
            assert!(field_names.contains(&"name".to_string()));
            assert!(field_names.contains(&"age".to_string()));
        } else {
            panic!("Person should be an object type");
        }

        // Check that ContactInfo is a union with Person and Address
        if let WAILType::Composite(WAILCompositeType::Union(union)) = &registry
            .get("types.lib.wail.ContactInfo")
            .unwrap()
            .field_type
        {
            assert_eq!(union.members.len(), 2);

            // Verify the union members (could be in any order)
            let member_types: Vec<String> = union
                .members
                .iter()
                .map(|m| match &m.field_type {
                    WAILType::Composite(WAILCompositeType::Object(obj)) => {
                        obj.type_data.type_name.to_string()
                    }
                    _ => "unknown".to_string(),
                })
                .collect();

            assert!(
                member_types.contains(&"Person".to_string()),
                "Union doesn't contain Person"
            );
            assert!(
                member_types.contains(&"Address".to_string()),
                "Union doesn't contain Address"
            );
        } else {
            panic!("ContactInfo should be a union type");
        }

        // Check that ExtendedPerson references Person
        if let WAILType::Composite(WAILCompositeType::Object(obj)) = &registry
            .get("nested.lib.wail.ExtendedPerson")
            .unwrap()
            .field_type
        {
            let fields = obj.type_data.field_definitions.as_ref().unwrap();
            assert_eq!(fields.len(), 3); // person, notes, _type

            // Check that person field is of type Person
            let person_field = fields
                .iter()
                .find(|f| f.name == "person")
                .expect("No person field found");
            if let WAILType::Composite(WAILCompositeType::Object(pers_obj)) =
                &person_field.field_type
            {
                assert_eq!(pers_obj.type_data.type_name, "Person");
            } else {
                panic!("person field should be of type Person");
            }
        } else {
            panic!("ExtendedPerson should be an object type");
        }

        // Check templates
        assert!(
            template_registry.contains_key("templates.lib.wail.GetContactInfo"),
            "GetContactInfo template not imported"
        );

        assert!(
            !template_registry.contains_key("GetPerson"),
            "GetPerson template should not be imported directly"
        );

        // Check that GetContactInfo returns ContactInfo
        let template = template_registry
            .get("templates.lib.wail.GetContactInfo")
            .unwrap();

        println!("{:?}", template.output.field_type);
        if let WAILType::Composite(WAILCompositeType::Union(_)) = &template.output.field_type {
            // Good, it's a union
        } else {
            panic!("GetContactInfo should return a union type");
        }

        // Check import chain was handled correctly
        let import_chain = parser.import_chain.borrow();
        assert!(
            import_chain.stack.is_empty(),
            "Import stack should be empty after parsing"
        );
    }
}
