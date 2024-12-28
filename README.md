# GASP (Gee Another Schema Parser)

GASP is a high-performance Rust-based parser and validator for WAIL (Widely Applicable Interface Language) schemas and JSON responses. It's specifically designed to work with Large Language Models (LLMs) by providing robust error recovery for common LLM response quirks.

## What is WAIL?

WAIL (Widely Applicable Interface Language) is a schema language designed for:
1. Generating type-validated LLM prompts
2. Validating JSON responses from LLMs
3. Providing clear error messages for schema violations

## Features

- **Robust Error Recovery**: Handles common LLM response issues like trailing commas, unquoted identifiers, and malformed JSON
- **Type Validation**: Strong type checking for both schema definitions and JSON responses
- **High Performance**: Written in Rust with Python bindings for optimal speed
- **Developer Friendly**: Clear error messages and intuitive schema syntax
- **LLM-Optimized**: Specifically designed to work with the quirks and inconsistencies of LLM outputs

## Installation

```bash
pip install gasp-py
```

## Usage

```python
from gasp_py import WAILValidator

# Create a validator with your WAIL schema
validator = WAILValidator("""
    // Define your schema here
    template Response {
        name: String,
        age: Number,
        interests: Array<String>
    }
""")

# Validate JSON responses
try:
    validator.validate_json("""
    {
        "name": "Alice",
        "age": 25,
        "interests": ["coding", "AI", "music"]
    }
    """)
except Exception as e:
    print(f"Validation error: {e}")
```

## Error Recovery

GASP includes built-in error recovery for common LLM response issues:
- Trailing commas in arrays and objects
- Unquoted identifiers in object keys
- Missing quotes around strings
- Inconsistent whitespace and formatting

## License

Apache License, Version 2.0 - see LICENSE file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Complete Example

Here's a complete example showing how to use GASP in Python:

```python
from gasp_py import WAILValidator
import json

def create_person_prompt():
    """Define the WAIL schema for person generation."""
    return """
    # Define our Person type
    object Person {
        name: String
        age: Number
        interests: Array<String>
    }

    # Define a template for generating a person from a description
    template GetPersonFromDescription(description: String) -> Person {
        prompt: '''
        Given this description of a person: {{description}}
        Create a Person object with their name, age, and interests.
        Return in this format: {{return_type}}
        '''
    }

    # Main section defines what we want to do
    main {
        person_prompt = GetPersonFromDescription(
            description: "Alice is a 25-year-old software engineer who loves coding, AI, and hiking."
        )
        prompt {
            {{person_prompt}}
        }
    }
    """

def main():
    # Initialize our validator with the schema
    validator = WAILValidator()
    validator.load_wail(create_person_prompt())

    # Get the generated prompt - this is what you'd send to your LLM
    prompt = validator.get_prompt()
    print("Generated Prompt:")
    print(prompt)
    print()

    # In a real application, you would send this prompt to your LLM
    # Here we'll simulate an LLM response with some typical quirks
    llm_response = """
    {
        'name': 'Alice',  # Single quoted strings
        age: 25,          # Unquoted key and number
        'interests': [    # Mix of quote styles
            "coding",     # Double quotes
            'AI',         # Single quotes
            hiking,       # Unquoted string
        ]                 # GASP handles all these cases
    }
    """

    try:
        # Validate the LLM's response
        validator.validate_json(llm_response)
        print("✓ Response validation successful!")
        
        # Get the parsed and validated response as a Python dict
        result = validator.get_parsed_json()
        
        # Work with the validated data
        print("\nParsed Person:")
        print(f"Name: {result['name']}")
        print(f"Age: {result['age']}")
        print(f"Interests: {', '.join(result['interests'])}")
        
        # You can also convert it to standard JSON
        print("\nAs standard JSON:")
        print(json.dumps(result, indent=2))
        
    except Exception as e:
        print(f"❌ Validation error: {e}")

if __name__ == "__main__":
    main()
```

This example demonstrates:
1. Creating a WAIL schema with proper Python string formatting
2. Defining object types and templates in WAIL
3. Generating a type-aware prompt for your LLM
4. Handling common LLM response quirks automatically
5. Validating and parsing the response
6. Working with the validated data in Python

When run, this script will output:
```
Generated Prompt:
Given this description of a person: Alice is a 25-year-old software engineer who loves coding, AI, and hiking.
Create a Person object with their name, age, and interests.
Return in this format: 
{
  name: string
  age: number
  interests: array<string>
}

✓ Response validation successful!

Parsed Person:
Name: Alice
Age: 25
Interests: coding, AI, hiking

As standard JSON:
{
  "name": "Alice",
  "age": 25,
  "interests": [
    "coding",
    "AI",
    "hiking"
  ]
}
```
