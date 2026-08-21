#!/usr/bin/env python3
"""Topos closed-loop optimizer experiment suite.

Runs all benchmark workloads through the full compile-plan-approve-apply-measure cycle
across clang flag variants (-O2 baseline, -O3, -Os, -flto, PGO-O3), collecting exact
speedups, sign-flip permutation p-values, binary size changes, locality metrics,
and gate satisfaction verdicts.
"""

import glob
import json
import os
import subprocess
import sys
import time

WORKLOADS = [
    ("matmul", "benchmarks/workloads/matmul.c", ["512"]),
    ("branchy", "benchmarks/workloads/branchy.c", ["100000000"]),
    ("memory_scan", "benchmarks/workloads/memory_scan.c", ["400000000"]),
    ("nbody", "benchmarks/workloads/nbody.c", ["500", "200"]),
    ("sha256", "benchmarks/workloads/sha256.c", ["500000"]),
    ("image_filter", "benchmarks/workloads/image_filter.c", ["1024", "1024", "100"]),
    ("tree_search", "benchmarks/workloads/tree_search.c", ["50000", "1000000"]),
    ("ode_sim", "benchmarks/workloads/ode_sim.c", ["15000000"]),
    ("sort_radix", "benchmarks/workloads/sort_radix.c", ["15000000"]),
]

def main():
    os.makedirs("benchmarks/results", exist_ok=True)
    results = []

    print("=" * 80)
    print("TOPOS RUNTIME: CLOSED-LOOP COMPILED OPTIMIZATION EXPERIMENT SUITE")
    print("=" * 80)

    for wid, src, args in WORKLOADS:
        print(f"\n▶ Running workload: {wid} ({src})")
        plan_path = f".topos/compiled/plan_{wid}.json"
        
        # 1. Plan
        cmd_plan = [
            "cargo", "run", "-q", "-p", "topos", "--",
            "compiled", "plan", src,
            "--runs", "10",
            "--min-speedup", "1.0",
            "--max-size-increase", "50.0",
            "--out", plan_path,
            "--json",
            "--",
        ] + args
        res_plan = subprocess.run(cmd_plan, capture_output=True, text=True)
        if res_plan.returncode != 0:
            print(f"  ❌ Plan failed: {res_plan.stderr}")
            continue

        # 2. Approve
        cmd_appr = [
            "cargo", "run", "-q", "-p", "topos", "--",
            "compiled", "approve", plan_path,
            "--by", "topos-experiment-runner",
            "--json",
        ]
        res_appr = subprocess.run(cmd_appr, capture_output=True, text=True)
        if res_appr.returncode != 0:
            print(f"  ❌ Approve failed: {res_appr.stderr}")
            continue

        # 3. Apply (Measures interleaved runs, exact permutation test, gates)
        cmd_apply = [
            "cargo", "run", "-q", "-p", "topos", "--",
            "compiled", "apply", plan_path,
            "--json",
        ]
        res_apply = subprocess.run(cmd_apply, capture_output=True, text=True)
        
        report = None
        promoted = None
        if res_apply.stdout.strip():
            try:
                parsed = json.loads(res_apply.stdout)
                report = parsed.get("report")
                promoted = parsed.get("promoted")
            except json.JSONDecodeError:
                pass
        
        if report is None:
            # Check latest report in store
            report_files = sorted(glob.glob(".topos/compiled/runs/*/report.json"))
            if report_files:
                with open(report_files[-1]) as rf:
                    report = json.load(rf)

        if report:
            results.append({
                "workload_id": wid,
                "source": src,
                "args": args,
                "promoted": promoted,
                "report": report,
            })
            base_variant = report.get("baseline_variant", "O2")
            base_size = report.get("baseline_size_bytes", 0)
            candidates = report.get("candidates", [])
            print(f"  Baseline: {base_variant} ({base_size} B)")
            for c in candidates:
                speed_str = f"{c.get('speedup_pct', 0.0):+.2f}%" if c.get('speedup_pct') is not None else "N/A"
                p_str = f"p={c.get('p_value'):.4f}" if c.get('p_value') is not None else "p=—"
                size_str = f"{c.get('size_increase_pct', 0.0):+.1f}%"
                verdict = c.get('noise', 'unknown')
                speed_sat = "S" if c.get('speed_satisfied') else "·"
                size_sat = "Z" if c.get('size_satisfied') else "·"
                print(f"    - {c.get('variant'):<8}: Speedup {speed_str:<8} ({p_str:<8}) | Size {size_str:<7} | [{speed_sat}{size_sat}] {verdict}")
            if promoted:
                print(f"  🏆 PROMOTED: {promoted}")

    out_file = "benchmarks/results/experiment_summary.json"
    with open(out_file, "w") as f:
        json.dump(results, f, indent=2)

    print("\n" + "=" * 80)
    print(f"Summary written to {out_file}")
    print("=" * 80)

if __name__ == "__main__":
    main()
