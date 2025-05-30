"""
Helpers for generating type-specific format instructions for LLM prompts.
"""
import inspect
import typing
from typing import Any, Dict, List, Optional, Tuple, Type, Union, get_type_hints, get_origin, get_args

def type_to_format_instructions(type_obj: Any, name: Optional[str] = None) -> str:
    """
    Generate format instructions for a Python type.
    
    Args:
        type_obj: The Python type to generate instructions for
        name: Optional name to use for the type tag (defaults to class name)
        
    Returns:
        A string containing format instructions
    """
    # Track complex types that need structure examples
    structure_examples = {}
    
    # Main formatting function
    def format_type_with_examples(type_obj: Type, name: Optional[str] = None) -> Tuple[str, str]:
        # Check origin first to handle type aliases properly
        origin = get_origin(type_obj)
        
        # Special handling for type aliases (created with 'type' statement)
        # These don't have get_origin() returning Union, but have __value__ attribute
        if hasattr(type_obj, '__value__'):
            # This is a type alias, use the actual type
            actual_type = type_obj.__value__
            
            # Check if it's a types.UnionType (Python 3.12 X = Y | Z syntax)
            if type(actual_type).__name__ == 'UnionType':
                # Use the type alias name if no explicit name provided
                if not name and hasattr(type_obj, '__name__'):
                    tag_name = type_obj.__name__
                else:
                    tag_name = name or "Object"
                # Format as union using __args__
                return tag_name, _format_union_type_from_args(actual_type.__args__, tag_name, structure_examples)
            
            origin = get_origin(actual_type)
            # Use the type alias name if no explicit name provided
            if not name and hasattr(type_obj, '__name__'):
                tag_name = type_obj.__name__
            else:
                tag_name = name or "Object"
        else:
            # Determine tag name based on type
            if name:
                tag_name = name
            else:
                # For other types, try to get __name__ attribute
                tag_name = getattr(type_obj, "__name__", "Object")
        
        # Handle Union types
        if origin is Union:
            # If we detected a union through type alias, use the actual type
            if hasattr(type_obj, '__value__'):
                return tag_name, _format_union_type(type_obj.__value__, tag_name, structure_examples)
            else:
                return tag_name, _format_union_type(type_obj, tag_name, structure_examples)
        
        # Handle List types
        if origin is list:
            return tag_name, _format_list_type(type_obj, tag_name, structure_examples)
        
        # Handle Dict types
        if origin is dict:
            return tag_name, _format_dict_type(type_obj, tag_name, structure_examples)
        
        # Handle primitive types
        if type_obj is str:
            return tag_name, f"<{tag_name}>\"your string value\"</{tag_name}>"
        if type_obj is int or type_obj is float:
            return tag_name, f"<{tag_name}>42</{tag_name}>"
        if type_obj is bool:
            return tag_name, f"<{tag_name}>true</{tag_name}>"
        
        # Handle classes (objects with fields)
        return tag_name, _format_class_type(type_obj, tag_name, structure_examples)
    
    # Generate the main format instruction
    tag_name, main_format = format_type_with_examples(type_obj, name)
    
    # Build the final instructions with structure examples first
    if structure_examples:
        examples_text = []
        for type_name, type_structure in structure_examples.items():
            examples_text.append(f"Each {type_name} object should have this structure:\n{type_structure}")
        
        instructions = "\\n\\n".join(examples_text)
        instructions += "\\nYour response should be formatted as:\\n\\n" + main_format
        instructions += f"\\nIMPORTANT: You MUST wrap your json response in the EXACT tags <{tag_name}> and </{tag_name}>. Do NOT use ```json code blocks. The tags are required for proper parsing."
    else:
        instructions = main_format
    
    return instructions

def _format_class_type(cls: Type, tag_name: str, structure_examples: Dict[str, str]) -> str:
    """Format instructions for a class type."""
    try:
        hints = get_type_hints(cls)
    except TypeError:
        # If we can't get type hints, treat as empty class
        hints = {}
    
    # Always treat classes as complex types, even if empty
    # Get the class name
    class_name = getattr(cls, "__name__", "Object")
    
    # Add to structure examples with _type_name
    if not hints:
        # Empty class still gets a structure example
        structure_examples[class_name] = f'{{\n  "_type_name": "{class_name}"\n}}'
        # Return the class object format
        return f"<{tag_name}>\n{class_name} object\n</{tag_name}>"
    
    # Get docstrings for fields if available
    field_docs = _extract_field_docs(cls)
    
    fields = []
    for field_name, field_type in hints.items():
        # Skip private fields
        if field_name.startswith('_'):
            continue
            
        # Format field type
        field_type_str = _get_type_description(field_type)
        
        # Add a comment if we have documentation for this field
        comment = f"  // {field_docs.get(field_name, '')}" if field_name in field_docs else ""
        
        fields.append(f'  "{field_name}": {field_type_str}{comment}')
        
        # Track complex nested types
        origin = get_origin(field_type)
        if origin is list:
            # If it's a list of complex types, add them to examples
            args = get_args(field_type)
            if args and _is_complex_type(args[0]):
                item_type = args[0]
                item_class_name = getattr(item_type, "__name__", "Object")
                # Add the item type to structure examples if not already there
                if item_class_name not in structure_examples:
                    structure_examples[item_class_name] = _generate_structure_example(item_type)
        elif origin is dict:
            # If it's a dict with complex values, add them to examples
            args = get_args(field_type)
            if len(args) == 2 and _is_complex_type(args[1]):
                value_type = args[1]
                value_class_name = getattr(value_type, "__name__", "Object")
                # Add the value type to structure examples if not already there
                if value_class_name not in structure_examples:
                    structure_examples[value_class_name] = _generate_structure_example(value_type)
        elif _is_complex_type(field_type) and not (origin is Union):
            # If it's a direct complex type, add it to examples
            field_class_name = getattr(field_type, "__name__", "Object")
            if field_class_name not in structure_examples:
                structure_examples[field_class_name] = _generate_structure_example(field_type)
    
    # Add this class type to structure examples with _type_name
    fields_with_type_name = [f'  "_type_name": "{class_name}"'] + fields
    fields_str_with_type_name = ",\n".join(fields_with_type_name)
    structure_examples[class_name] = f"{{\n{fields_str_with_type_name}\n}}"
    
    # Return the class object format
    return f"<{tag_name}>\n{class_name} object\n</{tag_name}>"

def _is_complex_type(type_obj: Type) -> bool:
    """Determine if a type is complex and should have a structure example."""
    # Primitive types are not complex
    if type_obj in (str, int, float, bool, type(None)):
        return False
    
    # Check if it's a class type
    origin = get_origin(type_obj)
    if origin is Union:
        # For unions, check if any member is complex
        args = get_args(type_obj)
        return any(_is_complex_type(arg) for arg in args)
    elif origin is list:
        # For lists, check if the item type is complex
        args = get_args(type_obj)
        return bool(args) and _is_complex_type(args[0])
    elif origin is dict:
        # For dicts, check if the value type is complex
        args = get_args(type_obj)
        return len(args) == 2 and _is_complex_type(args[1])
    
    # Check if it's a class type (including empty classes)
    try:
        # If we can get type hints, it's a class (even if hints is empty)
        hints = get_type_hints(type_obj)
        return isinstance(hints, dict)  # True for both empty and non-empty classes
    except (TypeError, AttributeError):
        # If we can't get type hints, it's probably not a complex type
        return False

def _generate_structure_example(type_obj: Type) -> str:
    """Generate a structure example for a complex type."""
    # Handle different kinds of complex types
    origin = get_origin(type_obj)
    
    if origin is Union:
        # For unions, generate examples for each option
        args = get_args(type_obj)
        options = []
        for i, arg in enumerate(args):
            if arg is not type(None):  # Skip None type
                options.append(f"// Option {i+1}:\n{_generate_structure_example(arg)}")
        
        return "\n\n- OR -\n\n".join(options)
    
    elif origin is list:
        # For lists, provide example content
        args = get_args(type_obj)
        if args and _is_complex_type(args[0]):
            item_example = _generate_structure_example(args[0])
            return f"[\n  {item_example},\n  ...\n]"
        else:
            return "[...]"
    
    elif origin is dict:
        # For dicts, provide example keys and values
        args = get_args(type_obj)
        if len(args) == 2:
            key_type, value_type = args
            key_example = _get_type_description(key_type, simple=True)
            if _is_complex_type(value_type):
                value_example = _generate_structure_example(value_type)
                return f'{{\n  "{key_example}": {value_example},\n  ...\n}}'
            else:
                value_example = _get_type_description(value_type, simple=True)
                return f'{{\n  "{key_example}": {value_example},\n  ...\n}}'
        else:
            return "{...}"
    
    # For classes, get their field types
    try:
        hints = get_type_hints(type_obj)
        if not hints:
            return "{}"
            
        fields = []
        for field_name, field_type in hints.items():
            if field_name.startswith('_'):
                continue
                
            field_type_str = _get_type_description(field_type)
            fields.append(f'  "{field_name}": {field_type_str}')
            
        fields_str = ",\n".join(fields)
        return f"{{\n{fields_str}\n}}"
    except (TypeError, AttributeError):
        # If we can't get type hints, just return a generic object
        return "{...}"

def _generate_structure_example_with_type_name(type_obj: Type, type_name: str) -> str:
    """Generate a structure example for a complex type with _type_name discrimination field."""
    # Handle different kinds of complex types
    origin = get_origin(type_obj)
    
    if origin is Union:
        # For unions, generate examples for each option
        args = get_args(type_obj)
        options = []
        for i, arg in enumerate(args):
            if arg is not type(None):  # Skip None type
                arg_name = getattr(arg, "__name__", f"Type{i+1}")
                options.append(f"// Option {i+1}:\n{_generate_structure_example_with_type_name(arg, arg_name)}")
        
        return "\n\n- OR -\n\n".join(options)
    
    elif origin is list:
        # For lists, provide example content
        args = get_args(type_obj)
        if args and _is_complex_type(args[0]):
            item_example = _generate_structure_example_with_type_name(args[0], type_name)
            return f"[\n  {item_example},\n  ...\n]"
        else:
            return "[...]"
    
    elif origin is dict:
        # For dicts, provide example keys and values
        args = get_args(type_obj)
        if len(args) == 2:
            key_type, value_type = args
            key_example = _get_type_description(key_type, simple=True)
            if _is_complex_type(value_type):
                value_example = _generate_structure_example_with_type_name(value_type, type_name)
                return f'{{\n  "_type_name": "{type_name}",\n  "{key_example}": {value_example},\n  ...\n}}'
            else:
                value_example = _get_type_description(value_type, simple=True)
                return f'{{\n  "_type_name": "{type_name}",\n  "{key_example}": {value_example},\n  ...\n}}'
        else:
            return f'{{\n  "_type_name": "{type_name}",\n  ...\n}}'
    
    # For classes, get their field types and add _type_name
    try:
        hints = get_type_hints(type_obj)
        if not hints:
            return f'{{\n  "_type_name": "{type_name}"\n}}'
            
        fields = [f'  "_type_name": "{type_name}"']
        for field_name, field_type in hints.items():
            if field_name.startswith('_'):
                continue
                
            field_type_str = _get_type_description(field_type)
            fields.append(f'  "{field_name}": {field_type_str}')
            
        fields_str = ",\n".join(fields)
        return f"{{\n{fields_str}\n}}"
    except (TypeError, AttributeError):
        # If we can't get type hints, just return a generic object with _type_name
        return f'{{\n  "_type_name": "{type_name}"\n}}'

def _format_union_type_from_args(args: Tuple[Type, ...], tag_name: str, structure_examples: Dict[str, str]) -> str:
    """Format instructions for a Union type from args tuple."""
    # Handle Optional types specially
    if type(None) in args and len(args) == 2:
        non_none_type = next(arg for arg in args if arg is not type(None))
        return _format_optional_type(non_none_type, tag_name, structure_examples)
    
    # Format each option
    options = []
    for i, arg in enumerate(args):
        if _is_complex_type(arg):
            # For complex types, add a description and track structure examples
            arg_name = getattr(arg, "__name__", f"Type{i+1}")
            option_content = f"{arg_name} object"
            
            # Add the type to structure examples with _type_name discrimination field
            if arg_name not in structure_examples:
                structure_examples[arg_name] = _generate_structure_example_with_type_name(arg, arg_name)
        else:
            # For simple types, generate standard format instructions
            option_format = type_to_format_instructions(arg, tag_name)
            
            # Extract the content part (between the tags)
            tag_open = f"<{tag_name}>"
            tag_close = f"</{tag_name}>"
            content_start = option_format.find(tag_open) + len(tag_open)
            content_end = option_format.rfind(tag_close)
            option_content = option_format[content_start:content_end]
            
        option_text = f"// Option {i+1}:\n{option_content}"
        options.append(option_text)
    
    separator = "\n\n- OR -\n\n"
    all_options = separator.join(options)
    
    return f"<{tag_name}>\n{all_options}\n</{tag_name}>"

def _format_union_type(union_type: Type, tag_name: str, structure_examples: Dict[str, str]) -> str:
    """Format instructions for a Union type."""
    args = get_args(union_type)
    return _format_union_type_from_args(args, tag_name, structure_examples)

def _format_optional_type(type_obj: Type, tag_name: str, structure_examples: Dict[str, str]) -> str:
    """Format instructions for an Optional type."""
    if _is_complex_type(type_obj):
        # For complex types, add structure examples
        type_name = getattr(type_obj, "__name__", "Object")
        content = f"{type_name} object"
        
        # Add the type to structure examples
        if type_name not in structure_examples:
            structure_examples[type_name] = _generate_structure_example(type_obj)
    else:
        # For simple types, use standard format
        simple_format = type_to_format_instructions(type_obj, tag_name)
        
        # Extract the content part
        tag_open = f"<{tag_name}>"
        tag_close = f"</{tag_name}>"
        content_start = simple_format.find(tag_open) + len(tag_open)
        content_end = simple_format.rfind(tag_close)
        content = simple_format[content_start:content_end]
    
    # Add null as an option
    content_with_null = f"// Option 1: Value\n{content}\n\n- OR -\n\n// Option 2: Null\nnull"
    
    return f"<{tag_name}>\n{content_with_null}\n</{tag_name}>"

def _format_list_type(list_type: Type, tag_name: str, structure_examples: Dict[str, str]) -> str:
    """Format instructions for a List type."""
    args = get_args(list_type)
    if not args:
        return f"<{tag_name}>[...]</{tag_name}>"
        
    item_type = args[0]
    
    if _is_complex_type(item_type):
        # For lists of complex types, use a descriptive format
        item_name = getattr(item_type, "__name__", "Object")
        
        # Add the item type to structure examples
        if item_name not in structure_examples:
            structure_examples[item_name] = _generate_structure_example(item_type)
        
        return f"<{tag_name}>[...array of {item_name} objects...]</{tag_name}>"
    else:
        # For simple types, use the standard format
        item_desc = _get_type_description(item_type, simple=True)
        return f"<{tag_name}>[{item_desc}, {item_desc}, ...]</{tag_name}>"

def _format_dict_type(dict_type: Type, tag_name: str, structure_examples: Dict[str, str]) -> str:
    """Format instructions for a Dict type."""
    args = get_args(dict_type)
    if not args or len(args) != 2:
        return f"<{tag_name}>{{}}</{tag_name}>"
        
    key_type, value_type = args
    key_desc = _get_type_description(key_type, simple=True)
    
    if _is_complex_type(value_type):
        # For dicts with complex value types, use descriptive format
        value_name = getattr(value_type, "__name__", "Object")
        
        # Add the value type to structure examples
        if value_name not in structure_examples:
            structure_examples[value_name] = _generate_structure_example(value_type)
        
        return f"<{tag_name}>{{...dictionary mapping {key_desc} to {value_name} objects...}}</{tag_name}>"
    else:
        # For simple value types, use standard format
        value_desc = _get_type_description(value_type, simple=True)
        return f'<{tag_name}>{{\n  "{key_desc}": {value_desc},\n  "{key_desc}": {value_desc},\n  ...\n}}</{tag_name}>'

def _get_type_description(type_obj: Type, simple: bool = False) -> str:
    """Get a simple string description of a type."""
    # Handle primitive types
    if type_obj is str:
        return "string"
    if type_obj is int:
        return "number"
    if type_obj is float:
        return "number"
    if type_obj is bool:
        return "boolean"
    
    # Handle Union types
    origin = get_origin(type_obj)
    if origin is Union:
        args = get_args(type_obj)
        # Handle Optional
        if type(None) in args and len(args) == 2:
            non_none = next(arg for arg in args if arg is not type(None))
            base_desc = _get_type_description(non_none, simple)
            return f"{base_desc} (optional)"
        else:
            arg_descs = [_get_type_description(arg, simple) for arg in args]
            return " | ".join(arg_descs)
    
    # Handle List types
    if origin is list:
        args = get_args(type_obj)
        if args:
            item_desc = _get_type_description(args[0], simple)
            return f"{item_desc}[]"
        else:
            return "array"
    
    # Handle Dict types
    if origin is dict:
        if simple:
            return "object"
        args = get_args(type_obj)
        if len(args) == 2:
            key_desc = _get_type_description(args[0], True)
            value_desc = _get_type_description(args[1], True)
            return f"Record<{key_desc}, {value_desc}>"
        else:
            return "object"
    
    # Default to class name or "object"
    return getattr(type_obj, "__name__", "object")

def _extract_field_docs(cls: Type) -> Dict[str, str]:
    """Extract field documentation from class docstring."""
    result = {}
    
    if not hasattr(cls, '__doc__') or not cls.__doc__:
        return result
    
    # Try to find field descriptions in docstring
    doc = inspect.getdoc(cls)
    if not doc:
        return result
    lines = doc.split('\n')
    current_field = None
    
    for line in lines:
        # Check for field descriptions in various formats
        
        # Format: field_name: Description
        if ':' in line and not line.startswith(' '):
            parts = line.split(':', 1)
            if len(parts) == 2:
                field = parts[0].strip()
                desc = parts[1].strip()
                if field and hasattr(cls, field):
                    result[field] = desc
                    current_field = field
                    
        # Format: field_name -- Description
        elif ' -- ' in line and not line.startswith(' '):
            parts = line.split(' -- ', 1)
            if len(parts) == 2:
                field = parts[0].strip()
                desc = parts[1].strip()
                if field and hasattr(cls, field):
                    result[field] = desc
                    current_field = field
        
        # Continuation of previous field description
        elif line.startswith('    ') and current_field:
            result[current_field] += ' ' + line.strip()
    
    return result

def interpolate_prompt(template: str, type_obj: Any, format_tag: str = "return_type", name: Optional[str] = None) -> str:
    """
    Replace {{format_tag}} in the template with format instructions for the type.
    
    Args:
        template: The prompt template with {{format_tag}} placeholders
        type_obj: The Python type to generate instructions for
        format_tag: The tag to replace (default: "return_type")
        name: Optional name to use for the type tag (defaults to class name)
        
    Returns:
        The interpolated prompt
    """
    placeholder = "{{" + format_tag + "}}"
    
    if placeholder not in template:
        return template
    
    instructions = type_to_format_instructions(type_obj, name=name)
    instructions_with_header = f"Your response should be formatted as:\n\n{instructions}"
    
    return template.replace(placeholder, instructions_with_header)
