from typing import Union, get_origin, get_args
from gasp import Deserializable
from gasp.template_helpers import type_to_format_instructions

class SuccessResponse(Deserializable):
    status: str = "success"
    data: dict

class ErrorResponse(Deserializable):
    status: str = "error"
    error_code: int

# Define a union type using 'type' statement
type ResponseType = Union[SuccessResponse, ErrorResponse]

print(f"ResponseType: {ResponseType}")
print(f"get_origin(ResponseType): {get_origin(ResponseType)}")
print(f"get_args(ResponseType): {get_args(ResponseType)}")

# Try with explicit Union
ExplicitUnion = Union[SuccessResponse, ErrorResponse]
print(f"\nExplicitUnion: {ExplicitUnion}")
print(f"get_origin(ExplicitUnion): {get_origin(ExplicitUnion)}")
print(f"get_args(ExplicitUnion): {get_args(ExplicitUnion)}")

# Test format instructions
print("\n\nWith ResponseType:")
print(type_to_format_instructions(ResponseType))

print("\n\nWith ExplicitUnion:")
print(type_to_format_instructions(ExplicitUnion, name="Response"))

# Test actual parsing
print("\n\n=== Testing Parser with Generic Union ===")
from gasp import Parser

# Test with generic Union
parser_generic = Parser(ExplicitUnion)

# Let's check what the parser knows about the type
if hasattr(parser_generic, 'parser'):
    print(f"Parser has internal parser: {hasattr(parser_generic.parser, 'type_info')}")
    if hasattr(parser_generic.parser, 'type_info') and parser_generic.parser.type_info:
        type_info = parser_generic.parser.type_info
        print(f"Type info kind: {type_info.kind}")
        print(f"Type info name: {type_info.name}")
        print(f"Type info args count: {len(type_info.args) if hasattr(type_info, 'args') else 0}")

test_data = '<Union>{"status": "success", "data": {"key": "value"}}</Union>'
print(f"Feeding: {test_data}")
parser_generic.feed(test_data)
result = parser_generic.validate()
print(f"Result type: {type(result)}")
print(f"Result: {result}")
print(f"Is SuccessResponse? {isinstance(result, SuccessResponse)}")

# Test with named type alias
print("\n=== Testing Parser with Named Type Alias ===")
parser_named = Parser(ResponseType)
test_data2 = '<ResponseType>{"status": "error", "error_code": 404}</ResponseType>'
print(f"Feeding: {test_data2}")
parser_named.feed(test_data2)
result2 = parser_named.validate()
print(f"Result type: {type(result2)}")
print(f"Result: {result2}")
print(f"Is ErrorResponse? {isinstance(result2, ErrorResponse)}")
