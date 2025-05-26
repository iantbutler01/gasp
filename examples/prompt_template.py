#!/usr/bin/env python3
"""
Example showing how to use GASP to generate type-aware prompts and parse LLM responses.
"""
from typing import List, Optional, Union
from gasp import Deserializable, Parser
from gasp.template_helpers import interpolate_prompt

# Define our data models
class Address(Deserializable):
    """Physical address"""
    street: str  # Street name and number
    city: str    # City name
    zip_code: str # Postal code
    
class Person(Deserializable):
    """Information about a person"""
    name: str
    age: int
    address: Address
    hobbies: Optional[List[str]] = None

class ErrorResponse(Deserializable):
    """Error response when a person cannot be created"""
    error_code: int
    message: str
    
# Define a union type (can return either a Person or an ErrorResponse)
ResponseType = Union[Person, ErrorResponse]

# Define more complex types to demonstrate enhanced structure examples
class Company(Deserializable):
    """Information about a company"""
    name: str
    industry: str
    founded_year: int
    headquarters: Address
    employees: List[Person]  # List of complex objects

def main():
    # Create a prompt template with {{return_type}} placeholder
    # Note: We're using a raw string to avoid escaping issues
    description = "Acme Corp is a technology company founded in 2010, headquartered in San Francisco, with several employees including John (35, engineer) and Sarah (42, designer)."
    
    prompt_template = f"""
    Create a profile for a company based on this description:
    "{description}"
    
    {{{{return_type}}}}
    """
    
    # Interpolate the type information into the prompt
    # This will now include structure examples for complex nested types
    prompt = interpolate_prompt(prompt_template, Company, format_tag="return_type")
    
    print("ENHANCED FORMAT INSTRUCTIONS WITH STRUCTURE EXAMPLES:")
    print("=" * 60)
    print(prompt)
    print("=" * 60)
    
    print("\nNotice how the format instructions now include:")
    print("1. Descriptive format for lists of complex objects")
    print("2. Structure examples for each complex type")
    print("3. A reminder to use valid JSON format")
    
    # In a real application, you would send this prompt to an LLM
    # Here we'll simulate an LLM response for a Company
    
    llm_response = """
    Here's the company profile based on your description:
    
    <Company>
    {
      "name": "Acme Corp",
      "industry": "Technology",
      "founded_year": 2010,
      "headquarters": {
        "street": "123 Market Street",
        "city": "San Francisco",
        "zip_code": "94105"
      },
      "employees": [
        {
          "name": "John Doe",
          "age": 35,
          "address": {
            "street": "456 Pine St",
            "city": "San Francisco",
            "zip_code": "94102"
          }
        },
        {
          "name": "Sarah Smith",
          "age": 42,
          "address": {
            "street": "789 Oak St",
            "city": "San Francisco",
            "zip_code": "94103"
          }
        }
      ]
    }
    </Company>
    """
    
    print("\nSIMULATED LLM RESPONSE:")
    print("=" * 40)
    print(llm_response)
    print("=" * 40)
    
    # Parse the LLM response
    parser = Parser(Company)
    
    # Process the LLM response in chunks (simulating streaming)
    chunks = [llm_response[i:i+50] for i in range(0, len(llm_response), 50)]
    
    for i, chunk in enumerate(chunks[:5]):  # Show just the first few chunks to keep output manageable
        result = parser.feed(chunk)
        if result:
            print(f"\nChunk {i+1} partial result: {type(result).__name__} object")
    
    # Feed the rest silently
    for chunk in chunks[5:]:
        parser.feed(chunk)
    
    # Get the final validated result
    company = parser.validate()
    
    print("\nFINAL PARSED RESULT:")
    print("=" * 40)
    if company:
        print(f"Company: {company.name}")
        print(f"Industry: {company.industry}")
        print(f"Founded: {company.founded_year}")
        print(f"Headquarters: {company.headquarters.city}, {company.headquarters.zip_code}")
        print(f"Number of employees: {len(company.employees)}")
        print(f"Employees:")
        
        # Convert employee dictionaries to Person objects
        for emp_dict in company.employees:
            # Use __gasp_from_partial__ to convert dictionary to Person object
            emp = Person.__gasp_from_partial__(emp_dict)
            print(f"  - {emp.name}, {emp.age}")
    else:
        print("No valid company data found in response.")
    
if __name__ == "__main__":
    main()
