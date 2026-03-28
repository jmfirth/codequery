# User model with methods and constants.
class User
  MAX_AGE = 150

  def initialize(name, age)
    @name = name
    @age = age
  end

  def greet
    "Hello, #{@name}"
  end

  def _internal_check
    @age > 0
  end
end

class Admin < User
  def initialize(name, age, role)
    super(name, age)
    @role = role
  end

  def promote
    "Promoted to #{@role}"
  end
end
