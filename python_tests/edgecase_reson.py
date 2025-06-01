from gasp import Parser, Deserializable
from typing import List, Optional

# Define the type for the items in the list
class Chat(Deserializable):
    type_name: str
    ref_id: str
    content: str

lines = [
    "I nee",
    "d to analyze the user's",
    " message",
    " to determine the",
    " appropriate response format",
    " an",
    "d reference",
    " ID to",
    " use.",
    "",
    "Key",
    " considerations",
    ":",
    "-",
    " I",
    " need to provide",
    " a ref",
    "_id in my",
    " response",
    "- I",
    " should check",
    " if the user is",
    " requesting",
    " an",
    " external action that maps to",
    " a tool",
    "- If",
    " it",
    "'s a tool request,",
    " I respon",
    "d with JSON",
    " only",
    "",
    "- Otherwise",
    ", I use the Chat",
    " tool for convers",
    "ational response",
    "",
    "- For",
    " relative",
    " updates, I shoul",
    "d select",
    " the most",
    " relevant entity's",
    " reference ID",
    "",
    "- For",
    " non-relative",
    " requests",
    ", I use the provide",
    "d reference ID",
    "",
    "",
    "However",
    ", I notice",
    " that",
    " the user_",
    "message,",
    " ref",
    "_id, and active",
    "_entities plac",
    "eholders appear",
    " to be template",
    " variables that",
    " haven",
    "'t been fille",
    "d in with",
    " actual content",
    ". Without",
    " the",
    " actual user",
    " message,",
    " I cannot",
    " determine:",
    "",
    "- Whether",
    " this",
    " is a tool",
    " request or convers",
    "ational message",
    "",
    "- Which",
    " reference",
    " ID to use",
    "-",
    " What",
    " specific action",
    " to take",
    "",
    "Since",
    " I cannot",
    " see",
    " the actual user message or",
    " reference",
    " information",
    ", I'll respon",
    "d with the",
    " Chat",
    " tool to",
    " explain",
    " this",
    " limitation",
    ":",
    "",
    "",
    "<List",
    ">[",
    "",
    "{",
    "",
    '  "type_',
    'name": "Chat',
    '",',
    '  "ref',
    '_id": "{{',
    ' ref_id }}",',
    "",
    '  "content',
    '": "I',
    " notice",
    " that",
    " the message",
    " appears",
    " to contain",
    " template plac",
    "eholders rather",
    " than actual content",
    ". Coul",
    "d you please provide your",
    " actual",
    " request",
    " so",
    " I can assist",
    " you properly",
    '?"',
    "}",
    "]",
    "</List>"
]

def test_stream_parse_list_of_chat():
    # Create a parser for List[Chat]
    # The parser should handle the <List> tag case-insensitively
    parser = Parser(List[Chat])

    # Feed chunks to the parser
    for chunk in lines:
        parser.feed(chunk)

    # Validate the result
    result = parser.validate()

    # Assertions
    assert parser.is_complete(), "Parsing should be complete"
    assert isinstance(result, list), "Result should be a list"
    assert len(result) == 1, "List should contain one item"
    
    chat_item = result[0]
    assert isinstance(chat_item, Chat), "Item should be an instance of Chat"
    assert chat_item.type_name == "Chat"
    assert chat_item.ref_id == "{{ ref_id }}"
    assert chat_item.content == "I notice that the message appears to contain template placeholders rather than actual content. Could you please provide your actual request so I can assist you properly?"
    
    print("Stream parsing test for List[Chat] successful!")
    print("Parsed result:", result)

if __name__ == "__main__":
    test_stream_parse_list_of_chat()
