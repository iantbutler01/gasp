"""Debug what tags are expected for list parsing"""

from gasp import Parser
from typing import List

# Define a simple class
class Person:
    def __init__(self):
        self.name = None
        self.age = None
    
    def __repr__(self):
        return f"Person(name={self.name!r}, age={self.age})"

# Check different list parsers
print("=== Testing tag setup for different list types ===")

# Test 1: Plain list
try:
    parser1 = Parser(list)
    print(f"Parser(list) created successfully")
    # The parser doesn't expose expected_tags directly, so let's test what it accepts
    
    # Try with <list> tag
    result = parser1.feed('<list>[1, 2, 3]</list>')
    print(f"  Accepts <list> tag: {result is not None} (result: {result})")
    
except Exception as e:
    print(f"Error with Parser(list): {e}")

# Test 2: List[Person]
print("\n")
try:
    parser2 = Parser(List[Person])
    print(f"Parser(List[Person]) created successfully")
    
    # Try complete parsing
    complete = '<list>[{"name": "Alice", "age": 30}]</list>'
    result = parser2.feed(complete)
    print(f"  Result from complete JSON: {result}")
    print(f"  Type of result: {type(result)}")
    
    # Check if it's expecting a different tag
    # Let's try without any tag
    parser3 = Parser(List[Person])
    result2 = parser3.feed('[{"name": "Bob", "age": 25}]')
    print(f"  Result without tags: {result2}")
    
except Exception as e:
    print(f"Error with Parser(List[Person]): {e}")
    import traceback
    traceback.print_exc()

# Test 3: See what happens with a typed parser
print("\n=== Testing with ignored tags ===")
parser4 = Parser(List[Person], ignored_tags=[])  # Override default ignored tags
complete2 = '<list>[{"name": "Carol", "age": 35}]</list>'
result3 = parser4.feed(complete2)
print(f"Result with no ignored tags: {result3}")

# Test 4: Try parsing step by step
print("\n=== Testing step-by-step parsing ===")
parser5 = Parser(List[Person])
chunks = [
    '<list>[',
    '{"name": "Dave", "age": 40}',
    ']</list>'
]

for i, chunk in enumerate(chunks):
    print(f"Chunk {i}: {chunk!r}")
    result = parser5.feed(chunk)
    print(f"  Result: {result}")
    if result and isinstance(result, list):
        for j, item in enumerate(result):
            print(f"    [{j}] {item}")
