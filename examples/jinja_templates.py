#!/usr/bin/env python3
"""
Example showing how to use GASP with Jinja2 templates for more advanced prompt engineering.

This demonstrates:
1. Using Jinja2 templates with GASP type formatting
2. Conditional formatting based on template variables
3. Reusing type formatting across multiple prompts
"""
from typing import List, Optional, Union
from gasp import Deserializable, Parser
from gasp import render_template, render_file_template
from gasp.jinja_helpers import create_type_environment

# Define our data models
class Address(Deserializable):
    """Physical address with street, city and zip code"""
    street: str
    city: str
    zip_code: str

class Person(Deserializable):
    """Information about a person"""
    name: str
    age: int
    address: Address
    hobbies: Optional[List[str]] = None

class ErrorResponse(Deserializable):
    """Error details when a request fails"""
    error_code: int
    message: str
    details: Optional[List[str]] = None

# Define a union type for responses that could be success or error
ResponseType = Union[Person, ErrorResponse]

def basic_example():
    """Demonstrate basic Jinja2 template rendering with GASP"""
    print("\n=== BASIC JINJA2 TEMPLATE EXAMPLE ===")
    
    # Create a template string with Jinja2 syntax
    template = """
    # {{ title }}
    
    Please generate {{ article }} {{ type_name|type_description }}.
    
    {% if include_format %}
    Your response must be formatted as:
    {{ response_type|format_type("Response") }}
    {% endif %}
    """
    
    # Provide the template context with variables
    context = {
        'title': 'Person Generator',
        'article': 'a',
        'type_name': Person,
        'response_type': Person,
        'include_format': True
    }
    
    # Render the template
    prompt = render_template(template, context)
    
    print("RENDERED PROMPT:")
    print("-" * 40)
    print(prompt)
    
    # Let's simulate an LLM response
    llm_response = """
    <Response>
    {
      "name": "Alex Johnson",
      "age": 28,
      "address": {
        "street": "456 Maple Avenue",
        "city": "Portland",
        "zip_code": "97201"
      },
      "hobbies": ["photography", "cooking", "hiking"]
    }
    </Response>
    """
    
    # Parse the response
    parser = Parser(Person)
    parser.feed(llm_response)
    result = parser.validate()
    
    print("\nPARSED RESULT:")
    print("-" * 40)
    if result:
        print(f"Name: {result.name}")
        print(f"Age: {result.age}")
        print(f"City: {result.address.city}")
        print(f"Hobbies: {', '.join(result.hobbies or [])}")

def conditional_template_example():
    """Demonstrate conditional template logic based on parameters"""
    print("\n=== CONDITIONAL TEMPLATE EXAMPLE ===")
    
    # Template with conditional sections based on parameters
    template = """
    # {{ operation }} Request
    
    {% if operation == 'create' %}
    Create a new person in the database with the following details:
    - Name: {{ params.name }}
    - Age: {{ params.age }}
    - Location: {{ params.location }}
    
    {% elif operation == 'lookup' %}
    Look up a person with ID {{ params.id }} in the database.
    {% endif %}
    
    {% if detailed %}
    Include all available details in your response.
    {% else %}
    Only include basic information in your response.
    {% endif %}
    
    {{ response_type|format_type("Response") }}
    """
    
    # Create a context for looking up a person
    lookup_context = {
        'operation': 'lookup',
        'params': {'id': '12345'},
        'detailed': True,
        'response_type': ResponseType  # Union type that could be Person or ErrorResponse
    }
    
    # Render the lookup template
    lookup_prompt = render_template(template, lookup_context)
    
    print("LOOKUP PROMPT:")
    print("-" * 40)
    print(lookup_prompt)
    
    # Create a context for creating a person
    create_context = {
        'operation': 'create',
        'params': {
            'name': 'Sarah Miller', 
            'age': 34, 
            'location': 'Chicago'
        },
        'detailed': False,
        'response_type': Person
    }
    
    # Render the create template
    create_prompt = render_template(template, create_context)
    
    print("\nCREATE PROMPT:")
    print("-" * 40)
    print(create_prompt)

def template_inheritance_example():
    """
    Demonstrate template inheritance and how to create template files.
    
    Note: This function creates a template file in the current directory.
    In a real application, you'd typically store these in a templates/ folder.
    """
    print("\n=== TEMPLATE INHERITANCE EXAMPLE ===")
    
    # Create a base template file
    with open("base_prompt.j2", "w") as f:
        f.write("""
        # {{ title }}
        
        {% block instructions %}
        Basic instructions go here.
        {% endblock %}
        
        {% block format_requirements %}
        Your response must be formatted as:
        {{ response_type|format_type }}
        {% endblock %}
        """)
    
    # Create a template that extends the base
    with open("person_prompt.j2", "w") as f:
        f.write("""
        {% extends "base_prompt.j2" %}
        
        {% block instructions %}
        Please create a profile for a person with the following characteristics:
        - Profession: {{ profession }}
        - Age range: {{ age_range }}
        - Location: {{ location }}
        
        Make sure to include their name, exact age, full address, and at least 
        {{ hobby_count }} hobbies they might enjoy.
        {% endblock %}
        """)
    
    # Set up the context
    context = {
        'title': 'Detailed Person Generator',
        'profession': 'Software Engineer',
        'age_range': '25-35',
        'location': 'San Francisco',
        'hobby_count': 3,
        'response_type': Person
    }
    
    # Render the template from file
    prompt = render_file_template("person_prompt.j2", context)
    
    print("RENDERED FILE TEMPLATE:")
    print("-" * 40)
    print(prompt)
    
    # Clean up template files
    import os
    os.remove("base_prompt.j2")
    os.remove("person_prompt.j2")

def main():
    basic_example()
    conditional_template_example()
    template_inheritance_example()

if __name__ == "__main__":
    main()
