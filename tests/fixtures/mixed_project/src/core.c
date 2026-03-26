#include <stdio.h>

/* Core data structure. */
struct CoreState {
    int initialized;
    int error_count;
};

/* Status codes. */
enum Status {
    STATUS_OK,
    STATUS_ERROR,
    STATUS_PENDING
};

/* Initialize the core state. */
void core_init(struct CoreState* state) {
    state->initialized = 1;
    state->error_count = 0;
}

/* Get the current status as a string. */
const char* status_string(enum Status s) {
    switch (s) {
        case STATUS_OK: return "ok";
        case STATUS_ERROR: return "error";
        case STATUS_PENDING: return "pending";
        default: return "unknown";
    }
}
