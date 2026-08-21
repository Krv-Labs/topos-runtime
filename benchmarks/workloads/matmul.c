#include <stdlib.h>
#include <string.h>

static void matmul(const double *a, const double *b, double *c, int n) {
    for (int i = 0; i < n; i++) {
        for (int j = 0; j < n; j++) {
            double sum = 0.0;
            for (int k = 0; k < n; k++) {
                sum += a[i * n + k] * b[k * n + j];
            }
            c[i * n + j] = sum;
        }
    }
}

int main(int argc, char **argv) {
    int n = (argc > 1) ? atoi(argv[1]) : 64;
    if (n < 1) {
        return 1;
    }

    size_t sz = (size_t)n * (size_t)n;
    double *a = calloc(sz, sizeof(double));
    double *b = calloc(sz, sizeof(double));
    double *c = calloc(sz, sizeof(double));
    if (!a || !b || !c) {
        free(a);
        free(b);
        free(c);
        return 1;
    }

    for (size_t i = 0; i < sz; i++) {
        a[i] = 1.0;
        b[i] = 2.0;
    }

    matmul(a, b, c, n);

    double check = c[0];
    free(a);
    free(b);
    free(c);
    return (check > 0.0) ? 0 : 2;
}
