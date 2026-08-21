#include <math.h>
#include <stdlib.h>

typedef struct {
    double x, y, z;
    double vx, vy, vz;
    double mass;
} Body;

static void advance(Body *bodies, int n, double dt, int steps) {
    for (int s = 0; s < steps; s++) {
        for (int i = 0; i < n; i++) {
            for (int j = i + 1; j < n; j++) {
                double dx = bodies[j].x - bodies[i].x;
                double dy = bodies[j].y - bodies[i].y;
                double dz = bodies[j].z - bodies[i].z;
                double dist_sq = dx * dx + dy * dy + dz * dz + 1e-9;
                double inv_dist = 1.0 / sqrt(dist_sq);
                double mag = dt * inv_dist * inv_dist * inv_dist;
                bodies[i].vx += dx * bodies[j].mass * mag;
                bodies[i].vy += dy * bodies[j].mass * mag;
                bodies[i].vz += dz * bodies[j].mass * mag;
                bodies[j].vx -= dx * bodies[i].mass * mag;
                bodies[j].vy -= dy * bodies[i].mass * mag;
                bodies[j].vz -= dz * bodies[i].mass * mag;
            }
        }
        for (int i = 0; i < n; i++) {
            bodies[i].x += dt * bodies[i].vx;
            bodies[i].y += dt * bodies[i].vy;
            bodies[i].z += dt * bodies[i].vz;
        }
    }
}

int main(int argc, char **argv) {
    int n = (argc > 1) ? atoi(argv[1]) : 200;
    int steps = (argc > 2) ? atoi(argv[2]) : 100;
    if (n < 2 || steps < 1) return 1;

    Body *bodies = calloc((size_t)n, sizeof(Body));
    if (!bodies) return 1;

    for (int i = 0; i < n; i++) {
        bodies[i].x = (double)(i % 100) - 50.0;
        bodies[i].y = (double)((i * 3) % 100) - 50.0;
        bodies[i].z = (double)((i * 7) % 100) - 50.0;
        bodies[i].mass = 1.0 + (double)(i % 10);
    }

    advance(bodies, n, 0.01, steps);

    double check = bodies[0].x + bodies[n - 1].z;
    free(bodies);
    return (check != 0.0) ? 0 : 2;
}
