#include "utils.h"
#include <stdio.h>

/* Entry point for the program. */
int main(int argc, char* argv[]) {
    int result = add(2, 3);
    printf("Result: %d\n", result);
    return 0;
}

struct Config {
    int verbose;
    int max_retries;
};

enum LogLevel {
    LOG_DEBUG,
    LOG_INFO,
    LOG_WARN,
    LOG_ERROR
};

typedef unsigned long size_t_alias;

int global_counter = 0;

#define MAX_BUFFER_SIZE 1024
#define SQUARE(x) ((x) * (x))
