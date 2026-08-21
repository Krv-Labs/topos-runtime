#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "== compiled workload suite =="
TOPOS_BENCHMARK=1 cargo test -p topos-engine benchmark_workloads -- --nocapture

echo ""
echo "== baseline compare =="
BASELINE="$ROOT/benchmarks/baselines/ubuntu-latest.json"
if [[ -f "$BASELINE" ]]; then
  cargo run -p topos -- benchmark --compare-baseline "$BASELINE" --json
else
  echo "skip: no baseline at $BASELINE"
fi

echo ""
echo "== curvature microbench =="
cargo bench -p topos-engine --bench curvature -- --noplot
