#!/usr/bin/env python3
"""
Sample Python code for testing syntax-aware pattern matching.
Contains function definitions, strings, and comments with similar content.
"""

# TODO: Refactor old_function to use new API
# old_function is deprecated since v2.0

def old_function(x, y):
    """
    This is old_function - the docstring mentions old_function too.
    """
    # Call old_function recursively
    message = "Calling old_function with parameters"
    print(f"old_function received: {x}, {y}")

    if x > 0:
        return old_function(x - 1, y)
    return y


def new_function(data):
    """Process data with new_function implementation."""
    result = data * 2
    # new_function is the replacement for old_function
    log_message = "new_function processed successfully"
    return result


class UserService:
    """Service class for user operations."""

    def get_user(self, user_id):
        # FIXME: Add caching for get_user
        print(f"Fetching user {user_id}")
        return {"id": user_id, "name": "Test User"}

    def deprecated_api(self, data):
        """
        deprecated_api should not be used anymore.
        Use new_api instead.
        """
        # This calls deprecated_api internally
        message = "deprecated_api is still in use"
        return self._process(data)

    def new_api(self, data):
        return self._process(data)

    def _process(self, data):
        return data


# Test functions
def test_old_function():
    """Test for old_function - should use old_function correctly."""
    result = old_function(3, 10)
    assert result == 10, "old_function test failed"


def test_new_function():
    assert new_function(5) == 10


if __name__ == "__main__":
    # Main entry point calls old_function
    print("Testing old_function and new_function")
    old_function(2, 5)
    new_function(10)
