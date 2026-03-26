#ifndef MODELS_HPP
#define MODELS_HPP

#include <string>

namespace mylib {

/* Base class for all animals. */
class Animal {
public:
    Animal(const std::string& name, int age) : name_(name), age_(age) {}
    virtual ~Animal() = default;

    virtual void speak() const = 0;

    const std::string& get_name() const { return name_; }
    int get_age() const { return age_; }

private:
    std::string name_;
    int age_;

protected:
    void log_action(const std::string& action) const {}
};

/* A dog that can bark. */
class Dog : public Animal {
public:
    Dog(const std::string& name, int age) : Animal(name, age) {}

    void speak() const override;

private:
    int tricks_count_ = 0;
};

enum class Color {
    Red,
    Green,
    Blue
};

} // namespace mylib

#endif // MODELS_HPP
