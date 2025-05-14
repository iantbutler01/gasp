#!/usr/bin/env python3
"""
GASP Tag-Based Parser Example

This example demonstrates how to use GASP to parse typed data from LLM outputs.
The parser uses tags like <Person>...</Person> to identify what Python type to create.
"""

from gasp import Deserializable, Parser
from typing import List, Optional

# --- DEFINING TYPE CLASSES ---
# GASP works with standard Python type annotations to define your data structure.
# The Deserializable base class provides helpers for object instantiation.

class Address(Deserializable):
    """An address with street, city, and zip code"""
    street: str
    city: str
    zip_code: str

class Person(Deserializable):
    """A person with name, age, and address"""
    name: str
    age: int
    address: Address  # Nested complex type
    hobbies: Optional[List[str]] = None  # Optional field with a default value

def main():
    """Basic example: Parsing a Person object from JSON with tags"""
    print("=== Basic example ===")
    
    # Create a parser for the Person type
    # This tells GASP what class to instantiate when it sees <Person> tags
    parser = Parser(Person)
    
    # --- STREAMING CHUNKS ---
    # GASP can handle data in chunks as it arrives from an LLM
    # Note how each chunk is a partial JSON fragment
    
    # First chunk - Just has name and age
    chunk1 = '<Person>{"name": "Alice", "age": 30'
    result1 = parser.feed(chunk1)
    print("Chunk 1 result:", result1)  # Already creates a partial Person object!
    
    # Second chunk - Adds partial address
    chunk2 = ', "address": {"street": "123 Main St", "city": "Springfield"'
    result2 = parser.feed(chunk2)
    print("Chunk 2 result:", result2)  # Person with partial address
    
    # Third chunk - Completes the object
    chunk3 = ', "zip_code": "12345"}, "hobbies": ["reading", "coding"]}</Person>'
    result3 = parser.feed(chunk3)
    print("Chunk 3 result:", result3)  # Complete Person object
    
    # --- CHECKING COMPLETION ---
    # We can check if parsing is complete (all tags closed)
    print("Is complete:", parser.is_complete())  # True
    
    # --- GETTING VALIDATED RESULT ---
    # Get the final validated object
    validated = parser.validate()
    print("Validated result:", validated)
    
    # --- ACCESSING THE DATA ---
    # We can access fields directly since it's a proper Python object
    if validated:
        print(f"\nPerson details:")
        print(f"  Name: {validated.name}")
        print(f"  Age: {validated.age}")
        print(f"  Address: {validated.address.street}, {validated.address.city}")
        print(f"  Hobbies: {', '.join(validated.hobbies or [])}")

def pydantic_example():
    """Example showing integration with Pydantic models"""
    print("=== Pydantic example ===")
    
    # --- PYDANTIC INTEGRATION ---
    try:
        from pydantic import BaseModel
    except ImportError:
        print("Pydantic not installed, skipping example")
        return
    
    # Define our data model using Pydantic instead of Deserializable
    class PydanticAddress(BaseModel):
        street: str
        city: str
        zip_code: str
        
    class PydanticPerson(BaseModel):
        name: str
        age: int
        address: PydanticAddress
        hobbies: Optional[List[str]] = None
    
    # --- CREATING PARSER FROM PYDANTIC MODEL ---
    # GASP has special support for Pydantic models
    parser = Parser.from_pydantic(PydanticPerson)
    
    # Feed chunks with proper tags to indicate types  
    # Note that the tag name must match the Pydantic class name
    chunks = [
        '<PydanticPerson>{"name": "Bob", "age": 25',
        ', "address": {"street": "456 Oak Ave", "city": "Rivertown"',
        ', "zip_code": "67890"}, "hobbies": ["gaming", "hiking"]}</PydanticPerson>'
    ]
    
    # Process each chunk
    for i, chunk in enumerate(chunks, 1):
        result = parser.feed(chunk)
        print(f"Chunk {i} result:", result)
    
    # --- GETTING VALIDATED RESULT ---
    validated = parser.validate()
    print("Validated result:", validated)
    
    # --- CONVERTING TO PYDANTIC OBJECT ---
    # By default, GASP returns dictionaries for Pydantic models
    # We can convert these to proper Pydantic objects
    if validated:
        # Create a Pydantic model from the parsed dict
        pydantic_obj = PydanticPerson.model_validate(validated)
        
        # Now we can use all Pydantic features like validation, serialization, etc.
        data = pydantic_obj.model_dump()
        print("As dict:", data)
        
        # Access nested fields with proper typing
        print(f"\nPydantic Person details:")
        print(f"  Name: {pydantic_obj.name}")
        print(f"  Address: {pydantic_obj.address.street}, {pydantic_obj.address.city}")

if __name__ == "__main__":
    main()
    print()  # Add a blank line between examples
    pydantic_example()
