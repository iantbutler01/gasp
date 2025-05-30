from typing import Union, Type
import types
from gasp import Deserializable, Parser
from gasp.template_helpers import type_to_format_instructions

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

# Create dynamic classes with 'abc' module name (like Reson might do)
Search_webTool = create_dynamic_tool_class_with_module("search_web", "abc")
Analyze_textTool = create_dynamic_tool_class_with_module("analyze_text", "abc")

# Create output class
class ResearchReport(Deserializable):
    topic: str
    summary: str

# Create Union type
effective_output_type = Union[Search_webTool, Analyze_textTool, ResearchReport]

print("=== Testing Dynamic Class Union with 'abc' module ===")
print(f"Search_webTool = {Search_webTool}")
print(f"Search_webTool.__name__ = {Search_webTool.__name__}")
print(f"Search_webTool.__module__ = {Search_webTool.__module__}")
print(f"Search_webTool.__qualname__ = {getattr(Search_webTool, '__qualname__', 'N/A')}")

# Check what format instructions are generated
print("\n=== Format Instructions ===")
format_instructions = type_to_format_instructions(effective_output_type)
print(format_instructions)

# Let's also debug what's happening in the parser
print("\n=== Debugging Parser Type Info ===")
parser = Parser(effective_output_type)

# Add some debug output to see what's being stored
import json
from gasp import Parser as RustParser

# Create a simple debug function to see the internal state
def debug_parser_state(parser):
    """Try to inspect parser's internal state."""
    if hasattr(parser, 'parser'):
        rust_parser = parser.parser
        if hasattr(rust_parser, 'expected_tags'):
            print(f"Expected tags: {rust_parser.expected_tags}")
            
debug_parser_state(parser)

# Test parsing
print("\n=== Testing Parser ===")
test_json = '''<Union>
{
  "_type_name": "Search_webTool",
  "query": "test query"
}
</Union>'''

print(f"Parsing: {test_json}")
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
else:
    print(f"❌ ERROR: Unexpected result type: {type(result)}")
