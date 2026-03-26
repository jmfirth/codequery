"""Main module with functions and constants."""

MAX_RETRIES = 3
DEFAULT_TIMEOUT = 30
_INTERNAL_STATE = "active"

def greet(name: str) -> str:
    """Return a greeting string."""
    return f"Hello, {name}!"

def add(a: int, b: int) -> int:
    """Add two numbers."""
    return a + b

def _private_helper():
    """A private helper function."""
    pass
