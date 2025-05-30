#!/usr/bin/env python3
"""
Test script to verify that all components of the GASP package load correctly.
"""
from typing import List, Optional

def test_gasp_modules():
    # Import the package
    import gasp
    
    # Check native components
    assert hasattr(gasp, 'Parser'), "Parser not found"
    assert hasattr(gasp, 'StreamParser'), "StreamParser not found"
    
    # Check pure Python components
    assert hasattr(gasp, 'Deserializable'), "Deserializable not found"
    assert hasattr(gasp, 'template_helpers'), "template_helpers not found"
    
    # Test template helpers
    from gasp.template_helpers import type_to_format_instructions, interpolate_prompt
    
    # Define a test class
    class Person(gasp.Deserializable):
        """A person with name and age"""
        name: str
        age: int
        hobbies: Optional[List[str]] = None
    
    # Test type_to_format_instructions
    instructions = type_to_format_instructions(Person)
    print("Format instructions:")
    print(instructions)
    assert "<Person>" in instructions, "Tag not found in instructions"
    
    # Test interpolate_prompt
    template = "Create a person: {{return_type}}"
    prompt = interpolate_prompt(template, Person)
    print("\nInterpolated prompt:")
    print(prompt)
    assert "Your response should be formatted as:" in prompt, "Format header not in prompt"
    
    # Test Deserializable
    # We should call it as a class method that creates an instance
    p = Person.__gasp_from_partial__({"name": "Alice", "age": 30})
    assert p.name == "Alice", f"Expected name 'Alice', got '{p.name}'"
    assert p.age == 30, f"Expected age 30, got {p.age}"
    
    # Test update method
    p.__gasp_update__({"name": "Bob"})
    assert p.name == "Bob", f"Expected updated name 'Bob', got '{p.name}'"
    
    print("\nAll tests passed!")

if __name__ == "__main__":
    test_gasp_modules()
