"""Debug list streaming with non-Deserializable classes"""

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

print("=== Testing list of non-Deserializable objects ===")

try:
    parser = Parser(List[Person])
    print(f"Parser created successfully")
    
    list_chunks = [
        '<list>\n[',
        '\n  {"name": "Bob", "age": 25}',
        ',\n  {"name": "Carol", "age": 35',
        ', "email": "carol@example.com"}',
        '\n]\n</list>'
    ]
    
    for i, chunk in enumerate(list_chunks):
        print(f"\nFeeding chunk {i+1}: {chunk!r}")
        try:
            result = parser.feed(chunk)
            print(f"Result: {result}")
            if result:
                print(f"  Type of result: {type(result)}")
                if isinstance(result, list):
                    for j, item in enumerate(result):
                        print(f"  [{j}] {item} (type: {type(item).__name__})")
        except Exception as e:
            print(f"Error during feed: {e}")
            import traceback
            traceback.print_exc()
    
    print(f"\nParser complete? {parser.is_complete()}")
    
except Exception as e:
    print(f"Error creating parser: {e}")
    import traceback
    traceback.print_exc()
