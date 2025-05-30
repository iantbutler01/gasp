"""Debug list parsing with detailed output"""

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

print("=== Testing complete list parsing ===")

# Test 1: Complete JSON in one go
parser1 = Parser(List[Person])
complete_json = '<list>[{"name": "Alice", "age": 30}, {"name": "Bob", "age": 25}]</list>'
print(f"Feeding complete JSON: {complete_json}")
result1 = parser1.feed(complete_json)
print(f"Result: {result1}")
print(f"Complete: {parser1.is_complete()}")

# Test 2: Without tags
print("\n=== Testing without tags ===")
parser2 = Parser(List[Person])
no_tags_json = '[{"name": "Carol", "age": 35}, {"name": "Dave", "age": 28}]'
print(f"Feeding: {no_tags_json}")
result2 = parser2.feed(no_tags_json)
print(f"Result: {result2}")
print(f"Complete: {parser2.is_complete()}")

# Test 3: Direct list creation
print("\n=== Testing direct list creation ===")
parser3 = Parser(list)  # Just a plain list
plain_list = '<list>[1, 2, 3]</list>'
print(f"Feeding: {plain_list}")
result3 = parser3.feed(plain_list)
print(f"Result: {result3}")
print(f"Complete: {parser3.is_complete()}")
