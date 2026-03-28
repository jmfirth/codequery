"""Service module that imports from other modules."""

from models import User
from utils import format_name

def process_user(name: str, age: int) -> str:
    """Create and process a user."""
    user = User(name, age)
    if user.is_adult():
        return format_name(name, "verified")
    return format_name(name, "pending")

def list_users(users: list) -> list:
    """List all user names."""
    return [format_name(u.name, "") for u in users]
