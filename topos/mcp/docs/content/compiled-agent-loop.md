# Compiled binary optimizer

Search space: **clang driver flags + instrumented PGO**. Not `opt` pass names, not MLIR, not BOLT, not AutoFDO. ENERGY is always Unmeasured. LOCALITY is rendered as MEMORY FOOTPRINT (peak RSS + page faults). Ω_bitcode is a separate parallel lattice — it does not feed this loop.

## Loop

1. `topos_compiled_plan` — resolve the C/C++ target (or `[compiled]` in `.topos.toml`), probe the toolchain, mark each variant buildable/unbuildable, return canonical plan JSON. **Builds and runs nothing.**
2. Human: `topos compiled approve plan.json --by <identity>` — **not an MCP tool**. Exposing approve to an agent is the bypass the gate exists to prevent. `plan`'s `next_step` always says this.
3. `topos_compiled_apply` — refuses unless the plan is approved with a matching digest. Interleaved A/B measurement. Promotes the winner **only if SPEED and SIZE both pass**. Otherwise dest is unchanged and the result is `NO CANDIDATE MET THRESHOLDS`.
4. `topos_compiled_rollback` — restore the exact pre-apply baseline bytes (length-checked, atomic rename, read-back verified).

Default variants: O2, O3, Os, lto, pgo-O3.

Write `plan` from `topos_compiled_plan` to a file, get a human to approve it, then pass that path to `topos_compiled_apply`.
