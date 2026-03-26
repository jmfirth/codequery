#include "models.hpp"
#include <iostream>

namespace mylib {

void Dog::speak() const {
    std::cout << "Woof! My name is " << get_name() << std::endl;
}

} // namespace mylib
