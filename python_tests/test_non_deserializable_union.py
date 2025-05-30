from typing import Union, Type
from gasp import Parser

def create_non_deserializable_tool_class(name: str, module_name: str) -> Type:
    """Create a dynamic class WITHOUT Deserializable base."""
    
    # Define __init__ method that accepts kwargs
    def __init__(self, **kwargs):
        for key, value in kwargs.items():
            setattr(self, key, value)
    
    # Build class attributes
    class_attrs = {
        '__annotations__': {'query': str} if 'search' in name else {'text': str, 'focus': str},
        '__doc__': f"Tool for {name}",
        '__init__': __init__,
        '_tool_name': name,
        '__module__': module_name,
    }
    
    # Create the class using type() WITHOUT Deserializable
    tool_class = type(
        f"{name.capitalize()}Tool",
        (),  # NO base class
        class_attrs
    )
    
    return tool_class

# Create dynamic classes without Deserializable
Search_webTool = create_non_deserializable_tool_class("search_web", "abc")
Analyze_textTool = create_non_deserializable_tool_class("analyze_text", "abc")

# Create output class (also without Deserializable)
class ResearchReport:
    topic: str
    summary: str
    def __init__(self, **kwargs):
        for key, value in kwargs.items():
            setattr(self, key, value)

# Create Union type
effective_output_type = Union[Search_webTool, Analyze_textTool, ResearchReport]

print("=== Testing Non-Deserializable Dynamic Class Union ===")
print(f"Search_webTool = {Search_webTool}")
print(f"Has Deserializable base? {any('Deserializable' in base.__name__ for base in Search_webTool.__mro__ if hasattr(base, '__name__'))}")

# Test parsing
parser = Parser(effective_output_type)

test_json = '''<Union>
{
  "_type_name": "Search_webTool",
  "query": "test query"
}
</Union>'''

print(f"\nParsing: {test_json}")
parser.feed(test_json)
result = parser.validate()

print(f"\nResult type: {type(result)}")
print(f"Result: {result}")

if isinstance(result, dict):
    print("❌ ERROR: Got dict instead of Search_webTool instance")
    print(f"Dict contents: {result}")
elif hasattr(result, 'query'):
    print("✅ SUCCESS: Got Search_webTool instance")
    print(f"Query: {result.query}")
    print(f"Result module: {result.__class__.__module__}")
    print(f"Result class name: {result.__class__.__name__}")
else:
    print(f"❌ ERROR: Unexpected result type: {type(result)}")
