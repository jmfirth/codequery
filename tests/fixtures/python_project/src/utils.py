"""Utility functions and private helpers."""

def format_name(first: str, last: str) -> str:
    """Format a full name."""
    return f"{first} {last}"

def validate_age(age: int) -> bool:
    """Validate that age is positive."""
    return age > 0

def _sanitize_input(text: str) -> str:
    """Private: sanitize user input."""
    return text.strip().lower()

def __double_private():
    """Double underscore private function."""
    pass
