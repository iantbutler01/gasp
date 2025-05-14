# GASP - Type-Safe LLM Output Parser

> **⚠️ MAJOR BREAKING CHANGES IN VERSION 1.0.0 ⚠️**  
> Version 1.0.0 is a complete rewrite that removes WAIL entirely and introduces a new tag-based parsing approach.  
> If you're using an older version of GASP, you'll need to significantly change your code to upgrade.  
> See the [Migration Guide](#migrating-from-pre-10-versions) below.

GASP is a Rust-based parser for turning LLM outputs into properly typed Python objects. It handles streaming JSON fragments, recovers from common LLM quirks, and makes structured data extraction actually pleasant.

## The Problem

LLMs are great at generating structured data when asked, but not perfect:

```
<Person>
{
  "name": "Alice Smith",
  "age": 30,
  hobbies: ["coding", "hiking"]
}
</Person>
```

That output has unquoted keys, inconsistent formatting, and is embedded in natural language. Most JSON parsers just give up.

## How GASP Works

GASP uses a tag-based approach to extract and type-cast structured data:

1. Tags like `<Person>...</Person>` mark where the structured data lives (and what type it is)
2. The parser ignores everything outside those tags
3. Inside the tags, it handles messy JSON with broken quotes, trailing commas, etc.
4. The data gets converted into proper Python objects based on type annotations

## Features

- **Tag-Based Extraction**: Extract structured data even when surrounded by explanatory text
- **Streaming Support**: Process data incrementally as it arrives from the LLM
- **Type Inference**: Automatically match JSON objects to Python classes
- **Error Recovery**: Handle common JSON mistakes that LLMs make
- **Pydantic Integration**: Works with Pydantic for validation and schema definition

## Installation

```bash
pip install gasp-py
```

## Quick Example

```python
from gasp import Parser, Deserializable
from typing import List, Optional

class Address(Deserializable):
    street: str
    city: str
    zip_code: str

class Person(Deserializable):
    name: str
    age: int
    address: Address
    hobbies: Optional[List[str]] = None

# Create a parser for the Person type
parser = Parser(Person)

# Process LLM output chunks as they arrive
chunks = [
    '<Person>{"name": "Alice", "age": 30',
    ', "address": {"street": "123 Main St", "city": "Springfield"',
    ', "zip_code": "12345"}, "hobbies": ["reading", "coding"]}</Person>'
]

for chunk in chunks:
    result = parser.feed(chunk)
    print(result)  # Will show partial objects as they're built

# Get the final validated result
person = parser.validate()
print(f"Hello {person.name}!")  # Hello Alice!
```

## Working with Pydantic

GASP integrates seamlessly with Pydantic:

```python
from pydantic import BaseModel
from gasp import Parser

class UserProfile(BaseModel):
    username: str
    email: str
    is_active: bool = True

# Create parser from Pydantic model
parser = Parser.from_pydantic(UserProfile)

# Feed LLM output with tags
llm_output = '<UserProfile>{"username": "alice42", "email": "alice@example.com"}</UserProfile>'
result = parser.feed(llm_output)

# Access as a proper Pydantic object
profile = UserProfile.model_validate(parser.validate())
print(profile.model_dump_json(indent=2))
```

## How Tags Work

The tag name directly indicates what Python type to instantiate:

```
<Person>{ ... JSON data ... }</Person>  # Creates a Person instance
<List>[ ... array data ... ]</List>     # Creates a List
<Address>{ ... address data ... }</Address>  # Creates an Address
```

The parser ignores everything outside of the tags, so the LLM can provide explanations, context, or other text alongside the structured data.

## Advanced Templating with Jinja2

GASP provides built-in Jinja2 integration for more advanced prompt templating:

```python
from gasp import Deserializable, render_template
from typing import List, Optional

class Person(Deserializable):
    """Information about a person"""
    name: str
    age: int
    hobbies: Optional[List[str]] = None

# Create a template with Jinja2 syntax
template = """
# {{ title }}

Generate a {{ type_name|type_description }}.

{% if include_format_instructions %}
Your response must be formatted as:
{{ person_type|format_type }}
{% endif %}
"""

# Provide template context
context = {
    'title': 'Person Generator',
    'type_name': Person,
    'include_format_instructions': True,
    'person_type': Person
}

# Render the template
prompt = render_template(template, context)
```

### Available Jinja2 Filters

- `format_type`: Generates format instructions for a type (e.g., `{{ my_type|format_type }}`)
- `type_description`: Provides a human-readable description of a type, including docstring info

### Template Files & Inheritance

You can also use template files with inheritance:

```python
from gasp import render_file_template

# Renders a template file with GASP filters included
prompt = render_file_template("templates/person_prompt.j2", context)
```

### Direct Jinja2 Access

For advanced cases, you can use the Jinja2 environment directly:

```python
from gasp.jinja_helpers import create_type_environment

# Create a Jinja2 environment with GASP filters
env = create_type_environment()

# Add your own filters
env.filters["my_filter"] = my_filter_function

# Load templates from a directory
env.loader = jinja2.FileSystemLoader("templates/")

# Use directly with Jinja2 API
template = env.get_template("my_template.j2")
prompt = template.render(**context)
```

## Customizing Behavior

Need more control? You can customize type conversion, validation, and parsing behavior:

```python
# Custom type conversions and validation
class CustomPerson(Deserializable):
    name: str
    age: int

    @classmethod
    def __gasp_from_partial__(cls, partial_data):
        """Add custom validation or pre-processing"""
        # Normalize name to title case
        if "name" in partial_data:
            partial_data["name"] = partial_data["name"].title()
        return super().__gasp_from_partial__(partial_data)
```

## Migrating from pre-1.0 Versions

Version 1.0.0 represents a complete architectural shift:

### What's Been Removed

- **WAIL Parser**: The entire WAIL language and validation system has been removed
- **Schema Validation**: The schema-based approach has been replaced with typed parsing
- **WAILGenerator**: This class and its API are no longer available
- All WAIL-related files and examples

### What's New

- **Tag-Based Parsing**: Uses XML-like tags in LLM output to identify data types
- **Type Annotations**: Direct use of Python type annotations to define structures
- **Template Helpers**: Functions to generate format instructions from types
- **Streaming Support**: Improved support for processing data as it arrives

### Migration Steps

1. Replace WAIL schema definitions with Python classes using type annotations
2. Replace `WAILGenerator` with the new `Parser` class
3. Update your prompts to use the new tag-based format
4. Use `template_helpers.interpolate_prompt()` to generate type-aware prompts

Example of old WAIL approach:
```python
schema = r'''
object Response { name: String, age: Number }
template GenerateResponse() -> Response { ... }
'''
generator = WAILGenerator()
generator.load_wail(schema)
(prompt, _, _) = generator.get_prompt()
llm_response = your_llm_client.generate(prompt)
parsed_data = generator.parse_llm_output(llm_response)
```

New approach:
```python
from gasp import Deserializable, Parser
from gasp.template_helpers import interpolate_prompt

class Person(Deserializable):
    name: str
    age: int

# Create a template with a {{return_type}} placeholder
template = """
Generate a profile for a person who loves coding.

{{return_type}}
"""

# Generate a complete prompt with type information
prompt = interpolate_prompt(template, Person)
print(prompt)
# Output will include:
# Your response should be formatted as:
# <Person>{ "name": string, "age": number }</Person>

# Send to your LLM
llm_response = your_llm_client.generate(prompt)

# Parse the tagged response
parser = Parser(Person)
parser.feed(llm_response)
person = parser.validate()

print(f"Created person: {person.name}, {person.age} years old")
```

## Contributing

Contributions welcome! Check out the examples directory to see how things work.

## License

Apache License, Version 2.0
