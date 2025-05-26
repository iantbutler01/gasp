# Union Type Alias Parsing Fix Summary

## Problem
When using type aliases for unions (e.g., `type ResponseType = Union[SuccessResponse, ErrorResponse]`), the parser was:
1. Not recognizing the alias as a Union type
2. Using `<Union>` as the tag name instead of the alias name (e.g., `<ResponseType>`)
3. Unable to parse the responses back into the proper Python types

## Solution Implemented

### 1. Rust Parser Enhancement (`src/python_types.rs`)
Added special handling for type aliases at the beginning of `PyTypeInfo::extract_from_python`:
- Checks if the type has a `__value__` attribute
- If `__value__.__origin__` is `Union`, processes it as a union type alias
- Preserves the original alias name for the tag while extracting union type information

### 2. Python Deserializable Fix (`gasp/deserializable.py`)
Fixed the `issubclass` error by:
- Checking if `field_type` is actually a class before calling `issubclass`
- Wrapping in a try/except to handle type hints that aren't classes

### 3. Example Update (`examples/union_type_example.py`)
Updated to use the union type alias for both parsers instead of `dict`

## Results
- Prompt generation correctly shows `<ResponseType>` tags
- Parser successfully parses both success and error responses
- Returns dictionaries that can be discriminated based on fields
- Proper conversion to `SuccessResponse` or `ErrorResponse` instances

## Usage Example
```python
# Define union type alias
type ResponseType = Union[SuccessResponse, ErrorResponse]

# Use in parser
parser = Parser(ResponseType)
parser.feed(response_text)
result = parser.validate()

# Discriminate based on fields
if result.get('status') == 'success':
    success_obj = SuccessResponse.__gasp_from_partial__(result)
elif result.get('status') == 'error':
    error_obj = ErrorResponse.__gasp_from_partial__(result)
