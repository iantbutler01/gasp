from typing import Union, Type
from gasp import Deserializable, Parser
from gasp.template_helpers import type_to_format_instructions

def create_dynamic_tool_class(name: str) -> Type:
    """Create a dynamic Deserializable class similar to Reson's approach."""
    
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
    }
    
    # Create the class using type()
    tool_class = type(
        f"{name.capitalize()}Tool",
        (Deserializable,),  # Base class
        class_attrs
    )
    
    return tool_class

# Create dynamic classes like Reson does
Search_webTool = create_dynamic_tool_class("search_web")
Analyze_textTool = create_dynamic_tool_class("analyze_text")

# Create output class
class ResearchReport(Deserializable):
    topic: str
    summary: str

# Create Union type
effective_output_type = Union[Search_webTool, Analyze_textTool, ResearchReport]

print("=== Testing Dynamic Class Union ===")
print(f"Search_webTool.__name__ = {Search_webTool.__name__}")
print(f"Search_webTool.__module__ = {Search_webTool.__module__}")
print(f"Analyze_textTool.__name__ = {Analyze_textTool.__name__}")
print(f"Analyze_textTool.__module__ = {Analyze_textTool.__module__}")

# Check what format instructions are generated
print("\n=== Format Instructions ===")
format_instructions = type_to_format_instructions(effective_output_type)
print(format_instructions)

# Test parsing
print("\n=== Testing Parser ===")
parser = Parser(effective_output_type)

# Test with the JSON that LLM would generate based on format instructions
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
elif hasattr(result, 'query'):
    print("✅ SUCCESS: Got Search_webTool instance")
    print(f"Query: {result.query}")
else:
    print(f"❌ ERROR: Unexpected result type: {type(result)}")
