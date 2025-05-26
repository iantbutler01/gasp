#!/usr/bin/env python3
"""
GASP Nested Type Conversion Test

This script tests that nested types in collections are properly converted
to their correct Python types, rather than remaining as dictionaries.
"""

from gasp import Deserializable, Parser
from typing import List, Optional

# Define a nested type structure similar to what the user described
class Subsystem(Deserializable):
    """A subsystem with name and category"""
    name: str
    category: str
    priority: int = 0  # Default value
    
    def __repr__(self):
        return f"Subsystem(name='{self.name}', category='{self.category}', priority={self.priority})"

class ReportSubsystems(Deserializable):
    """Report of subsystems found in the codebase"""
    subsystems: List[Subsystem]
    
    def __init__(self, subsystems: List[Subsystem] | None = None):
        self.subsystems = subsystems or []
    
    def __repr__(self):
        return f"ReportSubsystems(subsystems={self.subsystems})"

def main():
    print("=== Testing nested type conversion ===")
    
    # Create a parser for the ReportSubsystems type
    parser = Parser(ReportSubsystems)
    
    # Test with a simple JSON string that includes nested Subsystem objects
    json_data = '''<ReportSubsystems>
    {
        "subsystems": [
            {
                "name": "Authentication",
                "category": "Security",
                "priority": 1
            },
            {
                "name": "Database",
                "category": "Storage",
                "priority": 2
            }
        ]
    }
    </ReportSubsystems>'''
    
    # Feed the data to the parser
    result = parser.feed(json_data)
    print("Parsed result:", result)
    
    # Check if the parser is complete
    print("Is complete:", parser.is_complete())
    
    # Validate the result
    validated = parser.validate()
    print("Validated result:", validated)
    
    # Check the types to verify nested objects are properly instantiated
    if validated:
        print("\nObject types:")
        print(f"Top-level object type: {type(validated)}")
        
        if hasattr(validated, 'subsystems') and validated.subsystems:
            print(f"First subsystem type: {type(validated.subsystems[0])}")
            
            # Access nested object properties to show it's a proper object
            subsystem = validated.subsystems[0]
            print(f"\nAccessing nested object properties:")
            print(f"  Name: {subsystem.name}")
            print(f"  Category: {subsystem.category}")
            print(f"  Priority: {subsystem.priority}")

if __name__ == "__main__":
    main()
