#!/usr/bin/env python3
"""
Example demonstrating how to use union types with GASP template generation.

This shows how to handle cases where the LLM response might be one of several types,
such as a success response or an error response.
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
    # Generate format instructions for the union type, including structure examples
    instructions = type_to_format_instructions(ResponseType)
    
    print("ENHANCED FORMAT INSTRUCTIONS WITH STRUCTURE EXAMPLES:")
    print("=" * 60)
    print(instructions)
    print("=" * 60)
    print("\nNotice how the format instructions now include:")
    print("1. Clear option labels for union types")
    print("2. Structure examples for complex types")
    print("3. A reminder to use valid JSON format")
    
    # Create a prompt template with the union type
    prompt_template = """
    Try to perform the following operation: {operation}
    
    {{return_type}}
    """
    
    # Interpolate with the union type - ensure we use the same "Response" tag name
    prompt = interpolate_prompt(prompt_template, ResponseType, "return_type")
    operation = "Find user with ID 12345"
    
    print("\nGENERATED PROMPT:")
    print("=" * 40)
    print(prompt.replace("{operation}", operation))
    print("=" * 40)
    
    # Simulate two different LLM responses
    
    # Success case
    success_response = """
    <ResponseType>
    {
      "status": "success",
      "data": {
        "user_id": 12345,
        "username": "johndoe",
        "email": "john@example.com"
      },
      "message": "User found successfully"
    }
    </ResponseType>
    """
    
    # Error case
    error_response = """
    <ResponseType>
    {
      "status": "error",
      "error_code": 404,
      "message": "User not found",
      "details": [
        "No user with ID 12345 exists in the database",
        "Try searching by username instead"
      ]
    }
    </ResponseType>
    """
    
    # For union types, Parser returns a dictionary that we need to convert to the appropriate class
    # based on discriminating fields
    
    print("\nPARSING SUCCESS RESPONSE:")
    print("=" * 40)
    # Use the union type to parse
    success_parser = Parser(ResponseType)
    success_parser.feed(success_response)
    result = success_parser.validate()
    
    if result:
        print(f"Result type: {type(result).__name__}")
        if isinstance(result, SuccessResponse):
            print(f"Status: {result.status}")
            print(f"Data: {result.data}")
            print(f"Message: {result.message}")
        elif isinstance(result, dict):
            # If we get a dict back, we need to discriminate based on fields
            if result.get('_type_name') == 'SuccessResponse' or result.get('status') == 'success':
                success_result = SuccessResponse.__gasp_from_partial__(result)
                print(f"Status: {success_result.status}")
                print(f"Data: {success_result.data}")
                print(f"Message: {success_result.message}")
        else:
            print(f"Unexpected result type: {type(result)}")
    else:
        print("No result returned from parser")
    
    print("\nPARSING ERROR RESPONSE:")
    print("=" * 40)
    error_parser = Parser(ResponseType)  # Use the union type
    error_parser.feed(error_response)
    result = error_parser.validate()
    
    if result:
        print(f"Result type: {type(result).__name__}")
        if isinstance(result, ErrorResponse):
            print(f"Status: {result.status}")
            print(f"Error code: {result.error_code}")
            print(f"Message: {result.message}")
            print(f"Details: {', '.join(result.details or [])}")
        elif isinstance(result, dict):
            # If we get a dict back, we need to discriminate based on fields
            if result.get('_type_name') == 'ErrorResponse' or result.get('status') == 'error':
                error_result = ErrorResponse.__gasp_from_partial__(result)
                print(f"Status: {error_result.status}")
                print(f"Error code: {error_result.error_code}")
                print(f"Message: {error_result.message}")
                print(f"Details: {', '.join(error_result.details or [])}")
        else:
            print(f"Unexpected result type: {type(result)}")
    else:
        print("No result returned from parser")

if __name__ == "__main__":
    main()
