# Tag-Based LLM Output Parsing

This document explains GASP's tag-based parsing system for extracting structured data from LLM outputs.

## Overview

GASP uses a tag-based approach to identify and extract structured data from LLM outputs. This approach provides several benefits:

1. **Clear boundaries** - Tags clearly mark the beginning and end of structured data
2. **Type identification** - The tag name identifies what type the data should be converted into
3. **Context separation** - Text outside tags is ignored, allowing LLMs to provide explanations
4. **Embedded structures** - Multiple structured objects can exist in a single LLM response

## Tag Format

GASP expects tags to follow an XML-like format:

```
<TagName>structured data content</TagName>
```

Where:
- `TagName` is the name of the Python type to instantiate (case-sensitive)
- The content between tags contains JSON data (with error recovery for common LLM quirks)

The parser handles streaming of both tags and content, so it can process data incrementally as it arrives.

## Type Mapping

The tag name directly influences what Python type is instantiated:

| Tag | Python Type |
|-----|-------------|
| `<Person>` | A `Person` class (searched for in current module, or any imported module) |
| `<List>` or `<Array>` | A Python `list` |
| `<Dict>` or `<Map>` | A Python `dict` |
| `<String>` | A Python `str` |
| `<Number>` | A Python `int` or `float` |
| `<Boolean>` | A Python `bool` |
| `<YourClassName>` | An instance of `YourClassName` |

## Streaming and Incremental Parsing

GASP can handle incomplete JSON fragments, making it suitable for streaming LLM outputs. For example:

```python
# First chunk
'<Person>{"name": "Alice", "age": 30'

# Second chunk 
', "address": {"street": "123 Main"'

# Third chunk
', "city": "Springfield"}, "active": true}</Person>'
```

Each chunk is processed incrementally, building up the object as data arrives. The parser uses a number of techniques to handle incomplete data:

1. Maintaining a stack-based parser state
2. Tracking object nesting depth
3. Merging partial objects with new information

## Type Safety and Validation

GASP provides two layers of type safety:

1. **Structure validation** - Ensures the JSON structure is valid (or can be repaired)
2. **Type validation** - Ensures the data matches the expected Python types

Type validation is based on Python's type annotations. GASP supports:

- Basic types (`str`, `int`, `float`, `bool`)
- Container types (`List[T]`, `Dict[K, V]`)
- Optional types (`Optional[T]`)
- Custom classes with annotations
- Pydantic models

## Error Recovery

The parser includes several error recovery strategies for common LLM output issues:

1. **Unquoted identifiers** - `{name: "Alice"}` → `{"name": "Alice"}`
2. **Single quoted strings** - `{'key': 'value'}` → `{"key": "value"}`
3. **Trailing commas** - `[1, 2, 3,]` → `[1, 2, 3]`
4. **Unquoted string values** - `{status: active}` → `{"status": "active"}`
5. **Type coercion** - Converting between compatible types when needed
6. **Missing braces/brackets** - Attempting to repair incomplete structures

These recovery strategies allow GASP to handle real-world LLM outputs that wouldn't parse with standard JSON parsers.

## Nested Tags and Complex Structures

GASP can handle nested tags and complex structures:

```
<UserResponse>
{
  "user": <User>{"name": "Alice", "id": 42}</User>,
  "permissions": <Permissions>["read", "write"]</Permissions>,
  "metadata": {
    "created_at": "2023-01-01",
    "active": true
  }
}
</UserResponse>
```

The parser correctly handles this structure, instantiating the appropriate types for each tagged section.

## Performance Considerations

The parser is implemented in Rust for performance, with Python bindings. This approach balances:

1. Fast parsing and type conversion (Rust)
2. Natural Python API and integration (PyO3 bindings)
3. Memory efficiency (incremental processing)

For very large outputs, stream processing is recommended to minimize memory usage.

## Custom Tag Handlers

Advanced users can implement custom tag handlers to extend the parser's capabilities:

```python
class CustomHandler(Deserializable):
    @classmethod
    def __gasp_from_partial__(cls, partial_data):
        # Custom processing of partial data
        return processed_instance
```

This allows for domain-specific parsing, normalization, and validation logic.

## Integration with LLM Frameworks

GASP is designed to work with any LLM framework or service. The tag-based approach is:

1. **Framework-agnostic** - Works with any LLM that can output tag-enclosed data
2. **Streaming-friendly** - Compatible with streaming APIs from providers
3. **Prompt-compatible** - Easy to include in prompts as a requested output format

No special prompt engineering is required beyond asking the LLM to format its response with the appropriate tags.
