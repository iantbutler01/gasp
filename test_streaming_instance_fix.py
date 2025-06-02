#!/usr/bin/env python3

import gasp
from gasp import Deserializable

# Define a simple Chat class similar to the user's example
class Chat(Deserializable):
    def __init__(self, _type_name: str = "Chat", content: str = ""):
        self._type_name = _type_name
        self.content = content
    
    def __repr__(self):
        return f"Chat(content='{self.content}')"

def test_streaming_chat_instances():
    """Test that we get Chat instances from the very beginning of streaming, not dicts."""
    
    # Create a parser for Chat type
    parser = gasp.Parser(Chat)
    
    # Simulate streaming chunks that gradually build up a Chat object
    chunks = [
        '<Chat>{"_type_name": "Chat"',  # Initial chunk with type info
        ', "content": "Hey',          # Add content field (exclamation inside quotes)
        '!"}</Chat>'                      # Complete the object
    ]
    
    results = []
    
    print("Testing streaming Chat instances...")
    
    for i, chunk in enumerate(chunks):
        print(f"Feeding chunk {i}: {chunk}")
        result = parser.feed(chunk)
        print(f"  Result type: {type(result)}")
        print(f"  Result: {result}")
        
        # Verify we get Chat instances, not dicts
        if result is not None:
            assert isinstance(result, Chat), f"Expected Chat instance, got {type(result)} at chunk {i}"
            results.append(result)
        
        print()
    
    # Verify the final result has the complete content
    assert len(results) > 0, "Should have received at least one result"
    final_result = results[-1]
    assert final_result.content == "Hey!", f"Expected 'Hey!', got '{final_result.content}'"
    
    print("✅ All chunks produced Chat instances (no dicts)!")
    print(f"✅ Final result: {final_result}")

def test_union_streaming():
    """Test streaming with Union types also produces proper instances."""
    from typing import Union
    
    class Action(Deserializable):
        def __init__(self, _type_name: str = "Action", action: str = ""):
            self._type_name = _type_name
            self.action = action
    
    class Response(Deserializable):
        def __init__(self, _type_name: str = "Response", message: str = ""):
            self._type_name = _type_name
            self.message = message
    
    # Create a Union type parser
    ChatOrAction = Union[Chat, Action, Response]
    parser = gasp.Parser(ChatOrAction)
    
    # Stream a Chat object via Union parser
    chunks = [
        '<ChatOrAction>{"_type_name": "Chat"',
        ', "content": "Hello from union"',
        '}</ChatOrAction>'
    ]
    
    print("Testing Union type streaming...")
    
    for i, chunk in enumerate(chunks):
        print(f"Feeding chunk {i}: {chunk}")
        result = parser.feed(chunk)
        print(f"  Result type: {type(result)}")
        print(f"  Result: {result}")
        
        if result is not None:
            # Should get proper typed instances, not dicts
            assert not isinstance(result, dict), f"Got dict instead of typed instance at chunk {i}"
            if hasattr(result, 'content'):
                assert isinstance(result, Chat), f"Expected Chat instance, got {type(result)}"
    
    print("✅ Union streaming also produces proper instances!")

if __name__ == "__main__":
    test_streaming_chat_instances()
    print()
    test_union_streaming()
    print("\n🎉 All tests passed! The streaming parser now consistently returns typed instances.")
