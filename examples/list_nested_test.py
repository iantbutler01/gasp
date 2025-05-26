#!/usr/bin/env python3
"""
Test for List of nested objects in GASP
"""

from gasp import Deserializable, Parser
from typing import List

# Define nested types with a list of objects
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

def test_direct_instantiation():
    """Test that direct instantiation works with lists of objects"""
    print("=== Testing direct instantiation with list of objects ===")
    
    # Create subsystem objects directly
    subsys1 = Subsystem.__gasp_from_partial__({"name": "Auth", "category": "Security", "priority": 1})
    subsys2 = Subsystem.__gasp_from_partial__({"name": "DB", "category": "Storage", "priority": 2})
    
    # Create report with list of subsystems
    report = ReportSubsystems.__gasp_from_partial__({"subsystems": [subsys1, subsys2]})
    
    print(f"First subsystem type: {type(report.subsystems[0])}")
    print(f"Report: {report}")
    
    # We expect report.subsystems to be a list of Subsystem objects, not dicts
    assert isinstance(report.subsystems[0], Subsystem), f"Expected Subsystem but got {type(report.subsystems[0])}"
    print("Direct instantiation test passed!")

def test_parser():
    """Test that the Parser correctly handles lists of nested types"""
    print("\n=== Testing parser with list of nested types ===")
    
    # Create a parser for the ReportSubsystems type
    parser = Parser(ReportSubsystems)
    
    # Test with JSON data that includes a list of Subsystem objects
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
    
    # Parse the data
    result = parser.feed(json_data)
    print(f"Parsed result: {result}")
    print(f"Result type: {type(result)}")
    
    # Check if parsing is complete
    print(f"Is complete: {parser.is_complete()}")
    
    # Check subsystems list
    if result and hasattr(result, 'subsystems') and result.subsystems:
        print(f"First subsystem type: {type(result.subsystems[0])}")
        print(f"First subsystem: {result.subsystems[0]}")
        
        # We expect each item in result.subsystems to be a Subsystem object, not a dict
        assert isinstance(result.subsystems[0], Subsystem), f"Expected Subsystem but got {type(result.subsystems[0])}"
        
        # Access properties of the subsystem to confirm it works
        print(f"Subsystem name: {result.subsystems[0].name}")
        print(f"Subsystem category: {result.subsystems[0].category}")
        print("Parser list of nested types test passed!")
    else:
        print("ERROR: Subsystems attribute missing or not properly instantiated")

if __name__ == "__main__":
    test_direct_instantiation()
    test_parser()
