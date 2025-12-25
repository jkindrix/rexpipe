# Python file for testing all tree-sitter scopes

# IMPORT SCOPE - import statements
import os
from pathlib import Path
from collections import defaultdict
import helper_module as hm

# TYPE SCOPE - type hints
def typed_function(name: str, count: int) -> list[str]:
    """Docstring mentioning helper"""
    return [name] * count

# FUNCTION SCOPE - function definitions
def helper_function(x):
    """A helper function"""
    return x * 2

def another_function(a, b):
    # Comment mentioning helper
    return a + b

# CLASS with methods
class User:
    def __init__(self, name: str, age: int):
        self.name = name
        self.age = age

    def greet(self):
        # This helper comment
        print(f"Hello, {self.name}")

    def helper_method(self):
        return self.age * 2

# FUNCTION_CALLS SCOPE
result = helper_function(42)
user = User("Alice", 30)
user.greet()
print("Result:", result)

# CONTROL_FLOW SCOPE
def control_flow_examples():
    x = 5

    if x > 0:
        print("positive")
    elif x < 0:
        print("negative")
    else:
        print("zero")

    for i in range(10):
        print(i)

    while x > 0:
        x -= 1
        break

    try:
        risky_operation()
    except Exception as e:
        print(e)
    finally:
        cleanup()

# IDENTIFIERS SCOPE
my_variable = 42
another_var = "hello"
CONSTANT_VALUE = 100

# STRING SCOPE
greeting = "hello in string"
multiline = """hello in
multiline string"""

# TESTS SCOPE - test functions
def test_helper():
    assert helper_function(2) == 4

def test_another():
    assert another_function(1, 2) == 3

class TestUser:
    def test_greet(self):
        user = User("Test", 25)
        assert user.name == "Test"

    def test_helper_method(self):
        user = User("Test", 10)
        assert user.helper_method() == 20
