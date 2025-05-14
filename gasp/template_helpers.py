"""
Helpers for generating type-specific format instructions for LLM prompts.
"""
import inspect
import typing
from typing import Any, Dict, List, Optional, Tuple, Type, Union, get_type_hints, get_origin, get_args

def type_to_format_instructions(type_obj: Type, name: Optional[str] = None) -> str:
    """
    Generate format instructions for a Python type.
    
    Args:
        type_obj: The Python type to generate instructions for
        name: Optional name to use for the type tag (defaults to class name)
        
    Returns:
        A string containing format instructions
    """
    tag_name = name or getattr(type_obj, "__name__", "Object")
    
    # Handle Union types
    origin = get_origin(type_obj)
    if origin is Union:
        return _format_union_type(type_obj, tag_name)
    
    # Handle List types
    if origin is list:
        return _format_list_type(type_obj, tag_name)
    
    # Handle Dict types
    if origin is dict:
        return _format_dict_type(type_obj, tag_name)
    
    # Handle primitive types
    if type_obj is str:
        return f"<{tag_name}>\"your string value\"</{tag_name}>"
    if type_obj is int or type_obj is float:
        return f"<{tag_name}>42</{tag_name}>"
    if type_obj is bool:
        return f"<{tag_name}>true</{tag_name}>"
    
    # Handle classes (objects with fields)
    return _format_class_type(type_obj, tag_name)

def _format_class_type(cls: Type, tag_name: str) -> str:
    """Format instructions for a class type."""
    hints = get_type_hints(cls)
    
    # Handle empty classes
    if not hints:
        return f"<{tag_name}>{{}}</{tag_name}>"
    
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
    
    fields_str = ",\n".join(fields)
    return f"<{tag_name}>{{\n{fields_str}\n}}</{tag_name}>"

def _format_union_type(union_type: Type, tag_name: str) -> str:
    """Format instructions for a Union type."""
    args = get_args(union_type)
    
    # Handle Optional types specially
    if type(None) in args and len(args) == 2:
        non_none_type = next(arg for arg in args if arg is not type(None))
        return _format_optional_type(non_none_type, tag_name)
    
    # Format each option
    options = []
    for i, arg in enumerate(args):
        option_format = type_to_format_instructions(arg, tag_name)
        
        # Extract the content part (between the tags)
        tag_open = f"<{tag_name}>"
        tag_close = f"</{tag_name}>"
        content_start = option_format.find(tag_open) + len(tag_open)
        content_end = option_format.rfind(tag_close)
        content = option_format[content_start:content_end]
        
        option_text = f"// Option {i+1}:\n{content}"
        options.append(option_text)
    
    separator = "\n\n- OR -\n\n"
    all_options = separator.join(options)
    
    return f"<{tag_name}>\n{all_options}\n</{tag_name}>"

def _format_optional_type(type_obj: Type, tag_name: str) -> str:
    """Format instructions for an Optional type."""
    base_format = type_to_format_instructions(type_obj, tag_name)
    
    # Extract the content part
    tag_open = f"<{tag_name}>"
    tag_close = f"</{tag_name}>"
    content_start = base_format.find(tag_open) + len(tag_open)
    content_end = base_format.rfind(tag_close)
    content = base_format[content_start:content_end]
    
    # Add null as an option
    content_with_null = f"{content}\n\n- OR -\n\nnull"
    
    return f"<{tag_name}>\n{content_with_null}\n</{tag_name}>"

def _format_list_type(list_type: Type, tag_name: str) -> str:
    """Format instructions for a List type."""
    args = get_args(list_type)
    if args:
        item_type = args[0]
        item_desc = _get_type_description(item_type, simple=True)
        return f"<{tag_name}>[{item_desc}, {item_desc}, ...]</{tag_name}>"
    else:
        return f"<{tag_name}>[...]</{tag_name}>"

def _format_dict_type(dict_type: Type, tag_name: str) -> str:
    """Format instructions for a Dict type."""
    args = get_args(dict_type)
    if len(args) == 2:
        key_type, value_type = args
        key_desc = _get_type_description(key_type, simple=True)
        value_desc = _get_type_description(value_type, simple=True)
        return f'<{tag_name}>{{\n  "{key_desc}": {value_desc},\n  "{key_desc}": {value_desc},\n  ...\n}}</{tag_name}>'
    else:
        return f"<{tag_name}>{{}}</{tag_name}>"

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
    
    if not cls.__doc__:
        return result
    
    # Try to find field descriptions in docstring
    lines = inspect.getdoc(cls).split('\n')
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

def interpolate_prompt(template: str, type_obj: Type, format_tag: str = "return_type", name: Optional[str] = None) -> str:
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
