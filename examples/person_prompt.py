#!/usr/bin/env python3

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
        template_args {
            description: String
        }
        # This is a comment
        let person_prompt = GetPersonFromDescription(
            description: $description
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
    (prompt, warnings, errors) = generator.get_prompt(description="Alice is a 25-year-old software engineer who loves coding, AI, and hiking.")
    print("Generated Prompt:")
    print(prompt)
    print("Warnings:")
    print(warnings)
    print("Errors:")
    print(errors)

    # In a real application, you would send this prompt to your LLM
    # Here we'll simulate an LLM response with some typical quirks
    llm_response = """
    {
        name: 'Alice',
        'age': 25,
        'interests': [
            "coding",
            'AI',
            hiking,
        ]
    }
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
        
        # You can also convert it to standard JSON
        print("\nAs standard JSON:")
        print(json.dumps(result, indent=2))
        
    except Exception as e:
        print(f"❌ Validation error: {e}")

if __name__ == "__main__":
    main() 