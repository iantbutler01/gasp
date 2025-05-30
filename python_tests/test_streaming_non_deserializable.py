"""Test streaming/partial data with non-Deserializable classes"""

from gasp import Parser
from typing import List

# Define a simple class without Deserializable base
class Person:
    def __init__(self):
        # Initialize with empty values
        self.name = None
        self.age = None
        self.email = None
    
    def __repr__(self):
        return f"Person(name={self.name!r}, age={self.age}, email={self.email!r})"

# Define a class that accepts kwargs
class Product:
    def __init__(self, **kwargs):
        self.id = kwargs.get('id')
        self.name = kwargs.get('name')
        self.price = kwargs.get('price')
    
    def __repr__(self):
        return f"Product(id={self.id}, name={self.name!r}, price={self.price})"

# Test streaming with Person class
print("=== Testing streaming with Person class (no kwargs) ===")
parser = Parser(Person)

# Feed data incrementally
chunks = [
    '<Person>\n{',
    '\n  "name": "Alice"',
    ',\n  "age": 30',
    ',\n  "email": "alice@example.com"',
    '\n}\n</Person>'
]

for i, chunk in enumerate(chunks):
    result = parser.feed(chunk)
    if result:
        print(f"After chunk {i+1}: {result}")
        print(f"  Type: {type(result)}")
        print(f"  Same instance? {i > 0 and 'Yes' or 'First result'}")

print(f"\nFinal result complete? {parser.is_complete()}")

# Test streaming with Product class
print("\n=== Testing streaming with Product class (accepts kwargs) ===")
parser2 = Parser(Product)

# Feed data incrementally
chunks2 = [
    '<Product>\n{',
    '\n  "id": 123',
    ',\n  "name": "Laptop"',
    ',\n  "price": 999.99',
    '\n}\n</Product>'
]

for i, chunk in enumerate(chunks2):
    result = parser2.feed(chunk)
    if result:
        print(f"After chunk {i+1}: {result}")
        print(f"  Type: {type(result)}")

print(f"\nFinal result complete? {parser2.is_complete()}")

# Test with a list of non-Deserializable objects
print("\n=== Testing list of non-Deserializable objects ===")

from typing import List

parser3 = Parser(List[Person])

list_chunks = [
    '<list>\n[',
    '\n  {"name": "Bob", "age": 25}',
    ',\n  {"name": "Carol", "age": 35',
    ', "email": "carol@example.com"}',
    '\n]\n</list>'
]

for i, chunk in enumerate(list_chunks):
    result = parser3.feed(chunk)
    if result:
        print(f"After chunk {i+1}:")
        for j, person in enumerate(result):
            print(f"  [{j}] {person} (type: {type(person).__name__})")
