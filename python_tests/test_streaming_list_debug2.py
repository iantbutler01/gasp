"""Debug list streaming with non-Deserializable classes"""

from gasp import Parser
from typing import List, get_origin, get_args

# Define a simple class without Deserializable base
class Person:
    def __init__(self):
        # Initialize with empty values
        self.name = None
        self.age = None
        self.email = None
    
    def __repr__(self):
        return f"Person(name={self.name!r}, age={self.age}, email={self.email!r})"

print("=== Testing list of non-Deserializable objects ===")

# First check the type
list_type = List[Person]
print(f"List type: {list_type}")
print(f"Origin: {get_origin(list_type)}")
print(f"Args: {get_args(list_type)}")

# Try without tags
try:
    parser = Parser(list_type)
    print(f"\nParser created successfully")
    
    # Try simple JSON array without tags
    simple_chunks = [
        '[',
        '{"name": "Bob", "age": 25}',
        ',{"name": "Carol", "age": 35, "email": "carol@example.com"}',
        ']'
    ]
    
    print("\n--- Testing simple JSON array (no tags) ---")
    for i, chunk in enumerate(simple_chunks):
        print(f"\nFeeding chunk {i+1}: {chunk!r}")
        result = parser.feed(chunk)
        print(f"Result: {result}")
        if result:
            print(f"  Type of result: {type(result)}")
            if isinstance(result, list):
                for j, item in enumerate(result):
                    print(f"  [{j}] {item} (type: {type(item).__name__})")
    
    print(f"\nParser complete? {parser.is_complete()}")
    
except Exception as e:
    print(f"Error: {e}")
    import traceback
    traceback.print_exc()

# Also test with regular Person parsing to verify it works
print("\n\n=== Testing single Person parsing for comparison ===")
parser2 = Parser(Person)
result = parser2.feed('<Person>{"name": "Test", "age": 40}</Person>')
print(f"Single Person result: {result}")
