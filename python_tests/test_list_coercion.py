#!/usr/bin/env python3
"""Test automatic coercion of a single element to a list."""

import pytest
from gasp import Parser, Deserializable
from typing import List


class Person(Deserializable):
    """Person class for testing list coercion."""

    name: str
    age: int
    email: str

    def __repr__(self):
        return f"Person(name={self.name!r}, age={self.age}, email={self.email!r})"

    def __eq__(self, other):
        if not isinstance(other, Person):
            return False
        return (
            self.name == other.name
            and self.age == other.age
            and self.email == other.email
        )


def test_single_element_coercion_to_list():
    """Test that a single emitted element is coerced to a list."""
    parser = Parser(List[Person])

    xml_data = """<Person>
        <name type="str">Alice</name>
        <age type="int">30</age>
        <email type="str">alice@example.com</email>
    </Person>"""

    parser.feed(xml_data)
    result = parser.validate()

    assert result is not None
    assert isinstance(result, list)
    assert len(result) == 1
    assert parser.is_complete()

    expected_person = Person(name="Alice", age=30, email="alice@example.com")
    assert result[0] == expected_person


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
