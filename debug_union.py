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
