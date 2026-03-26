#include "utils.h"

/* Add two integers and return the result. */
int add(int a, int b) {
    return a + b;
}

/* Multiply two integers and return the result. */
int multiply(int a, int b) {
    return a * b;
}

static int internal_helper(int x) {
    return x * 2;
}
