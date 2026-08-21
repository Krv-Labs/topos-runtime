#include <stdlib.h>
#include <stdint.h>
#include <string.h>

static void conv5x5(const uint8_t *in, uint8_t *out, int w, int h) {
    static const int16_t kernel[5][5] = {
        {1, 4, 7, 4, 1},
        {4, 16, 26, 16, 4},
        {7, 26, 41, 26, 7},
        {4, 16, 26, 16, 4},
        {1, 4, 7, 4, 1}
    };
    for (int y = 2; y < h - 2; y++) {
        for (int x = 2; x < w - 2; x++) {
            int32_t sum = 0;
            for (int ky = -2; ky <= 2; ky++) {
                for (int kx = -2; kx <= 2; kx++) {
                    sum += in[(y + ky) * w + (x + kx)] * kernel[ky + 2][kx + 2];
                }
            }
            out[y * w + x] = (uint8_t)(sum >> 8);
        }
    }
}

int main(int argc, char **argv) {
    int w = (argc > 1) ? atoi(argv[1]) : 512;
    int h = (argc > 2) ? atoi(argv[2]) : 512;
    int iters = (argc > 3) ? atoi(argv[3]) : 10;
    if (w < 5 || h < 5 || iters < 1) return 1;

    size_t sz = (size_t)w * (size_t)h;
    uint8_t *in = malloc(sz);
    uint8_t *out = malloc(sz);
    if (!in || !out) {
        free(in);
        free(out);
        return 1;
    }

    for (size_t i = 0; i < sz; i++) in[i] = (uint8_t)(i % 256);
    for (int it = 0; it < iters; it++) {
        conv5x5(in, out, w, h);
        memcpy(in, out, sz);
    }

    uint8_t check = out[sz / 2];
    free(in);
    free(out);
    return (check >= 0) ? 0 : 2;
}
