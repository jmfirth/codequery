#include "models.hpp"
#include <iostream>

int main() {
    mylib::Dog dog("Rex", 5);
    dog.speak();
    std::cout << dog.get_name() << std::endl;
    return 0;
}

void free_function() {
    // A free function outside any namespace or class
}
