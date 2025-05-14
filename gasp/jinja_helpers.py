"""
Jinja2 template support for GASP.

This module provides helpers for using Jinja2 templates with GASP's type formatting system.
It includes custom filters and functions to generate type-aware prompts with more
advanced templating capabilities than the basic interpolate_prompt function.
"""
import inspect
from typing import Any, Dict, Optional, Type, Union
import jinja2

from .template_helpers import type_to_format_instructions

def create_type_environment() -> jinja2.Environment:
    """
    Create a Jinja2 environment with GASP type formatting filters.
    
    Returns:
        A Jinja2 Environment with GASP custom filters.
    """
    env = jinja2.Environment(
        trim_blocks=True,
        lstrip_blocks=True,
        keep_trailing_newline=True,
    )
    
    # Add custom filters for type formatting
    env.filters["format_type"] = format_type_filter
    env.filters["type_description"] = type_description_filter
    
    return env

def format_type_filter(type_obj: Type, name: Optional[str] = None) -> str:
    """
    Jinja2 filter for formatting a type as instructions.
    
    Example usage in template:
    {{ person_type|format_type }}
    {{ user_type|format_type("User") }}
    
    Args:
        type_obj: The Python type to format
        name: Optional name override for the type tag
        
    Returns:
        Formatted type instructions
    """
    return type_to_format_instructions(type_obj, name)

def type_description_filter(type_obj: Type) -> str:
    """
    Jinja2 filter that returns a simple text description of a type.
    
    Example usage in template:
    The API returns a {{ response_type|type_description }}.
    
    Args:
        type_obj: The Python type to describe
        
    Returns:
        Human-readable description of the type
    """
    # Get the type name
    type_name = getattr(type_obj, "__name__", str(type_obj))
    
    # Get docstring if available
    doc = inspect.getdoc(type_obj)
    if doc:
        # Use first line of docstring
        first_line = doc.split('\n')[0].strip()
        return f"{type_name} ({first_line})"
    
    return type_name

def render_template(template_str: str, context: Dict[str, Any], 
                   env: Optional[jinja2.Environment] = None) -> str:
    """
    Render a Jinja2 template with the given context.
    
    Example usage:
    ```python
    template = '''
    # {{ title }}
    
    Generate a {{ response_type|type_description }}.
    
    {{ response_type|format_type }}
    '''
    
    context = {
        'title': 'Person Generator',
        'response_type': Person
    }
    
    prompt = render_template(template, context)
    ```
    
    Args:
        template_str: Jinja2 template string
        context: Dictionary of variables to use in the template
        env: Optional Jinja2 environment (creates one with GASP filters if not provided)
        
    Returns:
        The rendered template as a string
    """
    if env is None:
        env = create_type_environment()
    
    template = env.from_string(template_str)
    return template.render(**context)

def render_file_template(template_path: str, context: Dict[str, Any],
                        env: Optional[jinja2.Environment] = None) -> str:
    """
    Render a Jinja2 template file with the given context.
    
    Args:
        template_path: Path to the Jinja2 template file
        context: Dictionary of variables to use in the template
        env: Optional Jinja2 environment (creates one with GASP filters if not provided)
        
    Returns:
        The rendered template as a string
    """
    if env is None:
        env = create_type_environment()
        
    # Configure the file system loader
    file_loader = jinja2.FileSystemLoader(searchpath='./')
    env.loader = file_loader
    
    template = env.get_template(template_path)
    return template.render(**context)
