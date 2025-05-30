#!/usr/bin/env python3
"""Debug generic Union parsing to understand the issue."""

from typing import Union, get_origin, get_args
from gasp import Parser, Deserializable

class A(Deserializable):
    """First type in union"""
    name: str
    value_a: int

class B(Deserializable):
    """Second type in union"""
    title: str  
    value_b: float

# Create a generic Union
union_type = Union[A, B]

print("=== Union Type Analysis ===")
print(f"Union type: {union_type}")
print(f"Origin: {get_origin(union_type)}")
print(f"Args: {get_args(union_type)}")
print(f"Arg[0] name: {get_args(union_type)[0].__name__}")
print(f"Arg[1] name: {get_args(union_type)[1].__name__}")
print()

# Test the parser extraction
print("=== Testing Parser Extraction ===")
parser = Parser(union_type)

# Check what tags the parser is looking for
print(f"Expected tags: {parser.parser.expected_tags if hasattr(parser, 'parser') else 'N/A'}")

# Feed some data with explicit type name
test_data = '<Union>{"_type_name": "A", "name": "Test", "value_a": 42}</Union>'
print(f"\nFeeding: {test_data}")
parser.feed(test_data)
result = parser.validate()

print(f"Result type: {type(result)}")
print(f"Result: {result}")

# Try to understand the internal type info
if hasattr(parser, 'parser') and hasattr(parser.parser, 'type_info'):
    type_info = parser.parser.type_info
    if type_info:
        print(f"\nType info kind: {type_info.kind}")
        print(f"Type info name: {type_info.name}")
        print(f"Type info args: {len(type_info.args) if hasattr(type_info, 'args') else 0}")
        if hasattr(type_info, 'args') and type_info.args:
            for i, arg in enumerate(type_info.args):
                print(f"  Arg[{i}]: name={arg.name}, has_py_type={arg.py_type is not None}")
