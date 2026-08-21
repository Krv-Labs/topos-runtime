#include <stdint.h>
#include <stdlib.h>

int main(int argc, char **argv) {
    long n = (argc > 1) ? strtol(argv[1], NULL, 10) : 1000000L;
    if (n < 1) return 1;

    uint32_t *a = malloc((size_t)n * sizeof(uint32_t));
    uint32_t *b = malloc((size_t)n * sizeof(uint32_t));
    if (!a || !b) {
        free(a);
        free(b);
        return 1;
    }

    uint32_t state = 0xACE1u;
    for (long i = 0; i < n; i++) {
        state = state * 1664525u + 1013904223u;
        a[i] = state;
    }

    for (int shift = 0; shift < 32; shift += 8) {
        size_t count[256] = {0};
        for (long i = 0; i < n; i++) {
            count[(a[i] >> shift) & 0xFF]++;
        }
        size_t bucket[256];
        bucket[0] = 0;
        for (int i = 1; i < 256; i++) {
            bucket[i] = bucket[i - 1] + count[i - 1];
        }
        for (long i = 0; i < n; i++) {
            uint8_t byte = (a[i] >> shift) & 0xFF;
            b[bucket[byte]++] = a[i];
        }
        uint32_t *tmp = a;
        a = b;
        b = tmp;
    }

    uint32_t check = a[0] + a[n - 1];
    free(a);
    free(b);
    return (check > 0) ? 0 : 2;
}
