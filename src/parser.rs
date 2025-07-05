use log::debug;
use pyo3::prelude::*;
use pyo3::types::PyString;

use crate::python_types::PyTypeInfo;
use crate::tag_finder::{Tag, TagFinder};

#[derive(Debug, Clone)]
enum StackFrame {
    List {
        tag_name: String,
        items: Vec<PyObject>,
        item_type: PyTypeInfo,
        depth: usize,
        implicit: bool,
    },
    Dict {
        tag_name: String,
        entries: Vec<(PyObject, PyObject)>,
        key_type: Option<PyTypeInfo>,
        value_type: Option<PyTypeInfo>,
        current_key: Option<PyObject>,
        depth: usize,
    },
    Set {
        tag_name: String,
        items: Vec<PyObject>,
        item_type: PyTypeInfo,
        depth: usize,
    },
    Tuple {
        tag_name: String,
        items: Vec<PyObject>,
        types: Vec<PyTypeInfo>,
        depth: usize,
    },
    Object {
        tag_name: String,
        type_info: PyTypeInfo,
        instance: PyObject,
        current_field: Option<String>,
        depth: usize,
    },
    Field {
        name: String,
        content: String,
        type_info: PyTypeInfo,
        depth: usize,
    },
}

/// Wrapper for the StreamParser that handles typed conversions
#[derive(Debug)]
pub struct TypedStreamParser {
    tag_finder: TagFinder,
    type_info: Option<PyTypeInfo>,
    is_done: bool,
    stack: Vec<StackFrame>,
    stack_based_result: Option<PyObject>,
    depth: usize,
}

impl TypedStreamParser {
    pub fn new(wanted_tags: Vec<String>, ignored_tags: Vec<String>) -> Self {
        Self {
            tag_finder: TagFinder::new_with_filter(wanted_tags, ignored_tags),
            type_info: None,
            is_done: false,
            stack: Vec::new(),
            stack_based_result: None,
            depth: 0,
        }
    }

    pub fn with_type(
        type_info: PyTypeInfo,
        wanted_tags: Vec<String>,
        ignored_tags: Vec<String>,
    ) -> Self {
        Self {
            tag_finder: TagFinder::new_with_filter(wanted_tags, ignored_tags),
            type_info: Some(type_info),
            is_done: false,
            stack: Vec::new(),
            stack_based_result: None,
            depth: 0,
        }
    }

    fn should_use_stack(&self) -> bool {
        if let Some(type_info) = &self.type_info {
            matches!(
                type_info.kind,
                crate::python_types::PyTypeKind::List
                    | crate::python_types::PyTypeKind::Dict
                    | crate::python_types::PyTypeKind::Set
                    | crate::python_types::PyTypeKind::Tuple
                    | crate::python_types::PyTypeKind::Class
                    | crate::python_types::PyTypeKind::Union
            )
        } else {
            false
        }
    }

    fn frame_to_pyobject(&self, frame: StackFrame) -> PyResult<PyObject> {
        pyo3::Python::with_gil(|py| {
            match frame {
                StackFrame::List { items, .. } => Ok(pyo3::types::PyList::new(py, &items).into()),
                StackFrame::Dict { entries, .. } => {
                    let dict = pyo3::types::PyDict::new(py);
                    for (key, value) in entries {
                        dict.set_item(key, value)?;
                    }
                    Ok(dict.into())
                }
                StackFrame::Set { items, .. } => {
                    let set = pyo3::types::PySet::new(py, &items)?;
                    Ok(set.into())
                }
                StackFrame::Tuple { items, .. } => {
                    let tuple = pyo3::types::PyTuple::new(py, &items);
                    Ok(tuple.into())
                }
                StackFrame::Object { instance, .. } => Ok(instance),
                StackFrame::Field {
                    content, type_info, ..
                } => {
                    // Convert content to the appropriate primitive type
                    match type_info.kind {
                        crate::python_types::PyTypeKind::String => {
                            // Decode HTML entities for strings
                            let decoded = content
                                .replace("&amp;", "&")
                                .replace("&lt;", "<")
                                .replace("&gt;", ">")
                                .replace("&quot;", "\"")
                                .replace("&#39;", "'")
                                .replace("&apos;", "'");
                            Ok(decoded.into_py(py))
                        }
                        crate::python_types::PyTypeKind::Integer => match content.parse::<i64>() {
                            Ok(val) => Ok(val.into_py(py)),
                            Err(_) => Ok(py.None()),
                        },
                        crate::python_types::PyTypeKind::Float => match content.parse::<f64>() {
                            Ok(val) => Ok(val.into_py(py)),
                            Err(_) => Ok(py.None()),
                        },
                        crate::python_types::PyTypeKind::Boolean => {
                            let val =
                                matches!(content.to_lowercase().as_str(), "true" | "1" | "yes");
                            Ok(val.into_py(py))
                        }
                        crate::python_types::PyTypeKind::None => Ok(py.None()),
                        _ => Ok(py.None()),
                    }
                }
            }
        })
    }

    fn push_frame_for_type(
        &mut self,
        type_info: &PyTypeInfo,
        tag_name: &str,
        depth: usize,
    ) -> PyResult<()> {
        if type_info.kind == crate::python_types::PyTypeKind::Optional {
            let inner_type = type_info
                .args
                .get(0)
                .cloned()
                .unwrap_or_else(PyTypeInfo::any);
            let none_type =
                PyTypeInfo::new(crate::python_types::PyTypeKind::None, "None".to_string())
                    .with_module("builtins".to_string());
            let union_type =
                PyTypeInfo::new(crate::python_types::PyTypeKind::Union, "Union".to_string())
                    .with_module("typing".to_string())
                    .with_args(vec![inner_type, none_type]);
            return self.push_frame_for_type(&union_type, tag_name, depth);
        }
        // Primitive types should be represented as a simple Field frame, not a
        // structural container frame.  Pushing a dedicated frame for a primitive
        // like `Float` or `String` would later cause “Cannot create frame for
        // primitive type …” errors when the parser tries to instantiate it as an
        // object.  Instead, store an empty‐content Field here so nested text
        // bytes get appended in `handle_stack_bytes`.
        if type_info.is_primitive() {
            self.stack.push(StackFrame::Field {
                name: tag_name.to_string(),
                content: String::new(),
                type_info: type_info.clone(),
                depth,
            });
            return Ok(());
        }
        let frame = pyo3::Python::with_gil(|py| match type_info.kind {
            crate::python_types::PyTypeKind::List => {
                let item_type = type_info
                    .args
                    .get(0)
                    .cloned()
                    .unwrap_or_else(PyTypeInfo::any);
                Ok(Some(StackFrame::List {
                    tag_name: tag_name.to_string(),
                    items: Vec::new(),
                    item_type,
                    depth,
                    implicit: false,
                }))
            }
            crate::python_types::PyTypeKind::Dict => {
                let key_type = type_info.args.get(0).cloned();
                let value_type = type_info.args.get(1).cloned();
                Ok(Some(StackFrame::Dict {
                    tag_name: tag_name.to_string(),
                    entries: Vec::new(),
                    key_type,
                    value_type,
                    current_key: None,
                    depth,
                }))
            }
            crate::python_types::PyTypeKind::Set => {
                let item_type = type_info
                    .args
                    .get(0)
                    .cloned()
                    .unwrap_or_else(PyTypeInfo::any);
                Ok(Some(StackFrame::Set {
                    tag_name: tag_name.to_string(),
                    items: Vec::new(),
                    item_type,
                    depth,
                }))
            }
            crate::python_types::PyTypeKind::Tuple => Ok(Some(StackFrame::Tuple {
                tag_name: tag_name.to_string(),
                items: Vec::new(),
                types: type_info.args.clone(),
                depth,
            })),
            crate::python_types::PyTypeKind::Class => {
                let instance = if let Some(py_type) = &type_info.py_type {
                    if py_type
                        .as_ref(py)
                        .hasattr("__gasp_from_partial__")
                        .unwrap_or(false)
                    {
                        let empty_dict = pyo3::types::PyDict::new(py);
                        py_type
                            .as_ref(py)
                            .call_method1("__gasp_from_partial__", (empty_dict,))?
                    } else {
                        py_type.as_ref(py).call0()?
                    }
                } else {
                    return Err(pyo3::exceptions::PyTypeError::new_err(
                        "Cannot instantiate class without py_type",
                    ));
                };
                Ok(Some(StackFrame::Object {
                    tag_name: tag_name.to_string(),
                    type_info: type_info.clone(),
                    instance: instance.into(),
                    current_field: None,
                    depth,
                }))
            }
            crate::python_types::PyTypeKind::Union => {
                // For unions, we don't create a frame immediately.
                // We wait for a specific member tag to be identified.
                Ok(None)
            }
            _ => Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "Cannot create frame for primitive type {:?}",
                type_info.kind
            ))),
        })?;
        if let Some(frame) = frame {
            self.stack.push(frame);
        }
        Ok(())
    }

    fn create_type_info_from_string(&self, type_str: &str) -> PyResult<PyTypeInfo> {
        let (kind, name) = match type_str {
            "int" => (crate::python_types::PyTypeKind::Integer, "int"),
            "str" | "string" => (crate::python_types::PyTypeKind::String, "str"),
            "float" => (crate::python_types::PyTypeKind::Float, "float"),
            "bool" | "boolean" => (crate::python_types::PyTypeKind::Boolean, "bool"),
            "list" => (crate::python_types::PyTypeKind::List, "list"),
            "dict" => (crate::python_types::PyTypeKind::Dict, "dict"),
            "set" => (crate::python_types::PyTypeKind::Set, "set"),
            "tuple" => (crate::python_types::PyTypeKind::Tuple, "tuple"),
            _ => {
                // Default to string for unknown types
                (crate::python_types::PyTypeKind::String, "str")
            }
        };

        let type_info = PyTypeInfo::new(kind, name.to_string()).with_module("builtins".to_string());

        Ok(type_info)
    }

    fn build_current_intermediate_state(&self) -> PyResult<Option<PyObject>> {
        if self.stack.is_empty() {
            return Ok(None);
        }

        let mut temp_stack = self.stack.clone();

        while temp_stack.len() > 1 {
            let top_frame = temp_stack.pop().unwrap();
            let py_object = self.frame_to_pyobject(top_frame)?;

            if let Some(parent_frame) = temp_stack.last_mut() {
                match parent_frame {
                    StackFrame::List { items, .. } => items.push(py_object),
                    StackFrame::Set { items, .. } => items.push(py_object),
                    StackFrame::Tuple { items, .. } => items.push(py_object),
                    StackFrame::Object {
                        instance,
                        current_field,
                        ..
                    } => {
                        if let Some(field_name) = current_field.take() {
                            pyo3::Python::with_gil(|py| {
                                let _ = instance.as_ref(py).setattr(field_name.as_str(), py_object);
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        self.frame_to_pyobject(temp_stack[0].clone()).map(Some)
    }

    fn is_inside_container(&self) -> bool {
        // Check if the current context is directly inside a container (List, Set, Tuple, or Dict)
        if let Some(frame) = self.stack.last() {
            matches!(
                frame,
                StackFrame::List { .. }
                    | StackFrame::Set { .. }
                    | StackFrame::Tuple { .. }
                    | StackFrame::Dict { .. }
            )
        } else {
            false
        }
    }

    fn handle_stack_tag_open(&mut self, tag: &Tag) -> PyResult<()> {
        let tag_name = &tag.name;
        debug!(
            "handle_stack_tag_open: tag_name={}, stack_len={}",
            tag_name,
            self.stack.len()
        );

        // Coercion logic: if we expect a list but get an inner object type, implicitly wrap it in a list.
        let type_info_clone = self.type_info.clone();
        if self.stack.is_empty() {
            if let Some(type_info) = type_info_clone {
                if type_info.kind == crate::python_types::PyTypeKind::List {
                    if let Some(item_type) = type_info.args.get(0) {
                        // Check if the tag name matches a type that should be in the list
                        let should_create_implicit_list = match &item_type.kind {
                            crate::python_types::PyTypeKind::Union => {
                                // For Union types, check if the tag matches any union member
                                item_type.args.iter().any(|t| &t.name == tag_name)
                            }
                            crate::python_types::PyTypeKind::List => {
                                // Can't disambiguate if this is the inner or outer list, so don't allow
                                false
                            }
                            _ => {
                                // For non-union types, check if the tag matches the expected item type
                                &item_type.name == tag_name
                            }
                        };

                        if should_create_implicit_list {
                            debug!(
                                "Implicitly creating list frame for type: {} with item type: {}",
                                type_info.name, item_type.name
                            );
                            // Implicitly create the list frame
                            self.stack.push(StackFrame::List {
                                tag_name: "list".to_string(),
                                items: Vec::new(),
                                item_type: item_type.clone(),
                                depth: tag.depth - 1,
                                implicit: true,
                            });
                        }
                    }
                }
            }
        }

        // Debug: print current stack state
        for (i, frame) in self.stack.iter().enumerate() {
            match frame {
                StackFrame::Object {
                    type_info,
                    current_field,
                    ..
                } => {
                    debug!(
                        "  Stack[{}]: Object(type={}, current_field={:?})",
                        i, type_info.name, current_field
                    );
                }
                StackFrame::Field { name, .. } => {
                    debug!("  Stack[{}]: Field(name={})", i, name);
                }
                _ => {
                    debug!("  Stack[{}]: {:?}", i, frame);
                }
            }
        }

        // Determine what type of frame to create based on the current stack top.
        // This is done by peeking at the stack without a long-lived mutable borrow.
        let mut next_type_info = if let Some(frame) = self.stack.last() {
            match frame {
                StackFrame::Object { type_info, .. } => {
                    if let Some(field_info) = type_info.fields.get(tag_name) {
                        let field_info =
                            if field_info.kind == crate::python_types::PyTypeKind::Optional {
                                // Convert Optional[T] to Union[T, None]
                                let inner_type = field_info
                                    .args
                                    .get(0)
                                    .cloned()
                                    .unwrap_or_else(|| PyTypeInfo::any());
                                let none_type = PyTypeInfo::new(
                                    crate::python_types::PyTypeKind::None,
                                    "None".to_string(),
                                )
                                .with_module("builtins".to_string());

                                PyTypeInfo::new(
                                    crate::python_types::PyTypeKind::Union,
                                    "Union".to_string(),
                                )
                                .with_module("typing".to_string())
                                .with_args(vec![inner_type, none_type])
                            } else {
                                field_info.clone()
                            };
                        // If the field is a union type, check the type attribute
                        if field_info.kind == crate::python_types::PyTypeKind::Union {
                            if let Some(type_attr) = tag.attributes.get("type") {
                                field_info
                                    .args
                                    .iter()
                                    .find(|t| {
                                        &t.name == type_attr
                                            || type_attr.starts_with(&t.name)
                                            || t.name.starts_with(type_attr)
                                    })
                                    .cloned()
                                    .or(Some(field_info))
                            } else {
                                Some(field_info)
                            }
                        } else {
                            Some(field_info)
                        }
                    } else if type_info.kind == crate::python_types::PyTypeKind::Union {
                        type_info.args.iter().find(|t| t.name == *tag_name).cloned()
                    } else {
                        None
                    }
                }
                StackFrame::List { item_type, .. } => {
                    // For lists, we need to check if the tag matches the expected item type
                    let matches_list_item = if tag_name == "item" {
                        true
                    } else if item_type.kind == crate::python_types::PyTypeKind::Union {
                        // For Union types, check if tag matches any union member
                        item_type.args.iter().any(|t| &t.name == tag_name)
                    } else {
                        // For non-union types, check direct match
                        &item_type.name == tag_name
                    };

                    if matches_list_item {
                        // Check if the item_type is a Union and if so, use the type attribute
                        let item_type =
                            if item_type.kind == crate::python_types::PyTypeKind::Optional {
                                // Convert Optional[T] to Union[T, None]
                                let inner_type = item_type
                                    .args
                                    .get(0)
                                    .cloned()
                                    .unwrap_or_else(|| PyTypeInfo::any());
                                let none_type = PyTypeInfo::new(
                                    crate::python_types::PyTypeKind::None,
                                    "None".to_string(),
                                )
                                .with_module("builtins".to_string());

                                PyTypeInfo::new(
                                    crate::python_types::PyTypeKind::Union,
                                    "Union".to_string(),
                                )
                                .with_module("typing".to_string())
                                .with_args(vec![inner_type, none_type])
                            } else {
                                item_type.clone()
                            };

                        if item_type.kind == crate::python_types::PyTypeKind::Union {
                            if let Some(type_attr) = tag.attributes.get("type") {
                                item_type
                                    .args
                                    .iter()
                                    .find(|t| {
                                        let tattr = type_attr.as_str();
                                        &t.name == tattr
                                            || (t.name == "None"
                                                && (tattr == "None" || tattr == "NoneType"))
                                            || tattr.starts_with(&t.name)
                                            || t.name.starts_with(tattr)
                                    })
                                    .cloned()
                            } else {
                                // If no type attribute, try to match by tag name
                                item_type
                                    .args
                                    .iter()
                                    .find(|t| &t.name == tag_name)
                                    .cloned()
                                    .or(Some(item_type))
                            }
                        } else {
                            Some(item_type.clone())
                        }
                    } else {
                        None
                    }
                }
                StackFrame::Set { item_type, .. } if tag_name == "item" => {
                    // Check if the item_type is a Union and if so, use the type attribute
                    if item_type.kind == crate::python_types::PyTypeKind::Union {
                        if let Some(type_attr) = tag.attributes.get("type") {
                            item_type
                                .args
                                .iter()
                                .find(|t| &t.name == type_attr)
                                .cloned()
                        } else {
                            Some(item_type.clone())
                        }
                    } else {
                        Some(item_type.clone())
                    }
                }
                StackFrame::Tuple { items, types, .. } if tag_name == "item" => {
                    // Check if this is a homogeneous tuple (Tuple[T, ...])
                    if types.len() == 2
                        && types.get(1).map(|t| t.name == "Ellipsis").unwrap_or(false)
                    {
                        // For homogeneous tuples, always use the first type
                        types.get(0).cloned()
                    } else {
                        // For fixed tuples, get the type for the current position
                        types.get(items.len()).cloned()
                    }
                }
                StackFrame::Dict { value_type, .. } if tag_name == "item" => value_type.clone(),
                _ => None,
            }
        } else {
            // Stack is empty, this is the root.
            self.type_info.clone()
        };

        // If we don't have type info or the type is Any, and we're handling an item in a container,
        // check if the tag has a type attribute we can use
        if tag_name == "item" && self.is_inside_container() {
            if next_type_info.is_none()
                || (next_type_info
                    .as_ref()
                    .map(|t| t.kind == crate::python_types::PyTypeKind::Any)
                    .unwrap_or(false))
            {
                if let Some(type_attr) = tag.attributes.get("type") {
                    // Create PyTypeInfo based on the type attribute
                    next_type_info = Some(self.create_type_info_from_string(type_attr)?);
                }
            }
        }

        let mut pushed_new_frame = false;
        if let Some(type_info) = next_type_info {
            let mut should_push = false;
            // Determine the concrete type we should instantiate for this tag.
            //
            // Special-case handling for Optional[T] which the type extractor represents as
            // `Union[T, None]`.  Whenever the Union comprises exactly one real type plus
            // `None`, we can safely pick the non-None member automatically instead of
            // treating the whole tag as an abstract Union (which would otherwise skip
            // frame creation and break scoping).  This prevents the parent object’s
            // `current_field` from remaining stale and receiving values that belong to
            // nested objects.
            // Decide which concrete member of a Union we should instantiate.
            //
            // Order of precedence:
            //   1. If this element specifies a `type="..."` attribute, honour it.
            //   2. If the union is an Optional-style `Union[T, None]`, and no explicit
            //      type attribute is given, select the non-None member automatically.
            //   3. Otherwise, fall back to a tag-name match or keep the union abstract.
            let actual_type = if type_info.kind == crate::python_types::PyTypeKind::Union {
                // a) explicit type attribute
                if let Some(type_attr) = tag.attributes.get("type") {
                    let tattr = type_attr.as_str();
                    type_info
                        .args
                        .iter()
                        .find(|t| {
                            &t.name == tattr
                                || (t.name == "None" && (tattr == "None" || tattr == "NoneType"))
                                || tattr.starts_with(&t.name)
                                || t.name.starts_with(tattr)
                        })
                        .cloned()
                        .unwrap_or(type_info.clone())
                // b) Optional[T] pattern ≅ Union[T, None]
                } else if type_info.args.len() == 2
                    && type_info
                        .args
                        .iter()
                        .any(|t| t.kind == crate::python_types::PyTypeKind::None)
                {
                    type_info
                        .args
                        .iter()
                        .find(|t| t.kind != crate::python_types::PyTypeKind::None)
                        .cloned()
                        .unwrap_or(type_info.clone())
                } else if type_info.args.iter().any(|t| t.name == *tag_name) {
                    // Or, if the tag name itself matches a union member
                    type_info
                        .args
                        .iter()
                        .find(|t| t.name == *tag_name)
                        .cloned()
                        .unwrap_or(type_info.clone())
                } else {
                    // Fallback to treating the union abstractly
                    type_info.clone()
                }
            } else {
                type_info.clone()
            };

            should_push = true;

            if should_push {
                // Push the new frame.
                if actual_type.is_primitive() {
                    self.stack.push(StackFrame::Field {
                        name: tag_name.clone(),
                        content: String::new(),
                        type_info: actual_type,
                        depth: tag.depth,
                    });
                    pushed_new_frame = true;
                } else {
                    self.push_frame_for_type(&actual_type, tag_name, tag.depth)?;
                    pushed_new_frame = true;
                }
            }
        }

        // If we pushed a new frame, we need to update the parent frame's context
        if pushed_new_frame && self.stack.len() > 1 {
            // Look at the frame right before the one we just pushed (the parent)
            let parent_idx = self.stack.len() - 2;
            match self.stack.get_mut(parent_idx) {
                Some(StackFrame::Object {
                    current_field,
                    type_info,
                    ..
                }) => {
                    // Only set current_field if this tag corresponds to a field of the object
                    if type_info.fields.contains_key(tag_name) {
                        debug!(
                            "Setting current_field '{}' on Object frame at index {}",
                            tag_name, parent_idx
                        );
                        *current_field = Some(tag_name.clone());
                    }
                }
                Some(StackFrame::Dict { current_key, .. }) if tag_name == "item" => {
                    // For dict items, store the key from the tag attributes
                    if let Some(key_attr) = tag.attributes.get("key") {
                        pyo3::Python::with_gil(|py| {
                            *current_key = Some(key_attr.clone().into_py(py));
                        });
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn handle_stack_tag_close(&mut self, tag_name: &str, depth: usize) -> PyResult<()> {
        debug!(
            "handle_stack_tag_close: tag_name={}, depth={}, stack_len={}",
            tag_name,
            depth,
            self.stack.len()
        );
        if let Some(top) = self.stack.last() {
            debug!("  top_frame: {:?}", top);
        }

        while let Some(top_frame) = self.stack.last() {
            let (frame_tag_name, frame_depth) = match top_frame {
                StackFrame::List {
                    tag_name, depth, ..
                } => (tag_name.clone(), *depth),
                StackFrame::Dict {
                    tag_name, depth, ..
                } => (tag_name.clone(), *depth),
                StackFrame::Set {
                    tag_name, depth, ..
                } => (tag_name.clone(), *depth),
                StackFrame::Tuple {
                    tag_name, depth, ..
                } => (tag_name.clone(), *depth),
                StackFrame::Object {
                    tag_name, depth, ..
                } => (tag_name.clone(), *depth),
                StackFrame::Field { name, depth, .. } => (name.clone(), *depth),
            };

            if frame_depth > depth {
                // This is a child of the current closing tag, which was not properly closed.
                // We should pop it off and integrate it into its parent.
                let child_frame = self.stack.pop().unwrap();
                let child_object = self.frame_to_pyobject(child_frame)?;

                if let Some(parent_frame) = self.stack.last_mut() {
                    match parent_frame {
                        StackFrame::List { items, .. } => items.push(child_object),
                        StackFrame::Set { items, .. } => items.push(child_object),
                        StackFrame::Tuple { items, .. } => items.push(child_object),
                        StackFrame::Dict {
                            entries,
                            current_key,
                            ..
                        } => {
                            if let Some(key) = current_key.take() {
                                entries.push((key, child_object));
                            }
                        }
                        StackFrame::Object {
                            instance,
                            current_field,
                            ..
                        } => {
                            if let Some(field_name) = current_field.take() {
                                pyo3::Python::with_gil(|py| {
                                    let _ = instance
                                        .as_ref(py)
                                        .setattr(field_name.as_str(), child_object);
                                });
                            }
                        }
                        _ => {}
                    }
                }
            } else if frame_depth == depth
                && frame_tag_name.to_lowercase() == tag_name.to_lowercase()
            {
                // This is the matching frame for the closing tag.
                let child_frame = self.stack.pop().unwrap();
                let child_object = self.frame_to_pyobject(child_frame)?;

                if let Some(parent_frame) = self.stack.last_mut() {
                    match parent_frame {
                        StackFrame::List { items, .. } => items.push(child_object),
                        StackFrame::Set { items, .. } => items.push(child_object),
                        StackFrame::Tuple { items, .. } => items.push(child_object),
                        StackFrame::Dict {
                            entries,
                            current_key,
                            ..
                        } => {
                            if let Some(key) = current_key.take() {
                                entries.push((key, child_object));
                            }
                        }
                        StackFrame::Object {
                            instance,
                            current_field,
                            ..
                        } => {
                            if let Some(field_name) = current_field.take() {
                                pyo3::Python::with_gil(|py| {
                                    let _ = instance
                                        .as_ref(py)
                                        .setattr(field_name.as_str(), child_object);
                                });
                            }
                        }
                        _ => {}
                    }
                } else {
                    // No parent, this is the root object.
                    self.stack_based_result = Some(child_object);
                    self.is_done = true;
                }
                break; // We've handled the closing tag, so we can exit the loop.
            } else {
                // The top frame is at a lower depth, so we should not pop it.
                break;
            }
        }

        // If we just closed an item and the stack now contains only a single, implicitly created list frame,
        // it implies that the list is done.
        if self.stack.len() == 1 {
            if let Some(StackFrame::List { implicit, .. }) = self.stack.last() {
                if *implicit {
                    let frame = self.stack.pop().unwrap();
                    let obj = self.frame_to_pyobject(frame)?;
                    self.stack_based_result = Some(obj);
                    self.is_done = true;
                }
            }
        }

        Ok(())
    }

    fn handle_stack_bytes(&mut self, content: &str) -> PyResult<()> {
        if let Some(StackFrame::Field {
            content: field_content,
            ..
        }) = self.stack.last_mut()
        {
            field_content.push_str(content);
        }
        Ok(())
    }

    pub fn step(&mut self, chunk: &str) -> PyResult<Option<PyObject>> {
        let mut events = Vec::new();
        let events_ref = &mut events;
        self.tag_finder
            .push(chunk, |event| {
                debug!("Callback received event: {:?}", event);
                events_ref.push(event);
                Ok(())
            })
            .map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Tag parsing error: {:?}", e))
            })?;

        debug!("step: chunk={:?}, collected events={:?}", chunk, events);

        if self.should_use_stack() {
            for event in &events {
                match event {
                    crate::tag_finder::TagEvent::Open(tag) => {
                        self.depth = tag.depth;
                        self.handle_stack_tag_open(tag)?
                    }
                    crate::tag_finder::TagEvent::Close(name, depth) => {
                        self.depth = *depth;
                        self.handle_stack_tag_close(name, *depth)?
                    }
                    crate::tag_finder::TagEvent::Bytes(content) => {
                        self.handle_stack_bytes(content)?
                    }
                }
            }
            if self.is_done {
                return Ok(self.stack_based_result.clone());
            }
            return self.build_current_intermediate_state();
        }

        // Handle primitive types that don't use the stack
        if let Some(type_info) = &self.type_info {
            if type_info.is_primitive() {
                // For primitive types, we need a simple tag + content structure
                for event in &events {
                    match event {
                        crate::tag_finder::TagEvent::Open(tag) => {
                            if tag.name.to_lowercase() == type_info.name.to_lowercase()
                                && self.stack.is_empty()
                            {
                                // Start collecting content for this primitive
                                self.stack.push(StackFrame::Field {
                                    name: tag.name.clone(),
                                    content: String::new(),
                                    type_info: type_info.clone(),
                                    depth: tag.depth,
                                });
                            }
                        }
                        crate::tag_finder::TagEvent::Bytes(content) => {
                            if let Some(StackFrame::Field {
                                content: field_content,
                                ..
                            }) = self.stack.last_mut()
                            {
                                field_content.push_str(content);
                            }
                        }
                        crate::tag_finder::TagEvent::Close(name, _) => {
                            if name.to_lowercase() == type_info.name.to_lowercase()
                                && !self.stack.is_empty()
                            {
                                if let Some(frame) = self.stack.pop() {
                                    let result = self.frame_to_pyobject(frame)?;
                                    self.stack_based_result = Some(result.clone());
                                    self.is_done = true;
                                    return Ok(Some(result));
                                }
                            }
                        }
                    }
                }

                // Return partial results for primitives
                if !self.stack.is_empty() {
                    if let Some(StackFrame::Field {
                        content,
                        type_info,
                        depth,
                        ..
                    }) = self.stack.last()
                    {
                        // Build a partial result from the current content
                        let partial = self.frame_to_pyobject(StackFrame::Field {
                            name: type_info.name.clone(),
                            content: content.clone(),
                            type_info: type_info.clone(),
                            depth: *depth,
                        })?;
                        return Ok(Some(partial));
                    }
                }
            }
        }

        Ok(None)
    }

    pub fn is_done(&self) -> bool {
        self.is_done
    }
}

#[pyclass(name = "Parser", unsendable)]
pub struct PyParser {
    parser: TypedStreamParser,
    result: Option<PyObject>,
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
                let mut type_info = PyTypeInfo::extract_from_python(obj)?;
                debug!(
                    "[PyParser::new] Extracted type_info: name='{}', kind='{:?}', origin='{:?}'",
                    type_info.name, type_info.kind, type_info.origin
                );

                if type_info.py_type.is_none() {
                    type_info.py_type = Some(obj.into_py(py));
                }

                let mut wanted_tags = vec![type_info.name.clone()];
                let mut types_to_check = vec![type_info.clone()];

                while let Some(current_type) = types_to_check.pop() {
                    match current_type.kind {
                        crate::python_types::PyTypeKind::List
                        | crate::python_types::PyTypeKind::Set
                        | crate::python_types::PyTypeKind::Tuple
                        | crate::python_types::PyTypeKind::Union => {
                            for arg in &current_type.args {
                                if !wanted_tags.contains(&arg.name) {
                                    wanted_tags.push(arg.name.clone());
                                    types_to_check.push(arg.clone());
                                }
                            }
                        }
                        _ => {}
                    }
                }

                // Also add lowercase versions to handle case-insensitive matching
                let lowercase_tags: Vec<String> =
                    wanted_tags.iter().map(|s| s.to_lowercase()).collect();
                wanted_tags.extend(lowercase_tags);
                wanted_tags.sort();
                wanted_tags.dedup();
                debug!("[PyParser::new] wanted_tags: {:?}", wanted_tags);
                let parser = TypedStreamParser::with_type(type_info, wanted_tags, ignored_tags);
                Ok(Self {
                    parser,
                    result: None,
                })
            }
            None => {
                debug!("[PyParser::new] No type_obj provided.");
                let parser = TypedStreamParser::new(Vec::new(), ignored_tags);
                Ok(Self {
                    parser,
                    result: None,
                })
            }
        }
    }

    #[staticmethod]
    #[pyo3(text_signature = "(pydantic_model)")]
    fn from_pydantic(py: Python, pydantic_model: &PyAny) -> PyResult<Self> {
        let mut type_info = PyTypeInfo::extract_from_python(pydantic_model)?;
        if type_info.py_type.is_none() {
            type_info.py_type = Some(pydantic_model.into_py(py));
        }

        // Include both original and lowercase versions for case-insensitive matching
        let mut wanted_tags = vec![type_info.name.clone()];
        let lowercase = type_info.name.to_lowercase();
        if lowercase != type_info.name {
            wanted_tags.push(lowercase);
        }
        let typed_stream_parser = TypedStreamParser::with_type(type_info, wanted_tags, Vec::new());

        Ok(Self {
            parser: typed_stream_parser,
            result: None,
        })
    }

    #[pyo3(text_signature = "($self, chunk)")]
    fn feed(&mut self, _py: Python, chunk: &str) -> PyResult<Option<PyObject>> {
        debug!("Feeding chunk: {}", chunk);
        if let Some(res) = self.parser.step(chunk)? {
            self.result = Some(res);
        }
        Ok(self.result.clone())
    }

    #[pyo3(text_signature = "($self)")]
    fn is_complete(&self) -> bool {
        self.parser.is_done()
    }

    #[pyo3(text_signature = "($self)")]
    fn get_partial(&mut self, _py: Python) -> PyResult<Option<PyObject>> {
        Ok(self.result.clone())
    }

    #[pyo3(text_signature = "($self)")]
    fn validate(&mut self, _py: Python) -> PyResult<Option<PyObject>> {
        self.get_partial(_py)
    }
}
