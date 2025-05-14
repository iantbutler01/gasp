#!/usr/bin/env python3
"""
Example showing how to use GASP to generate type-aware prompts and parse LLM responses.
"""
from typing import List, Optional, Union
from gasp import Deserializable, Parser
from gasp.template_helpers import interpolate_prompt

# Define our data models
class Address(Deserializable):
    """Physical address"""
    street: str  # Street name and number
    city: str    # City name
    zip_code: str # Postal code
    
class Person(Deserializable):
    """Information about a person"""
    name: str
    age: int
    address: Address
    hobbies: Optional[List[str]] = None

class ErrorResponse(Deserializable):
    """Error response when a person cannot be created"""
    error_code: int
    message: str
    
# Define a union type (can return either a Person or an ErrorResponse)
ResponseType = Union[Person, ErrorResponse]

def main():
    # Create a prompt template with {{return_type}} placeholder
    # Note: We're using a raw string to avoid escaping issues
    description = "John is a 35-year-old software engineer living in Seattle who enjoys hiking and coding."
    
    prompt_template = f"""
    Create a profile for a person based on this description:
    "{description}"
    
    {{{{return_type}}}}
    """
    
    # Interpolate the type information into the prompt
    prompt = interpolate_prompt(prompt_template, Person, format_tag="return_type")
    
    print("GENERATED PROMPT:")
    print("=" * 40)
    print(prompt)
    print("=" * 40)
    
    # In a real application, you would send this prompt to an LLM
    # Here we'll simulate an LLM response
    
    llm_response = """
    I've created a profile based on your description:
    
    <Person>
    {
      "name": "John Smith",
      "age": 35,
      "address": {
        "street": "123 Pine Street",
        "city": "Seattle",
        "zip_code": "98101"
      },
      "hobbies": ["hiking", "coding", "reading tech blogs"]
    }
    </Person>
    
    Let me know if you need any other information about this person!
    """
    
    print("\nSIMULATED LLM RESPONSE:")
    print("=" * 40)
    print(llm_response)
    print("=" * 40)
    
    # Parse the LLM response
    parser = Parser(Person)
    
    # Process the LLM response in chunks (simulating streaming)
    chunks = [llm_response[i:i+50] for i in range(0, len(llm_response), 50)]
    
    for chunk in chunks:
        result = parser.feed(chunk)
        if result:
            print(f"\nParsed partial result: {result.__dict__ if hasattr(result, '__dict__') else result}")
    
    # Get the final validated result
    person = parser.validate()
    
    print("\nFINAL PARSED RESULT:")
    print("=" * 40)
    if person:
        print(f"Name: {person.name}")
        print(f"Age: {person.age}")
        print(f"City: {person.address.city}")
        print(f"Hobbies: {', '.join(person.hobbies)}")
    else:
        print("No valid person data found in response.")
    
if __name__ == "__main__":
    main()
