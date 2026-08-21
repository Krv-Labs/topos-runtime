#include <stdint.h>
#include <stdlib.h>
#include <string.h>

int main(int argc, char **argv) {
    long nbytes = (argc > 1) ? strtol(argv[1], NULL, 10) : (1L << 20);
    if (nbytes < 1) {
        return 1;
    }

    uint8_t *buf = malloc((size_t)nbytes);
    if (!buf) {
        return 1;
    }

    memset(buf, 1, (size_t)nbytes);

    uint64_t sum = 0;
    for (long i = 0; i < nbytes; i++) {
        sum += buf[i];
    }

    free(buf);
    return (sum > 0) ? 0 : 2;
}
