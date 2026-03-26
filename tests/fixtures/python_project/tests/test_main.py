"""Tests for main module."""

def test_greet():
    """Test the greet function."""
    assert greet("World") == "Hello, World!"

def test_add():
    """Test the add function."""
    assert add(1, 2) == 3

def test_greet_empty():
    """Test greet with empty string."""
    assert greet("") == "Hello, !"

def helper_not_a_test():
    """This is not a test function."""
    pass
