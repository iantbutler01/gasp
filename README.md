# GASP (Gee Another Schema Parser)

GASP is a high-performance Rust-based parser and validator for WAIL (Widely Applicable Interface Language) schemas and JSON responses. It's specifically designed to work with Large Language Models (LLMs) by providing robust error recovery for common LLM response quirks.

## What is WAIL?

WAIL (Widely Applicable Interface Language) is a schema language designed for:
1. Generating type-validated LLM prompts
2. Validating JSON responses from LLMs
3. Providing clear error messages for schema violations

## Why

In our experience the ergonomics around tool calling kind of suck right now and in the lowest common denominator settings are down right painful.

If you're using OpenRouter (which is great) they choose not to support some platform specific features (understandable) like the "ANY" parameter from Anthropic so you wind up with super verbose output, the occasional no tool call, missing params even when specified in "required" and so we decided to implement this prompt creator and schema validator because what are tool calls other than type interfaces. 

Honestly, [BAML](https://github.com/BoundaryML/baml) is a really sick tool and more feature complete than this, with like people actually paid to work on it. However, they require you to use their code gen'd inference clients for sending messages to the LLM. That let's them do some really powerful things like validation mid streaming, but you have to be all in on them.

GASP and WAIL let you separate out prompt creation, inference and prompt validation from one another so you can apply GASP to whatever client floats your boat with the trade off that we aren't intending to make this work for every streaming format under the sun (at least I'm not, feel free to contribute!) so it's only applicable to fully generated outputs.

I didn't need all that especially because I along with my friend and co-founder have written [Asimov](https://github.com/BismuthCloud/asimov/tree/main) a framework for building Agents that includes all of our own inference machinery I'm not looking to give up.

Anyway both Asimov and now GASP/WAIL are built for supporting [Bismuth](https://waitlist.bismuth.sh) a programming agent that can help businesses find and patch bugs on your Github PRs before you ever know about them. 

## Features

- **Robust Error Recovery**: Handles common LLM response issues like trailing commas, unquoted identifiers, and malformed JSON
- **Type Coercion**: Attempt to fix type mismatches like Number -> String, single items to arrays, object types if a unique set of fields can be matched to a schema.
- **Type Validation**: Strong type checking for both schema definitions and JSON responses
- **High Performance**: Written in Rust with Python bindings for optimal speed
- **Developer Friendly**: Clear error messages (except for syntax errors see below) and intuitive schema syntax
- **LLM-Optimized**: Specifically designed to work with the quirks and inconsistencies of LLM outputs

## Anti-Features
- **Limited syntax error messages** - Right now syntax errors will tell you where the parser failed but messages aren't more helpful than that so sometimes it's hard to figure out what is wrong in the parser syntax.

## Caveats
- **Output parsing is assumed to be sequential** - That is we assume output happens in the same order as the template variable binding so like if binding1 doesn't correspond to output1 things will be wacky.

## Installation

```bash
pip install gasp-py
```

## Usage

```python
from gasp import WAILGenerator

# Create a validator with your WAIL schema
generator = WAILGenerator()

schema = r'''
object Response {
    name: String
    age: Number
    interests: String[]
}

template GenerateResponse(desc: String) -> Response {
    prompt: """
    Based on {{desc}} generate a response that returns 
    
    {{return_type}}      
    """
}

main {
    template_args {
        desc: String
    }

    let res = GenerateResponse(desc: $desc);

    prompt {
        {{res}}
    }
}
'''

generator.load_wail(schema)

# Get the prompt to send to the LLM
(prompt, warnings, errors) = generator.get_prompt(desc="A totally good response")

print(prompt)

# Use your favorite LLM client to send the prompt and get a response
# response = your_fav_client.generate(prompt) # or whatever your interface is
examples_res = """
<result>
{
    name: "Alice",
    "age": 25,
    "interests": [coding, 'AI', "music"]
}
</result>
"""

# Parse and validate JSON responses
generator.parse_llm_output(examples_res)

# Note the ability to handle malformed JSON
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
from gasp import WAILGenerator
import json

def create_person_prompt():
    """Define the WAIL schema for person generation."""
    return r'''
    # Define our Person type
    object Person {
        name: String
        age: Number
        interests: String[]
    }

    # Define a template for generating a person from a description
    template GetPersonFromDescription(description: String) -> Person {
        prompt: """
        Given this description of a person: "{{description}}"

        Create a Person object with their name, age, and interests.
        Return in this format: 
        {{return_type}}
        """
    }

    main {
        # This is a comment
        let person_prompt = GetPersonFromDescription(
            description: "Alice is a 25-year-old software engineer who loves coding, AI, and hiking."
        );

        # This is the prompt we'll send to the LLM
        prompt {
            This is an example of a prompt generated by WAIL:
            {{person_prompt}}
        }
    }
    '''

def main():
    # Initialize our validator with the schema
    generator = WAILGenerator()
    generator.load_wail(create_person_prompt())

    warnings, errors = generator.validate_wail()

    print("Validation Results:")
    print("\nWarnings:")
    print(warnings)
    print("\nErrors:")
    print(errors)

    # Get the generated prompt - this is what you'd send to your LLM
    (prompt, warnings, errors) = generator.get_prompt()
    print("Generated Prompt:")
    print(prompt)
    print("Warnings:")
    print(warnings)
    print("Errors:")
    print(errors)

    # In a real application, you would send this prompt to your LLM
    # Here we'll simulate an LLM response with some typical quirks
    llm_response = """
    <result>
    {
        'name': 'Alice',
        'age': 25,
        'interests': [
            "coding",
            'AI',
            hiking,
        ]
    }
    </result>
    """

    try:
        # Validate the LLM's response and get the parsed JSON as a Python dict
        result = generator.parse_llm_output(llm_response)
        print("✓ Response validation successful!")

        result = result["person_prompt"]
        

        # Work with the validated data
        print("\nParsed Person:")
        print(f"Name: {result['name']}")
        print(f"Age: {result['age']}")
        print(f"Interests: {', '.join(result['interests'])}")
        
        # # You can also convert it to standard JSON
        # print("\nAs standard JSON:")
        # print(json.dumps(result, indent=2))
        
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
Validation Results:
Warnings:
[]
Errors:
[]
Generated Prompt:

This is an example of a prompt generated by WAIL:

Given this description of a person: "Alice is a 25-year-old software engineer who loves coding, AI, and hiking."

Create a Person object with their name, age, and interests.
Return in this format: 

{
  name: string
  age: number
  interests: String[]>
}


Warnings:
[]
Errors:
[]
✓ Response validation successful!

Parsed Person:
Name: Alice
Age: 25
Interests: coding, AI, hiking
```

## Changelog

0.9.0 - Complete parser rewrite to an incremental non recursive parser. All original tests pass but parsing semantics may be slightly different. Considering this a breaking change because there are likely parser edge cases I can't account for.