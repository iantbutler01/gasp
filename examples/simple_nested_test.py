#!/usr/bin/env python3
"""
Simple test for nested object creation in GASP
"""

from gasp import Deserializable, Parser

# Define simple nested types
class Child(Deserializable):
    """A child class for testing nested types"""
    name: str
    age: int
    
    def __repr__(self):
        return f"Child(name='{self.name}', age={self.age})"

class Parent(Deserializable):
    """A parent class with a child as a nested field"""
    name: str
    child: Child
    
    def __repr__(self):
        return f"Parent(name='{self.name}', child={self.child})"

def test_direct_instantiation():
    """Test that the Deserializable classes work correctly with direct instantiation"""
    print("=== Testing direct instantiation ===")
    
    # Create the objects directly
    child = Child.__gasp_from_partial__({"name": "Junior", "age": 5})
    parent = Parent.__gasp_from_partial__({"name": "Senior", "child": child})
    
    print(f"Child type: {type(child)}")
    print(f"Child: {child}")
    print(f"Parent type: {type(parent)}")
    print(f"Parent: {parent}")
    print(f"Parent's child type: {type(parent.child)}")
    
    # We expect parent.child to be a Child object, not a dict
    assert isinstance(parent.child, Child), f"Expected Child but got {type(parent.child)}"
    print("Direct instantiation test passed!")

def test_parser():
    """Test that the Parser correctly handles nested types"""
    print("\n=== Testing parser with nested types ===")
    
    # Create a parser for the Parent type
    parser = Parser(Parent)
    
    # Test with JSON data that includes a nested Child object
    json_data = '''<Parent>
    {
        "name": "Senior",
        "child": {
            "name": "Junior",
            "age": 5
        }
    }
    </Parent>'''
    
    # Parse the data
    result = parser.feed(json_data)
    print(f"Parsed result: {result}")
    print(f"Result type: {type(result)}")
    
    # Check if parsing is complete
    print(f"Is complete: {parser.is_complete()}")
    
    # Get and print information about the nested object
    if result and hasattr(result, 'child'):
        print(f"Child type: {type(result.child)}")
        print(f"Child: {result.child}")
        
        # We expect result.child to be a Child object, not a dict
        assert isinstance(result.child, Child), f"Expected Child but got {type(result.child)}"
        
        # Access properties of the child to confirm it works
        print(f"Child's name: {result.child.name}")
        print(f"Child's age: {result.child.age}")
        print("Parser nested type test passed!")
    else:
        print("ERROR: Child attribute missing or not properly instantiated")

if __name__ == "__main__":
    test_direct_instantiation()
    test_parser()
