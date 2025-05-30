from typing import Union, Type
from gasp import Deserializable, Parser

def create_dynamic_tool_class_with_module(name: str, module_name: str) -> Type:
    """Create a dynamic Deserializable class with a specific module name."""
    
    # Define __init__ method for Deserializable
    def __init__(self, **kwargs):
        for key, value in kwargs.items():
            setattr(self, key, value)
    
    # Build class attributes
    class_attrs = {
        '__annotations__': {'query': str} if 'search' in name else {'text': str, 'focus': str},
        '__doc__': f"Tool for {name}",
        '__init__': __init__,
        '_tool_name': name,
        '__module__': module_name,  # Set the module explicitly
    }
    
    # Create the class using type()
    tool_class = type(
        f"{name.capitalize()}Tool",
        (Deserializable,),  # Base class
        class_attrs
    )
    
    return tool_class

# Create dynamic classes with weird module names
Search_webTool = create_dynamic_tool_class_with_module("search_web", "some.weird.module")
Analyze_textTool = create_dynamic_tool_class_with_module("analyze_text", "another.module")

# Create output class
class ResearchReport(Deserializable):
    topic: str
    summary: str

# Create Union type
effective_output_type = Union[Search_webTool, Analyze_textTool, ResearchReport]

print("=== Testing Dynamic Class Union with weird modules ===")
print(f"Search_webTool = {Search_webTool}")
print(f"Search_webTool.__name__ = {Search_webTool.__name__}")
print(f"Search_webTool.__module__ = {Search_webTool.__module__}")

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
else:
    print(f"❌ ERROR: Unexpected result type: {type(result)}")
