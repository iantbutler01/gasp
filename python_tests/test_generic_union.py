#!/usr/bin/env python3
"""Test case for generic Union parsing and deserialization."""

from typing import Union, List
from gasp import Parser, Deserializable

class A(Deserializable):
    """First type in union"""
    name: str
    value_a: int

class B(Deserializable):
    """Second type in union"""
    title: str  
    value_b: float

# Test 1: Using generic Union directly
def test_generic_union():
    print("=== Test 1: Generic Union ===")
    
    # Create parser with generic Union type
    parser = Parser(Union[A, B])
    
    # Test case A
    response_a = '<Union>{"_type_name": "A", "name": "Test A", "value_a": 42}</Union>'
    parser.feed(response_a)
    result_a = parser.validate()
    
    print(f"Result A type: {type(result_a)}")
    print(f"Result A: {result_a}")
    print(f"Is instance of A? {isinstance(result_a, A)}")
    print()
    
    # Test case B  
    parser_b = Parser(Union[A, B])
    response_b = '<Union>{"_type_name": "B", "title": "Test B", "value_b": 3.14}</Union>'
    parser_b.feed(response_b)
    result_b = parser_b.validate()
    
    print(f"Result B type: {type(result_b)}")
    print(f"Result B: {result_b}")
    print(f"Is instance of B? {isinstance(result_b, B)}")
    print()

# Test 2: Without _type_name discrimination
def test_generic_union_no_typename():
    print("=== Test 2: Generic Union without _type_name ===")
    
    # Create parser with generic Union type
    parser = Parser(Union[A, B])
    
    # Test case A (based on field matching)
    response_a = '<Union>{"name": "Test A", "value_a": 42}</Union>'
    parser.feed(response_a)
    result_a = parser.validate()
    
    print(f"Result A type: {type(result_a)}")
    print(f"Result A: {result_a}")
    print(f"Is instance of A? {isinstance(result_a, A)}")
    print()
    
    # Test case B (based on field matching)
    parser_b = Parser(Union[A, B])
    response_b = '<Union>{"title": "Test B", "value_b": 3.14}</Union>'
    parser_b.feed(response_b)
    result_b = parser_b.validate()
    
    print(f"Result B type: {type(result_b)}")
    print(f"Result B: {result_b}")
    print(f"Is instance of B? {isinstance(result_b, B)}")
    print()

# Test 3: Named type alias (for comparison)
type MyUnion = Union[A, B]

def test_named_union():
    print("=== Test 3: Named Union Type Alias ===")
    
    parser = Parser(MyUnion)
    
    # Test case A
    response_a = '<MyUnion>{"_type_name": "A", "name": "Test A", "value_a": 42}</MyUnion>'
    parser.feed(response_a)
    result_a = parser.validate()
    
    print(f"Result A type: {type(result_a)}")
    print(f"Result A: {result_a}")
    print(f"Is instance of A? {isinstance(result_a, A)}")
    print()

# Test 4: List of mixed Union items
def test_list_of_mixed_union_items():
    print("=== Test 4: List of Mixed Union Items ===")
    parser = Parser(List[Union[A, B]])

    mixed_list_data = '''
    <list>[
        {"_type_name": "A", "name": "First A", "value_a": 123},
        {"_type_name": "B", "title": "First B", "value_b": 45.67},
        {"_type_name": "A", "name": "Second A", "value_a": 890}
    ]</list>
    '''
    parser.feed(mixed_list_data)
    result = parser.validate()

    print(f"Result type: {type(result)}")
    print(f"Result: {result}")

    assert isinstance(result, list)
    assert len(result) == 3

    # First item (A)
    assert isinstance(result[0], A)
    assert result[0].name == "First A"
    assert result[0].value_a == 123

    # Second item (B)
    assert isinstance(result[1], B)
    assert result[1].title == "First B"
    assert result[1].value_b == 45.67

    # Third item (A)
    assert isinstance(result[2], A)
    assert result[2].name == "Second A"
    assert result[2].value_a == 890
    print("List of mixed union items test passed.")
    print()

if __name__ == "__main__":
    test_generic_union()
    test_generic_union_no_typename()
    test_named_union()
    test_list_of_mixed_union_items()
