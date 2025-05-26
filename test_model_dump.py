#!/usr/bin/env python3
"""
Test the recursive model_dump functionality
"""

from gasp import Deserializable
from typing import List

class Child(Deserializable):
    name: str
    age: int

class Parent(Deserializable):
    name: str
    child: Child
    children: list[Child]

def test_recursive_model_dump():
    """Test that model_dump recursively converts nested objects to dicts"""
    print("=== Testing recursive model_dump ===")
    
    # Create nested objects
    child1 = Child.__gasp_from_partial__({"name": "Alice", "age": 8})
    child2 = Child.__gasp_from_partial__({"name": "Bob", "age": 10})
    
    parent = Parent.__gasp_from_partial__({
        "name": "Parent", 
        "child": {"name": "Charlie", "age": 5},
        "children": [{"name": "Alice", "age": 8}, {"name": "Bob", "age": 10}]
    })
    
    print(f"Parent object: {parent}")
    print(f"Parent child type: {type(parent.child)}")
    print(f"Parent children[0] type: {type(parent.children[0])}")
    
    # Test model_dump - this should convert everything to dicts
    dumped = parent.model_dump()
    print(f"\nDumped parent: {dumped}")
    print(f"Dumped child type: {type(dumped['child'])}")
    print(f"Dumped children[0] type: {type(dumped['children'][0])}")
    
    # Verify that nested objects are now dicts
    assert isinstance(dumped['child'], dict), f"Expected dict but got {type(dumped['child'])}"
    assert isinstance(dumped['children'][0], dict), f"Expected dict but got {type(dumped['children'][0])}"
    
    # Verify the content is correct
    assert dumped['child']['name'] == 'Charlie'
    assert dumped['child']['age'] == 5
    assert dumped['children'][0]['name'] == 'Alice'
    assert dumped['children'][0]['age'] == 8
    assert dumped['children'][1]['name'] == 'Bob'
    assert dumped['children'][1]['age'] == 10
    
    print("✅ Recursive model_dump test passed!")

if __name__ == "__main__":
    test_recursive_model_dump()
