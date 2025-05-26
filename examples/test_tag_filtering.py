#!/usr/bin/env python3
"""
Example demonstrating tag filtering with gasp.

This shows how to use ignored tags to filter out unwanted content.
"""

import gasp
from typing import List, Dict

# Example 1: Default ignored tags
print("=== Example 1: Default Ignored Tags ===")

# Simulate LLM output with thinking tags (ignored by default)
llm_output = """
<think>
Let me think about this question...
The capital of France is Paris.
</think>

<Result>
{
    "answer": "The capital of France is Paris",
    "confidence": 0.95
}
</Result>

<thinking>
I'm very confident about this answer.
</thinking>
"""

# Create a simple class for the result
class Result:
    answer: str
    confidence: float

# Parse only the Result tag (think/thinking/system tags are ignored by default)
parser = gasp.Parser(Result)
result = parser.feed(llm_output)
print(f"Parsed result: {result}")
print(f"Answer: {result.answer if result else 'None'}")
print(f"Confidence: {result.confidence if result else 'None'}")
print()

# Example 2: Custom ignored tags
print("=== Example 2: Custom Ignored Tags ===")

llm_output2 = """
<Response>
{
    "data": "Important data",
    "status": "success"
}
</Response>

<DebugInfo>
{
    "processing_time": 0.5,
    "memory_used": "100MB"
}
</DebugInfo>
"""

class Response:
    data: str
    status: str

# Parse with custom ignored tags (overrides defaults)
parser2 = gasp.Parser(Response, ignored_tags=["DebugInfo"])
result2 = parser2.feed(llm_output2)
print(f"Parsed response: {result2}")
print(f"Data: {result2.data if result2 else 'None'}")
print(f"Status: {result2.status if result2 else 'None'}")
print()

# Example 3: No ignored tags
print("=== Example 3: No Ignored Tags ===")

llm_output3 = """
<Report>
{
    "title": "Monthly Report",
    "sections": ["Introduction", "Analysis", "Conclusion"]
}
</Report>
"""

class Report:
    title: str
    sections: List[str]

# Parse with no ignored tags (empty list overrides defaults)
parser3 = gasp.Parser(Report, ignored_tags=[])
result3 = parser3.feed(llm_output3)
print(f"Parsed report: {result3}")
print(f"Title: {result3.title if result3 else 'None'}")
print(f"Sections: {result3.sections if result3 else 'None'}")
print()

# Example 4: Streaming with ignored tags
print("=== Example 4: Streaming with Ignored Tags ===")

# Simulate streaming chunks
chunks = [
    "<system>Processing request...</system>",
    "<Answer>",
    '{"value": ',
    '"42",',
    ' "unit": "degrees"}',
    "</Answer>",
    "<think>That was easy!</think>"
]

class Answer:
    value: str
    unit: str

# Create parser (system and think are ignored by default)
parser4 = gasp.Parser(Answer)

# Feed chunks one by one
for i, chunk in enumerate(chunks):
    result = parser4.feed(chunk)
    print(f"Chunk {i}: '{chunk}' -> Result: {result}")

print(f"\nFinal answer: {result}")
print(f"Value: {result.value if result else 'None'}")
print(f"Unit: {result.unit if result else 'None'}")
