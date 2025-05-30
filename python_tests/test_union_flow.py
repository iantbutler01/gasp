#!/usr/bin/env python3
"""Test to understand the flow of Union type handling."""

from typing import Union, get_origin, get_args
from gasp import Parser, Deserializable
import json

class A(Deserializable):
    """First type in union"""
    name: str
    value_a: int
    
    def __repr__(self):
        return f"A(name={self.name}, value_a={self.value_a})"

class B(Deserializable):
    """Second type in union"""
    title: str  
    value_b: float
    
    def __repr__(self):
        return f"B(title={self.title}, value_b={self.value_b})"

# Test with named type alias first (working case)
type NamedUnion = Union[A, B]

print("=== Named Union (Working) ===")
parser_named = Parser(NamedUnion)
parser_named.feed('<NamedUnion>{"_type_name": "A", "name": "Named Test", "value_a": 100}</NamedUnion>')
result_named = parser_named.validate()
print(f"Type: {type(result_named)}, Value: {result_named}")
print()

# Test with generic Union
print("=== Generic Union (Not Working) ===")
parser_generic = Parser(Union[A, B])
parser_generic.feed('<Union>{"_type_name": "A", "name": "Generic Test", "value_a": 200}</Union>')
result_generic = parser_generic.validate()
print(f"Type: {type(result_generic)}, Value: {result_generic}")
print()

# Let's also test if the classes themselves work with Parser
print("=== Direct Class Parsing ===")
parser_a = Parser(A)
parser_a.feed('<A>{"name": "Direct A", "value_a": 300}</A>')
result_a = parser_a.validate()
print(f"Type: {type(result_a)}, Value: {result_a}")
print()

# Check what happens if we manually deserialize
print("=== Manual Deserialization ===")
data = {"_type_name": "A", "name": "Manual Test", "value_a": 400}
manual_a = A.__gasp_from_partial__(data)
print(f"Type: {type(manual_a)}, Value: {manual_a}")
