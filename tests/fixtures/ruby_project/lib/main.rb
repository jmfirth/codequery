# Main entry point for the Ruby project.
require_relative 'models'

# Greet a user by name.
def greet(name)
  "Hello, #{name}!"
end

def add(x, y)
  x + y
end

def _private_helper
  42
end

result = greet("World")
