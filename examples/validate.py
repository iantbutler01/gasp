#!/usr/bin/env python3

from gasp import WAILGenerator

def main():
    # Create a validator
    generator = WAILGenerator()

    # Load a WAIL schema with an intentional typo
    with open("./example.wail", "r") as f:
        ideal_wail_schema = f.read()
    wail_schema = '''
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
    let person_prompt = GetPersonFromDescription(
        description: "Alice is a 25-year-old software engineer who loves coding, AI, and hiking."
    );

    prompt {
        {{person_prompt}}
    }
}
'''

    wail_schema = ideal_wail_schema
    generator.load_wail(wail_schema)

    # Validate the schema
    warnings, errors = generator.validate_wail()

    print("Validation Results:")
    print("\nWarnings:")
    for warning in warnings:
        print(f"- {warning}")

    print("\nErrors:")
    for error in errors:
        print(f"- {error}")

if __name__ == "__main__":
    main() 