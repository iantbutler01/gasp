#!/usr/bin/env python3
"""
Example demonstrating incremental parsing with union type aliases.

This shows how GASP can handle partial data as it arrives from an LLM,
even with complex union types, providing partial results during streaming.
"""
from typing import List, Optional, Union
from gasp import Deserializable, Parser
from gasp.template_helpers import type_to_format_instructions, interpolate_prompt

# Define our data models
class SuccessResponse(Deserializable):
    """Successful response with data"""
    status: str = "success"
    data: dict
    message: Optional[str] = None

class ErrorResponse(Deserializable):
    """Error response when operation fails"""
    status: str = "error"
    error_code: int
    message: str
    details: Optional[List[str]] = None
    
# Define a union type (can return either a success or error)
type ResponseType = Union[SuccessResponse, ErrorResponse]

def main():
    print("=== Incremental Parsing with Union Type Aliases ===\n")
    
    # Create a parser for the union type
    parser = Parser(ResponseType)
    
    print("TEST 1: Incremental Success Response")
    print("-" * 40)
    
    # Simulate chunks arriving for a success response
    success_chunks = [
        '<ResponseType>{"status": "success"',  # Just status field
        ', "data": {"user_id": 12345',         # Start of data object
        ', "username": "johndoe", "email"',    # More data fields
        ': "john@example.com"}, "message"',    # Complete data, start message
        ': "User found successfully"}</ResponseType>'  # Complete response
    ]
    
    # Feed each chunk and show the result
    for i, chunk in enumerate(success_chunks, 1):
        result = parser.feed(chunk)
        print(f"\nChunk {i}: '{chunk}'")
        print(f"Parser result: {result}")
        print(f"Is complete: {parser.is_complete()}")
        
        # If we have a result, show what type it is
        if result:
            if isinstance(result, dict):
                print(f"Result type: dict with status='{result.get('status')}'")
                # Try to create the appropriate object type
                if result.get('status') == 'success':
                    try:
                        obj = SuccessResponse.__gasp_from_partial__(result)
                        print(f"Can create SuccessResponse: {obj}")
                    except Exception as e:
                        print(f"Cannot create SuccessResponse yet: {e}")
    
    # Get final validated result
    validated = parser.validate()
    print(f"\nFinal validated result: {validated}")
    
    print("\n\nTEST 2: Incremental Error Response")
    print("-" * 40)
    
    # Create a new parser for the error case
    error_parser = Parser(ResponseType)
    
    # Simulate chunks arriving for an error response
    error_chunks = [
        '<ResponseType>{"status": "error", "error_code"',  # Status and start of error_code
        ': 404, "message": "User not found"',              # Complete error_code and message
        ', "details": ["No user with ID',                  # Start of details array
        ' 12345 exists in the database"',                  # More detail text
        ', "Try searching by username instead"]',          # Complete details array
        '}</ResponseType>'                                  # Close the response
    ]
    
    # Feed each chunk and show the result
    for i, chunk in enumerate(error_chunks, 1):
        result = error_parser.feed(chunk)
        print(f"\nChunk {i}: '{chunk}'")
        print(f"Parser result: {result}")
        print(f"Is complete: {error_parser.is_complete()}")
        
        # If we have a result, show what type it is
        if result:
            if isinstance(result, dict):
                print(f"Result type: dict with status='{result.get('status')}'")
                # Try to create the appropriate object type
                if result.get('status') == 'error':
                    try:
                        obj = ErrorResponse.__gasp_from_partial__(result)
                        print(f"Can create ErrorResponse: {obj}")
                        if hasattr(obj, 'details') and obj.details:
                            print(f"  - Details so far: {obj.details}")
                    except Exception as e:
                        print(f"Cannot create ErrorResponse yet: {e}")
    
    # Get final validated result
    validated = error_parser.validate()
    print(f"\nFinal validated result: {validated}")
    if validated and validated.get('status') == 'error':
        error_obj = ErrorResponse.__gasp_from_partial__(validated)
        print(f"Error details: {error_obj.details}")
    
    print("\n\nTEST 3: Type Discrimination During Streaming")
    print("-" * 40)
    
    # Show that we can determine the type early in the stream
    discrimination_parser = Parser(ResponseType)
    
    # Just feed the beginning with the discriminator field
    first_chunk = '<ResponseType>{"status": "error"'
    result = discrimination_parser.feed(first_chunk)
    
    print(f"First chunk: '{first_chunk}'")
    print(f"Result: {result}")
    
    if result and isinstance(result, dict):
        status = result.get('status')
        print(f"\nEarly type discrimination:")
        print(f"  - Status field: '{status}'")
        print(f"  - This will be an: {'ErrorResponse' if status == 'error' else 'SuccessResponse'}")
        print(f"  - Even though we only have partial data!")

if __name__ == "__main__":
    main()
