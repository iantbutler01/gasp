#!/usr/bin/env python3
"""
Test for scalar type parsing in GASP
Tests all basic scalar types: str, int, float, bool
"""

from gasp import Parser

def test_string_parsing():
    """Test string parsing"""
    print("=== Testing string parsing ===")
    
    # Test with tags
    parser = Parser(str)
    json_data = '''<str>"Hello, world!"</str>'''
    
    print(f"Input: {json_data}")
    result = parser.feed(json_data)
    print(f"Parsed result: {result}")
    print(f"Result type: {type(result)}")
    print(f"Is complete: {parser.is_complete()}")
    
    if result is not None:
        print(f"✓ String parsing with tags: '{result}'")
        assert isinstance(result, str), f"Expected str but got {type(result)}"
    else:
        print("✗ ERROR: No result from string parsing with tags")
    
    # Test without tags
    parser2 = Parser(str)
    json_data2 = '"Simple string"'
    
    print(f"\nInput: {json_data2}")
    result2 = parser2.feed(json_data2)
    print(f"Parsed result: {result2}")
    print(f"Result type: {type(result2)}")
    print(f"Is complete: {parser2.is_complete()}")
    
    if result2 is not None:
        print(f"✓ String parsing without tags: '{result2}'")
    else:
        print("✗ ERROR: No result from string parsing without tags")

def test_int_parsing():
    """Test integer parsing"""
    print("\n=== Testing integer parsing ===")
    
    # Test with tags
    parser = Parser(int)
    json_data = '''<int>42</int>'''
    
    print(f"Input: {json_data}")
    result = parser.feed(json_data)
    print(f"Parsed result: {result}")
    print(f"Result type: {type(result)}")
    print(f"Is complete: {parser.is_complete()}")
    
    if result is not None:
        print(f"✓ Integer parsing with tags: {result}")
        assert isinstance(result, int), f"Expected int but got {type(result)}"
        assert result == 42, f"Expected 42 but got {result}"
    else:
        print("✗ ERROR: No result from integer parsing with tags")
    
    # Test without tags
    parser2 = Parser(int)
    json_data2 = '123'
    
    print(f"\nInput: {json_data2}")
    result2 = parser2.feed(json_data2)
    print(f"Parsed result: {result2}")
    print(f"Result type: {type(result2)}")
    print(f"Is complete: {parser2.is_complete()}")
    
    if result2 is not None:
        print(f"✓ Integer parsing without tags: {result2}")
    else:
        print("✗ ERROR: No result from integer parsing without tags")

def test_float_parsing():
    """Test float parsing"""
    print("\n=== Testing float parsing ===")
    
    # Test with tags
    parser = Parser(float)
    json_data = '''<float>3.14159</float>'''
    
    print(f"Input: {json_data}")
    result = parser.feed(json_data)
    print(f"Parsed result: {result}")
    print(f"Result type: {type(result)}")
    print(f"Is complete: {parser.is_complete()}")
    
    if result is not None:
        print(f"✓ Float parsing with tags: {result}")
        assert isinstance(result, float), f"Expected float but got {type(result)}"
        assert abs(result - 3.14159) < 0.0001, f"Expected 3.14159 but got {result}"
    else:
        print("✗ ERROR: No result from float parsing with tags")
    
    # Test without tags
    parser2 = Parser(float)
    json_data2 = '2.71828'
    
    print(f"\nInput: {json_data2}")
    result2 = parser2.feed(json_data2)
    print(f"Parsed result: {result2}")
    print(f"Result type: {type(result2)}")
    print(f"Is complete: {parser2.is_complete()}")
    
    if result2 is not None:
        print(f"✓ Float parsing without tags: {result2}")
    else:
        print("✗ ERROR: No result from float parsing without tags")

def test_bool_parsing():
    """Test boolean parsing"""
    print("\n=== Testing boolean parsing ===")
    
    # Test True with tags
    parser = Parser(bool)
    json_data = '''<bool>true</bool>'''
    
    print(f"Input: {json_data}")
    result = parser.feed(json_data)
    print(f"Parsed result: {result}")
    print(f"Result type: {type(result)}")
    print(f"Is complete: {parser.is_complete()}")
    
    if result is not None:
        print(f"✓ Boolean parsing with tags (true): {result}")
        assert isinstance(result, bool), f"Expected bool but got {type(result)}"
        assert result is True, f"Expected True but got {result}"
    else:
        print("✗ ERROR: No result from boolean parsing with tags (true)")
    
    # Test False with tags
    parser2 = Parser(bool)
    json_data2 = '''<bool>false</bool>'''
    
    print(f"\nInput: {json_data2}")
    result2 = parser2.feed(json_data2)
    print(f"Parsed result: {result2}")
    print(f"Result type: {type(result2)}")
    print(f"Is complete: {parser2.is_complete()}")
    
    if result2 is not None:
        print(f"✓ Boolean parsing with tags (false): {result2}")
        assert result2 is False, f"Expected False but got {result2}"
    else:
        print("✗ ERROR: No result from boolean parsing with tags (false)")
    
    # Test without tags
    parser3 = Parser(bool)
    json_data3 = 'true'
    
    print(f"\nInput: {json_data3}")
    result3 = parser3.feed(json_data3)
    print(f"Parsed result: {result3}")
    print(f"Result type: {type(result3)}")
    print(f"Is complete: {parser3.is_complete()}")
    
    if result3 is not None:
        print(f"✓ Boolean parsing without tags: {result3}")
    else:
        print("✗ ERROR: No result from boolean parsing without tags")

def test_none_parsing():
    """Test null/None parsing"""
    print("\n=== Testing None/null parsing ===")
    
    # Test with no type specified (should handle null)
    parser = Parser()
    json_data = '''null'''
    
    print(f"Input: {json_data}")
    result = parser.feed(json_data)
    print(f"Parsed result: {result}")
    print(f"Result type: {type(result)}")
    print(f"Is complete: {parser.is_complete()}")
    
    if result is None:
        print("✓ None/null parsing: None")
    else:
        print(f"✗ Unexpected result from null parsing: {result}")

def main():
    """Run all scalar parsing tests"""
    print("Running GASP scalar type parsing tests...")
    print("=" * 50)
    
    try:
        test_string_parsing()
        test_int_parsing()
        test_float_parsing()
        test_bool_parsing()
        test_none_parsing()
        
        print("\n" + "=" * 50)
        print("All tests completed!")
        
    except Exception as e:
        print(f"\n✗ Test failed with error: {e}")
        import traceback
        traceback.print_exc()

if __name__ == "__main__":
    main()
