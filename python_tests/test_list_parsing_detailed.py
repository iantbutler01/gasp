"""Detailed test to understand list parsing issue"""

from gasp import Parser
from typing import List
import json

# Define a simple class without Deserializable base
class Person:
    def __init__(self):
        # Initialize with empty values
        self.name = None
        self.age = None
        self.email = None
    
    def __repr__(self):
        return f"Person(name={self.name!r}, age={self.age}, email={self.email!r})"

# Test different scenarios
print("=== Test 1: Complete parsing with List[Person] ===")
parser1 = Parser(List[Person])
complete_json = '<list>[{"name": "Alice", "age": 30}, {"name": "Bob", "age": 25}]</list>'
print(f"Input: {complete_json}")

# Feed in one go
result = parser1.feed(complete_json)
print(f"Result: {result}")
print(f"Type: {type(result)}")
print(f"Complete: {parser1.is_complete()}")

# Test partial result
partial = parser1.get_partial()
print(f"Partial result: {partial}")

# Test 2: Try with a simpler Person that accepts kwargs
print("\n=== Test 2: Person class that accepts kwargs ===")
class PersonWithKwargs:
    def __init__(self, **kwargs):
        self.name = kwargs.get('name')
        self.age = kwargs.get('age')
        self.email = kwargs.get('email')
    
    def __repr__(self):
        return f"PersonWithKwargs(name={self.name!r}, age={self.age}, email={self.email!r})"

parser2 = Parser(List[PersonWithKwargs])
result2 = parser2.feed(complete_json)
print(f"Result: {result2}")
if result2 and isinstance(result2, list):
    for i, person in enumerate(result2):
        print(f"  [{i}] {person}")

# Test 3: Test with dict to see if JSON parsing works
print("\n=== Test 3: List[dict] parsing ===")
parser3 = Parser(List[dict])
result3 = parser3.feed(complete_json)
print(f"Result: {result3}")
print(f"Type: {type(result3)}")

# Test 4: Feed the same data in chunks
print("\n=== Test 4: Incremental parsing ===")
parser4 = Parser(List[PersonWithKwargs])

chunks = [
    '<list>[',
    '{"name": "Carol", "age": 35}',
    ', {"name": "Dave", "age": 28}',
    ']</list>'
]

for i, chunk in enumerate(chunks):
    print(f"\nChunk {i}: {chunk!r}")
    result = parser4.feed(chunk)
    print(f"  Result: {result}")
    if result and isinstance(result, list):
        print(f"  Length: {len(result)}")
        for j, person in enumerate(result):
            print(f"    [{j}] {person}")

print(f"\nFinal complete: {parser4.is_complete()}")
