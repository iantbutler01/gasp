from typing import Union
from gasp import Deserializable, Parser

# Define the tool classes similar to what reson might have
class Search_webTool(Deserializable):
    query: str

class Analyze_textTool(Deserializable):
    text: str
    focus: str

class SearchResult(Deserializable):
    title: str
    url: str
    snippet: str

class ResearchReport(Deserializable):
    topic: str
    summary: str
    key_findings: list[str]
    sources: list[SearchResult]

# Create the union type
ToolUnion = Union[Search_webTool, Analyze_textTool, ResearchReport]

# Test parsing
print("Testing tool union parsing...")

# Create parser with the union type
parser = Parser(ToolUnion)

# The exact JSON from the error example
test_json = '''<Union>
{
  "_type_name": "Search_webTool",
  "query": "latest advancements in quantum computing 2023"
}
</Union>'''

print(f"\nParsing: {test_json}")

# Feed the data
parser.feed(test_json)
result = parser.validate()

print(f"\nResult type: {type(result)}")
print(f"Result: {result}")

# Check if it's the correct type
if isinstance(result, Search_webTool):
    print(f"✓ Correctly parsed as Search_webTool")
    print(f"  Query: {result.query}")
elif isinstance(result, dict):
    print(f"✗ ERROR: Parsed as dict instead of Search_webTool")
    print(f"  Dict contents: {result}")
else:
    print(f"✗ ERROR: Unexpected type: {type(result)}")

# Also test without the Union tags to see raw behavior
print("\n\nTesting without explicit Union tags...")
parser2 = Parser(ToolUnion)
test_json2 = '{"_type_name": "Search_webTool", "query": "latest advancements in quantum computing 2023"}'
parser2.feed(test_json2)
result2 = parser2.validate()

print(f"Result type: {type(result2)}")
print(f"Is Search_webTool? {isinstance(result2, Search_webTool)}")
