#include <stdint.h>
#include <stdlib.h>

static uint32_t lcg(uint32_t *state) {
    *state = (*state * 1664525u) + 1013904223u;
    return *state;
}

int main(int argc, char **argv) {
    long steps = (argc > 1) ? strtol(argv[1], NULL, 10) : 1000000L;
    if (steps < 1) {
        return 1;
    }

    uint32_t state = 0xC0FFEEu;
    long acc = 0;

    for (long i = 0; i < steps; i++) {
        uint32_t r = lcg(&state);
        if ((r & 3u) == 0u) {
            acc += (long)(r & 0xFFu);
        } else if ((r & 3u) == 1u) {
            acc -= (long)(r & 0x7Fu);
        } else if ((r & 3u) == 2u) {
            acc ^= (long)(r >> 8);
        } else {
            acc += (long)(r & 1u);
        }
    }

    return (acc & 1L) == 0L ? 0 : 0;
}
