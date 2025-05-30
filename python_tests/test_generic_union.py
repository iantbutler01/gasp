#!/usr/bin/env python3
"""Test case for generic Union parsing and deserialization."""

from typing import Union
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

if __name__ == "__main__":
    test_generic_union()
    test_generic_union_no_typename()
    test_named_union()
