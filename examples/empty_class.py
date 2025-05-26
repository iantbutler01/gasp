#!/usr/bin/env python3
from gasp import Deserializable, Parser
from gasp.template_helpers import interpolate_prompt
from typing import List, Optional, Union

class EmptyClass(Deserializable):
    pass

class NotEmptyClass(Deserializable):
    """A class with some fields"""
    field1: str
    field2: int
    field3: Optional[List[str]] = None  # Optional field with a default value

type ExampleUnion = NotEmptyClass | EmptyClass

def main():
    """Basic example: Parsing a Person object from JSON with tags"""
    print("=== Basic example ===")
    
    # Create a parser for the Person type
    # This tells GASP what class to instantiate when it sees <Person> tags
    prompt = """
    Create an empty object of type EmptyClass.

    {{return_type}}
    """
    prompt = interpolate_prompt(prompt, EmptyClass, format_tag="return_type")
    print(prompt.strip())

    union_prompt = """
    Create an object of type ExampleUnion, which can be either NotEmptyClass or EmptyClass.

    {{return_type}}
    """
    union_prompt = interpolate_prompt(union_prompt, ExampleUnion, format_tag="return_type")
    print(union_prompt.strip())

    


    

if __name__ == "__main__":
    main()
