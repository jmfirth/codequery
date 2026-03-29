#include "models.hpp"
#include <iostream>

int main() {
    mylib::Dog dog("Rex", 5);
    dog.speak();
    std::cout << dog.get_name() << std::endl;
    return 0;
}

int add(int a, int b) {
    return a + b;
}

void free_function() {
    int result = add(2, 3);
    // A free function outside any namespace or class
}
