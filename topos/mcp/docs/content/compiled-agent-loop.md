# Topos Compiled Agent Loop

The Compiled Agent Loop provides offline, batch, and compiled execution primitives for structural code optimization in Topos.

## Overview

Unlike interactive, step-by-step refactoring loops, the compiled loop structures quality improvement into compiled phases:
1. **Offline Evaluation** — Analyze source code offline without real-time interactive feedback (`topos_compiled_evaluate`).
2. **Opportunity Identification** — Extract structural bottlenecks, high-divergence functions, coupling issues, and security vulnerabilities (`topos_compiled_identify_opportunities`).
3. **Plan Proposal** — Generate a structured, step-by-step transformation plan targeting desired lattice verdicts (`topos_compiled_propose_plan`).
4. **Recompilation & State Capture** — Apply plan steps and recompile, capturing content-addressed snapshot state (`topos_compiled_recompile`).
5. **Gain Verification** — Verify lattice progress, score deltas, and AST structural changes against the pre-compilation baseline (`topos_compiled_verify_gains`).
6. **Rollback Safety** — Restore pre-compilation state automatically if verification detects quality regression (`topos_compiled_rollback`).

## Tool Surface

### `topos_compiled_evaluate`
Score source code or target files in compiled execution mode. Evaluates the SIMPLE, COMPOSABLE, SECURE, and NAVIGABLE quality pillars and returns lattice verdicts with compiled contract metadata.

### `topos_compiled_identify_opportunities`
Scan target files for refactoring and optimization opportunities. Identifies high-complexity functions, scope divergence, coupling bottlenecks, and security findings ranked by potential lattice impact.

### `topos_compiled_propose_plan`
Generate a structured refactoring plan targeting specific lattice levels (e.g. `IDEAL`, `SIMPLE_COMPOSABLE_SECURE_NAVIGABLE`). Details sequential refactoring steps, targeted metrics, and risk mitigations.

### `topos_compiled_recompile`
Execute recompilation and snapshot generation following plan application. Captures content-addressed baseline state for verification and rollback.

### `topos_compiled_verify_gains`
Verify structural quality gains by comparing post-compilation metrics against pre-compilation baselines. Computes AST distance, score deltas, and returns acceptance status (`IMPROVEMENT`, `LATERAL_MOVE`, `REGRESSION`).

### `topos_compiled_rollback`
Safely roll back changes to a captured snapshot state if recompilation or verification indicates structural regression or failed verification gates.

## Standard Workflow Example

```json
// Step 1: Offline evaluation
topos_compiled_evaluate({"filepath": "src/main.rs"})

// Step 2: Identify opportunities
topos_compiled_identify_opportunities({"filepath": "src/main.rs", "limit": 5})

// Step 3: Propose plan
topos_compiled_propose_plan({"filepath": "src/main.rs", "target_verdict": "IDEAL"})

// Step 4: Recompile & capture state
topos_compiled_recompile({"filepath": "src/main.rs", "plan_id": "plan-101", "step": 1})

// Step 5: Verify quality gains
topos_compiled_verify_gains({"filepath": "src/main.rs", "snapshot_id": "<snapshot_id>"})

// Step 6: Rollback if needed
topos_compiled_rollback({"filepath": "src/main.rs", "snapshot_id": "<snapshot_id>", "reason": "Lattice regression"})
```
