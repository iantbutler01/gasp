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
ResponseType = Union[SuccessResponse, ErrorResponse]

def main():
    # Generate format instructions for the union type
    instructions = type_to_format_instructions(ResponseType, name="Response")
    
    print("FORMAT INSTRUCTIONS FOR UNION TYPE:")
    print("=" * 40)
    print(instructions)
    print("=" * 40)
    
    # Create a prompt template with the union type
    prompt_template = """
    Try to perform the following operation: {operation}
    
    {{return_type}}
    """
    
    # Interpolate with the union type - ensure we use the same "Response" tag name
    prompt = interpolate_prompt(prompt_template, ResponseType, "return_type", name="Response")
    operation = "Find user with ID 12345"
    
    print("\nGENERATED PROMPT:")
    print("=" * 40)
    print(prompt.replace("{operation}", operation))
    print("=" * 40)
    
    # Simulate two different LLM responses
    
    # Success case
    success_response = """
    <Response>
    {
      "status": "success",
      "data": {
        "user_id": 12345,
        "username": "johndoe",
        "email": "john@example.com"
      },
      "message": "User found successfully"
    }
    </Response>
    """
    
    # Error case
    error_response = """
    <Response>
    {
      "status": "error",
      "error_code": 404,
      "message": "User not found",
      "details": [
        "No user with ID 12345 exists in the database",
        "Try searching by username instead"
      ]
    }
    </Response>
    """
    
    # For union types, Parser returns a dictionary that we need to convert to the appropriate class
    # based on discriminating fields
    
    print("\nPARSING SUCCESS RESPONSE:")
    print("=" * 40)
    # First, use the tag name to get the raw dictionary
    success_parser = Parser(dict)  # Use dict instead of the union type
    success_parser.feed(success_response)
    result_dict = success_parser.validate()
    
    if result_dict:
        # Manual type discrimination based on 'status' field
        if result_dict.get('status') == 'success':
            # Convert to SuccessResponse
            success_result = SuccessResponse.__gasp_from_partial__(result_dict)
            print(f"Status: {success_result.status}")
            print(f"Data: {success_result.data}")
            print(f"Message: {success_result.message}")
        else:
            print(f"Unexpected status: {result_dict.get('status')}")
    
    print("\nPARSING ERROR RESPONSE:")
    print("=" * 40)
    error_parser = Parser(dict)  # Use dict instead of the union type
    error_parser.feed(error_response)
    result_dict = error_parser.validate()
    
    if result_dict:
        # Manual type discrimination based on 'status' field
        if result_dict.get('status') == 'error':
            # Convert to ErrorResponse
            error_result = ErrorResponse.__gasp_from_partial__(result_dict)
            print(f"Status: {error_result.status}")
            print(f"Error code: {error_result.error_code}")
            print(f"Message: {error_result.message}")
            print(f"Details: {', '.join(error_result.details or [])}")
        else:
            print(f"Unexpected status: {result_dict.get('status')}")

if __name__ == "__main__":
    main()
