# Topos Runtime Benchmarks & Experiments

Phase 1 baseline evaluation and Phase 2 closed-loop compiled-binary optimization experiments.

## Layout

| Path | Purpose |
| --- | --- |
| `manifest.toml` | Manifest of 9 diverse algorithmic, numerical, and structural workloads |
| `workloads/*.c` | C programs compiled via Clang (`matmul`, `branchy`, `nbody`, `sha256`, `image_filter`, etc.) |
| `baselines/ubuntu-latest.json` | CI regression baseline (verified wall-time ceilings) |
| `results/` | Machine-readable JSON results and detailed benchmark methodology reports |
| `../scripts/run_experiments.py` | Full closed-loop optimization experiment driver across all variants |
| `../topos/engine/src/benchmarks/` | Rust baseline runner (compile → disassemble → parse LLVM IR → run → perf) |
| `../topos/engine/benches/curvature.rs` | Criterion microbench (10k nodes / ~50k edges Ricci curvature) |

## Workloads

1. `matmul` — Dense matrix multiplication $O(N^3)$ (compute and memory access patterns, auto-vectorization)
2. `branchy` — Branch-heavy PRNG state machine (profile-guided optimization & branch prediction)
3. `memory_scan` — Large buffer sequential memory traversal (cache lines, page faults, RSS locality)
4. `nbody` — Gravitational particle simulation (floating-point vector arithmetic & SIMD)
5. `sha256` — Cryptographic block hashing (bitwise transformations, loop unrolling)
6. `image_filter` — 2D 5x5 convolution stencil (spatial locality, loop tiling, vector registers)
7. `tree_search` — Binary search tree insertions & skewed lookups (pointer chasing, cache latency)
8. `ode_sim` — 4th-order Runge-Kutta ODE numerical simulation (Lorenz attractor)
9. `sort_radix` — Radix sort over multi-million integer buffers (histogram passes, memory bandwidth)

## Commands

```bash
# Baseline evaluation
topos benchmark --json
topos benchmark --compare-baseline benchmarks/baselines/ubuntu-latest.json --tolerance-pct 150

# Full closed-loop compiled optimization experiment suite
python3 scripts/run_experiments.py

# Closed-loop optimization for a single target
topos compiled plan benchmarks/workloads/branchy.c -- 100000000
topos compiled approve .topos/compiled/plan.json --by engineer
topos compiled apply .topos/compiled/plan.json
topos compiled rollback

# Curvature microbench
cargo bench -p topos-engine --bench curvature

# Full CI benchmark suite
./scripts/run_benchmarks.sh
```

## Results & Methodology

See [results/2026-08-21-compiled-engine.md](results/2026-08-21-compiled-engine.md) for full methodology and per-workload analysis.
