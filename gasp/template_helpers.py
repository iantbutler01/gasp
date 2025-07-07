"""
Helpers for generating type-specific format instructions for LLM prompts.
Updated to reflect the actual XML format expected by the parser.
"""

import inspect
import typing
import types
from typing import (
    Any,
    Dict,
    List,
    Optional,
    Tuple,
    Set,
    Type,
    Union,
    get_type_hints,
    get_origin,
    get_args,
)


def type_to_format_instructions(
    type_obj: Any, name: Optional[str] = None, include_important: bool = True
) -> str:
    """
    Generate XML format instructions for a Python type.

    Args:
        type_obj: The Python type to generate instructions for
        name: Optional name to use for the type tag (defaults to class name)
        include_important: Whether to include the IMPORTANT section (default: True)

    Returns:
        A string containing XML format instructions
    """
    # Track complex types that need structure examples
    structure_examples = {}

    # Main formatting function
    def format_type_with_examples(
        type_obj: Type, name: Optional[str] = None
    ) -> Tuple[str, str]:
        # Check origin first to handle type aliases properly
        origin = get_origin(type_obj)

        # Special handling for type aliases (created with 'type' statement)
        if hasattr(type_obj, "__value__"):
            # This is a type alias, use the actual type
            actual_type = type_obj.__value__

            # Check if it's a types.UnionType (Python 3.12 X = Y | Z syntax)
            if type(actual_type).__name__ == "UnionType":
                # Use the type alias name if no explicit name provided
                if not name and hasattr(type_obj, "__name__"):
                    tag_name = type_obj.__name__
                else:
                    tag_name = name or "Object"
                # Format as union using __args__
                return tag_name, _format_union_type_from_args(
                    actual_type.__args__, tag_name, structure_examples
                )

            origin = get_origin(actual_type)
            # For type aliases that are unions, we still use the underlying union format
            if origin is Union:
                return "union", _format_union_type(
                    actual_type, "union", structure_examples
                )
            else:
                # For non-union type aliases, use the alias name
                if not name and hasattr(type_obj, "__name__"):
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
        if origin is Union or origin is typing.Union:
            # Unions don't have their own tag - they use member type tags
            return "union", _format_union_type(type_obj, "union", structure_examples)

        # Handle List types
        if origin is list or origin is typing.List:
            return tag_name, _format_list_type(type_obj, tag_name, structure_examples)

        # Handle Dict types
        if origin is dict or origin is typing.Dict:
            return tag_name, _format_dict_type(type_obj, tag_name, structure_examples)

        # Handle Tuple types
        if origin is tuple or origin is typing.Tuple:
            return tag_name, _format_tuple_type(type_obj, tag_name, structure_examples)

        # Handle Set types
        if origin is set or origin is typing.Set:
            return tag_name, _format_set_type(type_obj, tag_name, structure_examples)

        # Handle primitive types
        if type_obj is str:
            return tag_name, f'<{tag_name} type="str">your string value</{tag_name}>'
        if type_obj is int:
            return tag_name, f'<{tag_name} type="int">42</{tag_name}>'
        if type_obj is float:
            return tag_name, f'<{tag_name} type="float">3.14</{tag_name}>'
        if type_obj is bool:
            return tag_name, f'<{tag_name} type="bool">true</{tag_name}>'

        # Handle Any type
        if type_obj is Any or type_obj is typing.Any:
            return tag_name, f'<{tag_name} type="Any">any value</{tag_name}>'

        # Handle classes (objects with fields)
        return tag_name, _format_class_type(type_obj, tag_name, structure_examples)

    # Generate the main format instruction
    tag_name, main_format = format_type_with_examples(type_obj, name)

    # Build the final instructions
    instructions = "Your response should be formatted as:\n\n" + main_format

    # Add structure examples if any complex types were encountered
    if structure_examples:
        examples_text = []
        for type_name, type_structure in structure_examples.items():
            examples_text.append(
                f"When you see '{type_name}' in a type attribute, use this structure:\n{type_structure}"
            )

        instructions += "\n\n" + "\n\n".join(examples_text)

    # Add important notes about formatting (only at top level)
    if include_important:
        instructions += "\n\nIMPORTANT:"
        instructions += f"\n- You MUST wrap your response in the EXACT tags shown above"
        instructions += '\n- ALWAYS include type="..." attributes where shown'
        instructions += '\n- For dict items, ALWAYS include key="..." attribute'
        instructions += "\n- Do NOT use JSON format or ```xml code blocks"
        instructions += "\n- The tags and attributes are required for proper parsing"

    return instructions


def _format_class_type(
    cls: Type, tag_name: str, structure_examples: Dict[str, str]
) -> str:
    """Format instructions for a class type."""
    try:
        hints = get_type_hints(cls)
    except TypeError:
        # If we can't get type hints, treat as empty class
        hints = {}

    # Get the class name
    class_name = getattr(cls, "__name__", "Object")

    # Get docstrings for fields if available
    field_docs = _extract_field_docs(cls)

    # Build the XML structure
    fields = []
    for field_name, field_type in hints.items():
        # Skip private fields
        if field_name.startswith("_"):
            continue

        # Get the type attribute string
        type_attr = _get_xml_type_attr(field_type)

        # Get example value for the field
        example_value = _get_example_value(field_type)

        # Add comment if we have documentation
        comment = (
            f"  <!-- {field_docs.get(field_name, '')} -->\n    "
            if field_name in field_docs
            else ""
        )

        # Handle different field types
        origin = get_origin(field_type)
        if origin is list or origin is typing.List:
            # Special formatting for lists
            args = get_args(field_type)
            if args:
                item_type = args[0]
                item_type_name = _get_type_name(item_type)
                if _is_class_type(item_type):
                    item_class_name = getattr(item_type, "__name__", "Object")
                    item_content = (
                        f"\n            ...{item_class_name} fields...\n        "
                    )
                    item_format = f'<item type="{item_type_name}">{item_content}</item>'
                    if item_class_name not in structure_examples:
                        structure_examples[item_class_name] = (
                            _generate_class_structure_example(
                                item_type, structure_examples
                            )
                        )
                else:
                    item_example = _get_example_value(item_type)
                    item_format = f'<item type="{item_type_name}">{item_example}</item>'
                field_format = f'{comment}<{field_name} type="{type_attr}">\n        {item_format}\n        ...\n    </{field_name}>'
            else:
                field_format = f'{comment}<{field_name} type="{type_attr}">\n        <item>...</item>\n        ...\n    </{field_name}>'
        elif origin is tuple or origin is typing.Tuple:
            # Special formatting for tuples
            args = get_args(field_type)
            if args:
                items = []
                for item_type in args:
                    item_type_name = _get_type_name(item_type)
                    if _is_class_type(item_type):
                        item_class_name = getattr(item_type, "__name__", "Object")
                        item_content = (
                            f"\n            ...{item_class_name} fields...\n        "
                        )
                        item_format = (
                            f'<item type="{item_type_name}">{item_content}</item>'
                        )
                        if item_class_name not in structure_examples:
                            structure_examples[item_class_name] = (
                                _generate_class_structure_example(
                                    item_type, structure_examples
                                )
                            )
                    else:
                        item_example = _get_example_value(item_type)
                        item_format = (
                            f'<item type="{item_type_name}">{item_example}</item>'
                        )
                    items.append(item_format)
                items_str = "\n        ".join(items)
                field_format = f'{comment}<{field_name} type="{type_attr}">\n        {items_str}\n        ...\n    </{field_name}>'
            else:
                field_format = f'{comment}<{field_name} type="{type_attr}">\n        <item>...</item>\n        ...\n    </{field_name}>'
        elif origin is dict or origin is typing.Dict:
            # Special formatting for dicts
            args = get_args(field_type)
            if args and len(args) == 2:
                key_type, value_type = args
                value_type_name = _get_type_name(value_type)
                if _is_class_type(value_type):
                    value_class_name = getattr(value_type, "__name__", "Object")
                    value_content = (
                        f"\n            ...{value_class_name} fields...\n        "
                    )
                    item_format = f'<item key="example_key" type="{value_type_name}">{value_content}</item>'
                    if value_class_name not in structure_examples:
                        structure_examples[value_class_name] = (
                            _generate_class_structure_example(
                                value_type, structure_examples
                            )
                        )
                else:
                    value_example = _get_example_value(value_type)
                    item_format = f'<item key="example_key" type="{value_type_name}">{value_example}</item>'
                field_format = f'{comment}<{field_name} type="{type_attr}">\n        {item_format}\n        ...\n    </{field_name}>'
            else:
                field_format = f'{comment}<{field_name} type="{type_attr}">\n        <item key="key">value</item>\n        ...\n    </{field_name}>'
        elif origin is Union:
            # Optional fields
            args = get_args(field_type)
            if type(None) in args and len(args) == 2:
                non_none_type = next(arg for arg in args if arg is not type(None))
                type_attr = _get_xml_type_attr(non_none_type)
                example_value = _get_example_value(non_none_type)
                field_format = f'{comment}<{field_name} type="{type_attr}">{example_value}</{field_name}> (optional)'

                # Also check if the non-none type contains nested classes
                non_none_origin = get_origin(non_none_type)
                if non_none_origin is list:
                    list_args = get_args(non_none_type)
                    if list_args and _is_class_type(list_args[0]):
                        item_type = list_args[0]
                        item_class_name = getattr(item_type, "__name__", "Object")
                        if item_class_name not in structure_examples:
                            structure_examples[item_class_name] = (
                                _generate_class_structure_example(
                                    item_type, structure_examples
                                )
                            )
                elif non_none_origin is dict:
                    dict_args = get_args(non_none_type)
                    if len(dict_args) == 2 and _is_class_type(dict_args[1]):
                        value_type = dict_args[1]
                        value_class_name = getattr(value_type, "__name__", "Object")
                        if value_class_name not in structure_examples:
                            structure_examples[value_class_name] = (
                                _generate_class_structure_example(
                                    value_type, structure_examples
                                )
                            )
                elif _is_class_type(non_none_type):
                    # Direct optional class type
                    class_name = getattr(non_none_type, "__name__", "Object")
                    if class_name not in structure_examples:
                        structure_examples[class_name] = (
                            _generate_class_structure_example(
                                non_none_type, structure_examples
                            )
                        )
            else:
                field_format = f"{comment}<{field_name}>{example_value}</{field_name}>"
        else:
            # Regular fields
            field_format = f'{comment}<{field_name} type="{type_attr}">{example_value}</{field_name}>'

        fields.append(field_format)

        # Track complex nested types
        if origin is list:
            args = get_args(field_type)
            if args and _is_class_type(args[0]):
                item_type = args[0]
                item_class_name = getattr(item_type, "__name__", "Object")
                if item_class_name not in structure_examples:
                    structure_examples[item_class_name] = (
                        _generate_class_structure_example(item_type, structure_examples)
                    )
        elif origin is tuple or origin is typing.Tuple:
            args = get_args(field_type)
            for item_type in args:
                if _is_class_type(item_type):
                    item_class_name = getattr(item_type, "__name__", "Object")
                    if item_class_name not in structure_examples:
                        structure_examples[item_class_name] = (
                            _generate_class_structure_example(
                                item_type, structure_examples
                            )
                        )
        elif origin is dict:
            args = get_args(field_type)
            if len(args) == 2 and _is_class_type(args[1]):
                value_type = args[1]
                value_class_name = getattr(value_type, "__name__", "Object")
                if value_class_name not in structure_examples:
                    structure_examples[value_class_name] = (
                        _generate_class_structure_example(
                            value_type, structure_examples
                        )
                    )
        elif _is_class_type(field_type) and not (origin is Union):
            field_class_name = getattr(field_type, "__name__", "Object")
            if field_class_name not in structure_examples:
                structure_examples[field_class_name] = (
                    _generate_class_structure_example(field_type, structure_examples)
                )

    # Add this class to structure examples
    if fields:
        fields_str = "\n    ".join(fields)
        class_example = f"<{class_name}>\n    {fields_str}\n</{class_name}>"
    else:
        class_example = f"<{class_name}>\n</{class_name}>"

    structure_examples[class_name] = class_example

    # Return format for use in main output
    return f"<{tag_name}>\n    ...{class_name} fields...\n</{tag_name}>"


def _format_class_fields(cls: Type, indent: str = "") -> str:
    """Format just the fields of a class for inline use."""
    try:
        hints = get_type_hints(cls)
    except TypeError:
        return ""

    fields = []
    for field_name, field_type in hints.items():
        if field_name.startswith("_"):
            continue
        type_attr = _get_xml_type_attr(field_type)
        example_value = _get_example_value(field_type)
        fields.append(
            f'{indent}<{field_name} type="{type_attr}">{example_value}</{field_name}>'
        )

    return f"\n{indent}".join(fields)


def _generate_class_structure_example(
    cls: Type, structure_examples: Dict[str, str]
) -> str:
    """Generate a complete structure example for a class."""
    try:
        hints = get_type_hints(cls)
    except TypeError:
        hints = {}

    class_name = getattr(cls, "__name__", "Object")

    if not hints:
        return f"<{class_name}>\n</{class_name}>"

    fields = []
    for field_name, field_type in hints.items():
        if field_name.startswith("_"):
            continue

        type_attr = _get_xml_type_attr(field_type)
        origin = get_origin(field_type)

        field_format = ""

        # Handle list type
        if origin is list or origin is typing.List:
            args = get_args(field_type)
            if args:
                item_type = args[0]
                item_type_name = _get_type_name(item_type)
                if _is_class_type(item_type):
                    item_class_name = getattr(item_type, "__name__", "Object")
                    item_content = (
                        f"\n            ...{item_class_name} fields...\n        "
                    )
                    item_format = f'<item type="{item_type_name}">{item_content}</item>'
                    if item_class_name not in structure_examples:
                        structure_examples[item_class_name] = (
                            _generate_class_structure_example(
                                item_type, structure_examples
                            )
                        )
                else:
                    item_example = _get_example_value(item_type)
                    item_format = f'<item type="{item_type_name}">{item_example}</item>'
                field_format = f'    <{field_name} type="{type_attr}">\n        {item_format}\n        ...\n    </{field_name}>'
            else:
                field_format = f'    <{field_name} type="{type_attr}">\n        <item>...</item>\n        ...\n    </{field_name}>'

        # Handle tuple type
        elif origin is tuple or origin is typing.Tuple:
            args = get_args(field_type)
            if args:
                items = []
                for item_type in args:
                    item_type_name = _get_type_name(item_type)
                    if _is_class_type(item_type):
                        item_class_name = getattr(item_type, "__name__", "Object")
                        item_content = f"\n                ...{item_class_name} fields...\n            "
                        item_format = (
                            f'<item type="{item_type_name}">{item_content}</item>'
                        )
                        if item_class_name not in structure_examples:
                            structure_examples[item_class_name] = (
                                _generate_class_structure_example(
                                    item_type, structure_examples
                                )
                            )
                    else:
                        item_example = _get_example_value(item_type)
                        item_format = (
                            f'<item type="{item_type_name}">{item_example}</item>'
                        )
                    items.append("        " + item_format)

                items_str = "\n".join(items)
                field_format = f'    <{field_name} type="{type_attr}">\n{items_str}\n    </{field_name}>'
            else:
                field_format = f'    <{field_name} type="{type_attr}">\n        <item>...</item>\n    </{field_name}>'

        # Handle dict type
        elif origin is dict or origin is typing.Dict:
            args = get_args(field_type)
            if args and len(args) == 2:
                _, value_type = args
                value_type_name = _get_type_name(value_type)
                if _is_class_type(value_type):
                    value_class_name = getattr(value_type, "__name__", "Object")
                    value_content = (
                        f"\n            ...{value_class_name} fields...\n        "
                    )
                    item_format = f'<item key="example_key" type="{value_type_name}">{value_content}</item>'
                    if value_class_name not in structure_examples:
                        structure_examples[value_class_name] = (
                            _generate_class_structure_example(
                                value_type, structure_examples
                            )
                        )
                else:
                    value_example = _get_example_value(value_type)
                    item_format = f'<item key="example_key" type="{value_type_name}">{value_example}</item>'
                field_format = f'    <{field_name} type="{type_attr}">\n        {item_format}\n        ...\n    </{field_name}>'
            else:
                field_format = f'    <{field_name} type="{type_attr}">\n        <item key="key">value</item>\n        ...\n    </{field_name}>'

        # Handle optional fields
        elif origin is Union:
            args = get_args(field_type)
            if type(None) in args and len(args) == 2:
                non_none_type = next(arg for arg in args if arg is not type(None))
                type_attr = _get_xml_type_attr(non_none_type)
                example_value = _get_example_value(non_none_type)
                field_format = f'    <{field_name} type="{type_attr}">{example_value}</{field_name}> (optional)'

                if _is_class_type(non_none_type):
                    nested_class_name = getattr(non_none_type, "__name__", "Object")
                    if nested_class_name not in structure_examples:
                        structure_examples[nested_class_name] = (
                            _generate_class_structure_example(
                                non_none_type, structure_examples
                            )
                        )
            else:
                example_value = _get_example_value(field_type)
                field_format = f'    <{field_name} type="{type_attr}">{example_value}</{field_name}>'
        else:
            example_value = _get_example_value(field_type)
            field_format = (
                f'    <{field_name} type="{type_attr}">{example_value}</{field_name}>'
            )

        fields.append(field_format)

        # Recursively add nested types for classes that are not inside containers
        if _is_class_type(field_type) and not (
            origin
            in (
                list,
                typing.List,
                tuple,
                typing.Tuple,
                dict,
                typing.Dict,
                Union,
            )
        ):
            field_class_name = getattr(field_type, "__name__", "Object")
            if field_class_name not in structure_examples:
                structure_examples[field_class_name] = (
                    _generate_class_structure_example(field_type, structure_examples)
                )

    fields_str = "\n".join(fields)
    return f"<{class_name}>\n{fields_str}\n</{class_name}>"


def _format_union_type_from_args(
    args: Tuple[Type, ...], tag_name: str, structure_examples: Dict[str, str]
) -> str:
    """Format instructions for a Union type from args tuple."""
    # Handle Optional types specially
    if type(None) in args and len(args) == 2:
        non_none_type = next(arg for arg in args if arg is not type(None))
        return _format_optional_type(non_none_type, tag_name, structure_examples)

    # For unions, show each member type as a separate option
    options = []
    for i, arg in enumerate(args):
        if arg is type(None):
            continue  # Skip None type in unions

        arg_name = getattr(arg, "__name__", f"Type{i+1}")

        if _is_class_type(arg):
            # For class types, show the tag WITHOUT type attribute (union members don't use type)
            option_text = f"// Option {i+1}:\n<{arg_name}>\n    ...{arg_name} fields...\n</{arg_name}>"

            # Add the type to structure examples
            if arg_name not in structure_examples:
                structure_examples[arg_name] = _generate_class_structure_example(
                    arg, structure_examples
                )
        else:
            # For simple types, generate format with the arg's own tag (without IMPORTANT section)
            option_format = type_to_format_instructions(
                arg, arg_name, include_important=False
            )
            option_text = f"// Option {i+1}:\n{option_format}"

        options.append(option_text)

    separator = "\n\n- OR -\n\n"
    return separator.join(options)


def _format_union_type(
    union_type: Type, tag_name: str, structure_examples: Dict[str, str]
) -> str:
    """Format instructions for a Union type."""
    args = get_args(union_type)
    return _format_union_type_from_args(args, tag_name, structure_examples)


def _format_optional_type(
    type_obj: Type, tag_name: str, structure_examples: Dict[str, str]
) -> str:
    """Format instructions for an Optional type."""
    type_name = _get_type_name(type_obj)

    if _is_class_type(type_obj):
        # For optional complex types
        class_name = getattr(type_obj, "__name__", "Object")
        content = f'<{tag_name} type="{class_name}">\n    ...{class_name} fields...\n</{tag_name}> (optional - can be omitted)'

        # Add the type to structure examples
        if class_name not in structure_examples:
            structure_examples[class_name] = _generate_class_structure_example(
                type_obj, structure_examples
            )
    else:
        # For optional simple types
        example_value = _get_example_value(type_obj)
        content = f'<{tag_name} type="{type_name}">{example_value}</{tag_name}> (optional - can be omitted)'

    return content


def _format_list_type(
    list_type: Type, tag_name: str, structure_examples: Dict[str, str]
) -> str:
    """Format instructions for a List type."""
    args = get_args(list_type)
    if not args:
        return f'<{tag_name} type="list">\n    <item>...</item>\n    ...\n</{tag_name}>'

    item_type = args[0]

    # Check if item_type is a type alias that resolves to a Union
    actual_item_type = item_type
    if hasattr(item_type, "__value__"):
        actual_item_type = item_type.__value__

    # Get the type name - use the resolved type for type aliases
    item_type_name = _get_type_name(actual_item_type)

    # Special handling for List[Union[...]]
    origin = get_origin(actual_item_type)
    # Also check for UnionType (Python 3.10+ X | Y syntax)
    if origin is Union or type(actual_item_type).__name__ == "UnionType":
        # For lists of union types, we need to handle each union member
        if type(actual_item_type).__name__ == "UnionType":
            union_args = actual_item_type.__args__
        else:
            union_args = get_args(actual_item_type)

        # Add structure examples for each union member that is a class
        # Also handle type aliases that resolve to unions
        for arg in union_args:
            if arg is type(None):
                continue

            # Check if this arg is a type alias that resolves to a Union
            if hasattr(arg, "__value__"):
                actual_arg = arg.__value__
                arg_origin = get_origin(actual_arg)

                # If it's a Union or UnionType, recursively expand it
                if arg_origin is Union or type(actual_arg).__name__ == "UnionType":
                    if type(actual_arg).__name__ == "UnionType":
                        nested_args = actual_arg.__args__
                    else:
                        nested_args = get_args(actual_arg)

                    # Recursively process nested union members
                    for nested_arg in nested_args:
                        if nested_arg is not type(None) and _is_class_type(nested_arg):
                            nested_arg_name = getattr(nested_arg, "__name__", "Object")
                            if nested_arg_name not in structure_examples:
                                structure_examples[nested_arg_name] = (
                                    _generate_class_structure_example(
                                        nested_arg, structure_examples
                                    )
                                )
                # If it's not a union, but still a class type
                elif _is_class_type(actual_arg):
                    arg_name = getattr(actual_arg, "__name__", "Object")
                    if arg_name not in structure_examples:
                        structure_examples[arg_name] = (
                            _generate_class_structure_example(
                                actual_arg, structure_examples
                            )
                        )
            # Regular class type (not a type alias)
            elif _is_class_type(arg):
                arg_name = getattr(arg, "__name__", "Object")
                if arg_name not in structure_examples:
                    structure_examples[arg_name] = _generate_class_structure_example(
                        arg, structure_examples
                    )

        # Show generic type attribute for union
        return f'<{tag_name} type="list[{item_type_name}]">\n    <item type="... some type from {item_type_name} ...">...</item>\n    <item type="... some type from {item_type_name} ...">...</item>\n    ...\n</{tag_name}>'
    elif origin is dict:
        # Special handling for List[dict[...]]
        dict_args = get_args(item_type)
        if len(dict_args) == 2:
            key_type, value_type = dict_args
            value_type_name = _get_type_name(value_type)

            if _is_class_type(value_type):
                value_class_name = getattr(value_type, "__name__", "Object")
                if value_class_name not in structure_examples:
                    structure_examples[value_class_name] = (
                        _generate_class_structure_example(
                            value_type, structure_examples
                        )
                    )
                dict_content = f'\n        <item key="example_key" type="{value_type_name}">\n            ...{value_class_name} fields...\n        </item>\n        ...\n    '
            else:
                value_example = _get_example_value(value_type)
                dict_content = f'\n        <item key="example_key" type="{value_type_name}">{value_example}</item>\n        ...\n    '
        else:
            dict_content = '\n        <item key="key">value</item>\n        ...\n    '

        # Show dict structure explicitly
        return f'<{tag_name} type="list[{item_type_name}]">\n    <item type="{item_type_name}">{dict_content}</item>\n    <item type="{item_type_name}">{dict_content}</item>\n    ...\n</{tag_name}>'
    elif _is_class_type(item_type):
        # For lists of complex types
        class_name = getattr(item_type, "__name__", "Object")

        # Add the item type to structure examples
        if class_name not in structure_examples:
            structure_examples[class_name] = _generate_class_structure_example(
                item_type, structure_examples
            )

        return f'<{tag_name} type="list[{item_type_name}]">\n    <item type="{item_type_name}">\n        ...{class_name} fields...\n    </item>\n    <item type="{item_type_name}">\n        ...{class_name} fields...\n    </item>\n    ...\n</{tag_name}>'
    else:
        # For lists of simple types
        # Special handling for Any type
        if item_type is Any or item_type is typing.Any:
            # For Any type, show it similar to unions - suggesting any type can be used
            return f'<{tag_name} type="list[Any]">\n    <item type="... any type (str | int | float | bool | dict | list | custom objects | etc.) ...">...</item>\n    <item type="... any type (str | int | float | bool | dict | list | custom objects | etc.) ...">...</item>\n    ...\n</{tag_name}>'

        item_example = _get_example_value(item_type)
        return f'<{tag_name} type="list[{item_type_name}]">\n    <item type="{item_type_name}">{item_example}</item>\n    <item type="{item_type_name}">{item_example}</item>\n    ...\n</{tag_name}>'


def _format_dict_type(
    dict_type: Type, tag_name: str, structure_examples: Dict[str, str]
) -> str:
    """Format instructions for a Dict type."""
    args = get_args(dict_type)
    if not args or len(args) != 2:
        return f'<{tag_name} type="dict">\n    <item key="key1">value1</item>\n    <item key="key2">value2</item>\n    ...\n</{tag_name}>'

    key_type, value_type = args
    key_type_name = _get_type_name(key_type)
    value_type_name = _get_type_name(value_type)

    if _is_class_type(value_type):
        # For dicts with complex value types
        class_name = getattr(value_type, "__name__", "Object")

        # Add the value type to structure examples
        if class_name not in structure_examples:
            structure_examples[class_name] = _generate_class_structure_example(
                value_type, structure_examples
            )

        return f'<{tag_name} type="dict[{key_type_name}, {value_type_name}]">\n    <item key="example_key1" type="{value_type_name}">\n        ...{class_name} fields...\n    </item>\n    <item key="example_key2" type="{value_type_name}">\n        ...{class_name} fields...\n    </item>\n    ...\n</{tag_name}>'
    else:
        # For dicts with simple value types
        value_example = _get_example_value(value_type)
        return f'<{tag_name} type="dict[{key_type_name}, {value_type_name}]">\n    <item key="example_key1" type="{value_type_name}">{value_example}</item>\n    <item key="example_key2" type="{value_type_name}">{value_example}</item>\n    ...\n</{tag_name}>'


def _format_tuple_type(
    tuple_type: Type, tag_name: str, structure_examples: Dict[str, str]
) -> str:
    """Format instructions for a Tuple type."""
    args = get_args(tuple_type)
    if not args:
        return (
            f'<{tag_name} type="tuple">\n    <item>...</item>\n    ...\n</{tag_name}>'
        )

    if len(args) == 2 and args[1] is ...:
        # Homogeneous tuple like Tuple[int, ...]
        item_type = args[0]
        item_type_name = _get_type_name(item_type)
        item_example = _get_example_value(item_type)
        items = [
            f'    <item type="{item_type_name}">{item_example}</item>' for _ in range(3)
        ]
        return (
            f'<{tag_name} type="tuple[{item_type_name}, ...]">\n'
            + "\n".join(items)
            + "\n    ...\n</{tag_name}>"
        )
    else:
        # Fixed-length tuple
        items = []
        type_names = []
        for i, arg_type in enumerate(args):
            type_name = _get_type_name(arg_type)
            type_names.append(type_name)
            example = _get_example_value(arg_type)
            items.append(f'    <item type="{type_name}">{example}</item>')
        type_spec = ", ".join(type_names)
        return (
            f'<{tag_name} type="tuple[{type_spec}]">\n'
            + "\n".join(items)
            + f"\n</{tag_name}>"
        )


def _format_set_type(
    set_type: Type, tag_name: str, structure_examples: Dict[str, str]
) -> str:
    """Format instructions for a Set type."""
    args = get_args(set_type)
    if not args:
        return f'<{tag_name} type="set">\n    <item>...</item>\n    ...\n</{tag_name}>'

    item_type = args[0]
    item_type_name = _get_type_name(item_type)
    item_example = _get_example_value(item_type)

    return f'<{tag_name} type="set[{item_type_name}]">\n    <item type="{item_type_name}">{item_example}</item>\n    <item type="{item_type_name}">{item_example}</item>\n    ...\n</{tag_name}>'


def _is_class_type(type_obj: Type) -> bool:
    """Determine if a type is a class type (not a primitive or generic)."""
    # Primitive types are not classes
    if type_obj in (str, int, float, bool, type(None)):
        return False

    # Special types that should not be treated as classes
    if type_obj is Any or type_obj is typing.Any:
        return False

    # Check if it's a generic type
    origin = get_origin(type_obj)
    if origin is not None:
        return False

    # Check if it has type hints (indicates it's a class)
    try:
        hints = get_type_hints(type_obj)
        return True  # If we can get type hints, it's a class
    except (TypeError, AttributeError):
        return False


def _get_type_name(type_obj: Type) -> str:
    """Get a simple name for a type."""
    # Check for type aliases first
    if hasattr(type_obj, "__value__"):
        # For type aliases, still use the underlying type name for type attributes
        type_obj = type_obj.__value__

    # Also check for UnionType (Python 3.10+ X | Y syntax)
    if type(type_obj).__name__ == "UnionType":
        args = type_obj.__args__
        type_names = [_get_type_name(arg) for arg in args if arg is not type(None)]
        return " | ".join(type_names)

    origin = get_origin(type_obj)

    # Handle generic types
    if origin is list:
        args = get_args(type_obj)
        if args:
            return f"list[{_get_type_name(args[0])}]"
        return "list"
    elif origin is dict:
        args = get_args(type_obj)
        if len(args) == 2:
            return f"dict[{_get_type_name(args[0])}, {_get_type_name(args[1])}]"
        return "dict"
    elif origin is tuple:
        args = get_args(type_obj)
        if args:
            if len(args) == 2 and args[1] is ...:
                return f"tuple[{_get_type_name(args[0])}, ...]"
            else:
                type_names = [_get_type_name(arg) for arg in args]
                return f"tuple[{', '.join(type_names)}]"
        return "tuple"
    elif origin is set:
        args = get_args(type_obj)
        if args:
            return f"set[{_get_type_name(args[0])}]"
        return "set"
    elif origin is Union:
        args = get_args(type_obj)
        # Special handling for Optional (Union with None)
        if type(None) in args and len(args) == 2:
            non_none_type = next(arg for arg in args if arg is not type(None))
            return f"Optional[{_get_type_name(non_none_type)}]"
        # For other unions, exclude None
        type_names = [_get_type_name(arg) for arg in args if arg is not type(None)]
        return " | ".join(type_names)

    # Handle primitives
    if type_obj is str:
        return "str"
    if type_obj is int:
        return "int"
    if type_obj is float:
        return "float"
    if type_obj is bool:
        return "bool"
    if type_obj is type(None):
        return "None"

    # Default to class name
    return getattr(type_obj, "__name__", "object")


def _get_xml_type_attr(type_obj: Type) -> str:
    """Get the type attribute value for XML tags."""
    return _get_type_name(type_obj)


def _get_example_value(type_obj: Type) -> str:
    """Get an example value for a type."""
    # Handle primitives
    if type_obj is str:
        return "example string"
    if type_obj is int:
        return "42"
    if type_obj is float:
        return "3.14"
    if type_obj is bool:
        return "true"

    # Handle generic types
    origin = get_origin(type_obj)
    if origin is Union:
        args = get_args(type_obj)
        if type(None) in args and len(args) == 2:
            non_none = next(arg for arg in args if arg is not type(None))
            return _get_example_value(non_none)
        # For general unions, pick first non-None type
        for arg in args:
            if arg is not type(None):
                return _get_example_value(arg)

    # Default
    return "..."


def _extract_field_docs(cls: Type) -> Dict[str, str]:
    """Extract field documentation from class docstring."""
    result = {}

    if not hasattr(cls, "__doc__") or not cls.__doc__:
        return result

    # Try to find field descriptions in docstring
    doc = inspect.getdoc(cls)
    if not doc:
        return result
    lines = doc.split("\n")
    current_field = None

    for line in lines:
        # Check for field descriptions in various formats

        # Format: field_name: Description
        if ":" in line and not line.startswith(" "):
            parts = line.split(":", 1)
            if len(parts) == 2:
                field = parts[0].strip()
                desc = parts[1].strip()
                if field and hasattr(cls, field):
                    result[field] = desc
                    current_field = field

        # Format: field_name -- Description
        elif " -- " in line and not line.startswith(" "):
            parts = line.split(" -- ", 1)
            if len(parts) == 2:
                field = parts[0].strip()
                desc = parts[1].strip()
                if field and hasattr(cls, field):
                    result[field] = desc
                    current_field = field

        # Continuation of previous field description
        elif line.startswith("    ") and current_field:
            result[current_field] += " " + line.strip()

    return result


def interpolate_prompt(
    template: str,
    type_obj: Any,
    format_tag: str = "return_type",
    name: Optional[str] = None,
) -> str:
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

    return template.replace(placeholder, instructions)
