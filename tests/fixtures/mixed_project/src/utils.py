"""Utility functions for the mixed project."""

MAX_CONNECTIONS = 10

class Connection:
    """Represents a network connection."""

    def __init__(self, host: str, port: int):
        self.host = host
        self.port = port

    def connect(self) -> bool:
        """Establish the connection."""
        return True

def format_address(host: str, port: int) -> str:
    """Format a host:port address string."""
    return f"{host}:{port}"

def _internal_helper() -> None:
    """Private helper function."""
    pass
