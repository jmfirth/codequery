"""Models module with classes, methods, and decorators."""

class User:
    """A user in the system."""

    def __init__(self, name: str, age: int):
        self.name = name
        self.age = age

    def is_adult(self) -> bool:
        """Check if the user is an adult."""
        return self.age >= 18

    def _internal_check(self):
        """Private internal check."""
        pass

    @staticmethod
    def create(name: str) -> "User":
        """Create a user with default age."""
        return User(name, 0)

    @classmethod
    def from_dict(cls, data: dict) -> "User":
        """Create a user from a dictionary."""
        return cls(data["name"], data["age"])

class Admin(User):
    """An admin user with extra privileges."""

    def __init__(self, name: str, age: int, level: int):
        super().__init__(name, age)
        self.level = level

    def promote(self) -> None:
        """Promote the admin to next level."""
        self.level += 1
