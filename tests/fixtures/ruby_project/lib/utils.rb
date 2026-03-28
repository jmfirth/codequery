# Utility module with helper methods.
module Utils
  def self.format_name(first, last)
    "#{first} #{last}"
  end

  def self.validate(value)
    !value.nil?
  end
end
