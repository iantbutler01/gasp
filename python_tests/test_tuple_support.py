#!/usr/bin/env python3
"""
Test that Tuple[T] types work correctly with <tuple> tags
"""

from gasp import Parser
from typing import Tuple

def test_basic_tuple():
    """Test basic tuple parsing"""
    print("=== Testing basic tuple parsing ===")
    
    # Test plain tuple
    parser = Parser(tuple)
    result = parser.feed('<tuple>[1, 2, 3]</tuple>')
    print(f"tuple result: {result}")
    print(f"Type: {type(result)}")
    print(f"Is tuple: {isinstance(result, tuple)}")
    print()

def test_typed_tuples():
    """Test typed tuple parsing"""
    print("=== Testing typed tuples ===")
    
    # Test homogeneous tuple (Tuple[int, ...])
    parser = Parser(Tuple[int, ...])
    result = parser.feed('<tuple>[1, 2, 3, 4, 5]</tuple>')
    print(f"Tuple[int, ...] result: {result}")
    print(f"Type: {type(result)}")
    print()
    
    # Test fixed tuple (Tuple[str, int, float])
    parser = Parser(Tuple[str, int, float])
    result = parser.feed('<tuple>["hello", 42, 3.14]</tuple>')
    print(f"Tuple[str, int, float] result: {result}")
    print(f"Type: {type(result)}")
    print()

def test_nested_tuples():
    """Test tuples containing complex types"""
    print("=== Testing nested tuples ===")
    
    # Tuple of lists
    parser = Parser(Tuple[list, list])
    result = parser.feed('<tuple>[[1, 2, 3], ["a", "b", "c"]]</tuple>')
    print(f"Tuple[list, list] result: {result}")
    print(f"Type: {type(result)}")
    print()

class Person:
    """Simple person class for testing"""
    def __init__(self, name: str = "", age: int = 0):
        self.name = name
        self.age = age
    
    def __repr__(self):
        return f"Person(name='{self.name}', age={self.age})"

def test_tuple_with_objects():
    """Test tuples containing objects"""
    print("=== Testing tuples with objects ===")
    
    # Tuple of Person objects
    parser = Parser(Tuple[Person, Person])
    result = parser.feed('<tuple>[{"name": "Alice", "age": 30}, {"name": "Bob", "age": 25}]</tuple>')
    print(f"Tuple[Person, Person] result: {result}")
    print(f"Type: {type(result)}")
    print(f"First person: {result[0]}")
    print(f"Second person: {result[1]}")
    print()

def test_streaming_tuple():
    """Test streaming tuple parsing"""
    print("=== Testing streaming tuple parsing ===")
    
    parser = Parser(Tuple[str, int, bool])
    
    chunks = [
        '<tuple>[',
        '"streaming", ',
        '123, ',
        'true]',
        '</tuple>'
    ]
    
    for i, chunk in enumerate(chunks):
        result = parser.feed(chunk)
        print(f"Chunk {i}: '{chunk}' -> {result}")
    
    print(f"Final result: {result}")
    print(f"Is complete: {parser.is_complete()}")
    print()

def test_tuple_vs_list():
    """Compare tuple and list parsing"""
    print("=== Comparing tuple and list parsing ===")
    
    # Same data, different types
    data_with_list_tag = '<list>[1, 2, 3]</list>'
    data_with_tuple_tag = '<tuple>[1, 2, 3]</tuple>'
    
    # Parse as list
    list_parser = Parser(list)
    list_result = list_parser.feed(data_with_list_tag)
    print(f"List result: {list_result}, type: {type(list_result)}")
    
    # Parse as tuple
    tuple_parser = Parser(tuple)
    tuple_result = tuple_parser.feed(data_with_tuple_tag)
    print(f"Tuple result: {tuple_result}, type: {type(tuple_result)}")
    
    # Verify types
    print(f"List is list: {isinstance(list_result, list)}")
    print(f"Tuple is tuple: {isinstance(tuple_result, tuple)}")
    print()

if __name__ == "__main__":
    test_basic_tuple()
    test_typed_tuples()
    test_nested_tuples()
    test_tuple_with_objects()
    test_streaming_tuple()
    test_tuple_vs_list()
    
    print("=== All tuple tests completed! ===")
