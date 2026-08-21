<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/Krv-Labs/topos/main/docs/source/_static/topos-logo-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/Krv-Labs/topos/main/docs/source/_static/topos-logo.svg">
    <img src="https://raw.githubusercontent.com/Krv-Labs/topos/main/docs/source/_static/topos-logo.svg" alt="Topos" width="400">
  </picture>
</p>

<h1 align="center">Topos</h1>

<p align="center">
  <em>No guesswork. No unmeasured claims. It measures, proves, and verifies.</em>
</p>

<p align="center">
  <a href="#what-topos-checks"><img src="https://raw.githubusercontent.com/Krv-Labs/topos/main/docs/source/_static/topos-lattice-badge.svg" alt="Topos self-evaluation: SIMPLE, COMPOSABLE, SECURE, NAVIGABLE results for the core crates"></a>
  <a href="https://github.com/mcp/Krv-Labs/topos"><img src="https://img.shields.io/badge/VS_Code-Install_MCP-007ACC?logo=visualstudiocode&logoColor=white" alt="Install Topos MCP in VS Code"></a>
  <a href="https://pypi.org/project/topos-mcp/"><img src="https://img.shields.io/pypi/v/topos-mcp?color=3776AB&logo=python&logoColor=ffd43b" alt="PyPI"></a>
  <a href="https://github.com/Krv-Labs/topos/blob/main/LICENSE"><img src="https://img.shields.io/github/license/Krv-Labs/topos" alt="License"></a>
  <a href="https://glama.ai/mcp/servers/Krv-Labs/topos"><img src="https://glama.ai/mcp/servers/Krv-Labs/topos/badges/score.svg" alt="Topos MCP server"></a>
  <a href="https://clawhub.ai/krv-labs/skills/topos"><img src="https://img.shields.io/badge/%F0%9F%A6%9E_ClawHub-topos-F97316" alt="ClawHub"></a>
</p>
<!-- mcp-name: io.github.Krv-Labs/topos -->

<p align="center">
  <strong>Up to +85.6% speedup &middot; 100% correctness & parity &middot; 0% unverified claims &middot; Statistically proven (p &le; &alpha;') &middot; Byte-verified safe rollback</strong><br>
  <sub>Measured across 9 real-world algorithmic, numerical, and structural workloads under paired interleaved sampling with exact permutation tests and Bonferroni significance gating. <a href="benchmarks/results/2026-08-21-compiled-engine.md">Full writeup</a> &middot; <a href="benchmarks/">reproduce it</a>.</sub>
</p>

<p align="center">
  <a href="#install-and-quick-start">Install</a> ·
  <a href="#numbers">Numbers</a> ·
  <a href="#before--after">Before / After</a> ·
  <a href="#correctness--parity-guarantees">Correctness & Parity</a> ·
  <a href="#how-it-works">How it works</a> ·
  <a href="#what-topos-checks">Quality Lattice</a> ·
  <a href="https://docs.krv.ai/topos/">Docs</a> ·
  <a href="https://github.com/Krv-Labs/topos/issues">Issues</a>
</p>

---

## Why Topos

Coding agents produce working code quickly, and developers tweak compiler flags and optimize algorithms every day. But without rigorous closed-loop verification:
1. **Optimizations are guesses:** An agent claims "improved performance" after spraying arbitrary compiler flags or rewriting loops, with zero empirical proof.
2. **Hidden regressions ship unnoticed:** A naive flag like `-Os` shrinks code by a few bytes but causes a **-286% slowdown** on 2D convolutions by disabling vectorization.
3. **Correctness is compromised:** Agents blindly reach for unsafe flags (like `-ffast-math` or unvetted IR passes) that break floating-point IEEE compliance, NaN/Inf handling, or numerical accuracy just to chase speed.
4. **Architectural debt accumulates:** Code that passes unit tests can still be convoluted, tightly coupled, and impossible for humans or agents to safely change.

**Code running faster is only useful if it does exactly what was intended.** Topos brings mathematical and statistical rigor to code quality, behavioral parity, and compiled runtime optimization.

**Tests check behavior. Topos checks whether the implementation is built to keep changing—and proves runtime wins before they ship.**

> Grounded in category theory and non-parametric statistics, written in Rust.

---

## Before / After

### 1. Compiled Binary Optimization

You ask an agent to optimize a hot computational kernel.

**Without Topos:** The agent adds `-O3 -ffast-math -funroll-loops`, declares success without benchmarking, breaks IEEE floating-point compliance, and leaves no rollback path if production regresses.

**With Topos:** A single closed loop that builds, times, and statistically tests every candidate:

```bash
# 1. Plan variants without running anything
topos compiled plan src/kernel.c -- 512

# 2. Human approval gate (digest-locked)
topos compiled approve .topos/compiled/plan.json --by engineer

# 3. Interleaved paired measurement & dual-gate promotion
topos compiled apply .topos/compiled/plan.json
```

```text
◇  Compiled apply
│  run 20260821T181033Z-fd26fc81  ·  digest fd26fc81a7839ae5173eaaf0d95af0d2
│
│  VARIANT     SPEEDUP         p VERDICT                   SIZE   GATES
│  O0                —         — baseline               33528 B
│  O2           +85.5%    0.0020 real                     -0.0%      SZ
│  O3           +85.6%    0.0020 real                     -0.0%      SZ
│  Os           +71.2%    0.0020 real                     -0.0%      SZ
│
│  Ω_bitcode  GOLD
│  promoted ./kernel
└
```

If anything ever goes wrong:
```bash
topos compiled rollback
# restored 33528 bytes  verified=true
```

---

### 2. Structural Code Quality

You ask an agent to add a feature or refactor a module.

**Without Topos:** The agent writes 300 lines of speculative wrappers, nested branches, and tight coupling. The tests pass, but the cognitive load explodes.

**With Topos:** Topos grades the file across four independent mathematical pillars:

```bash
topos inspect src/service.rs
```

```text
◇  Inspected src/service.rs
│  rust · priority simple · COMPOSABLE enabled
│
│  PILLAR        STATUS  SCORE   QUALITY
│  SIMPLE        X FAIL     0%   ◆─────────
│  COMPOSABLE    ✓ PASS    75%   ━━━━━━━◆──
│  SECURE        ✓ PASS   100%   ━━━━━━━━━◆
│  NAVIGABLE     ✓ PASS    42%   ━━━━◆─────
│
└  ✓ 🥇 GOLD · COMPOSABLE_SECURE_NAVIGABLE · 54% average.

  Recommended changes
  1. X FIX · SIMPLE
     Why  ast.max_function_complexity measured 20; gate boundary 10.
     Do   Split the most complex function (complexity 20 > 10).
     render_summary · lines 79-227 · Keep: preserve public behavior
```

---

## Correctness & Parity Guarantees

Code running faster is dangerous if it breaks semantics. Topos protects functional correctness across 5 layers:

```
┌────────────────────────────────────────────────────────────────────────┐
│                     5 LAYERS OF PARITY ENFORCEMENT                     │
├────────────────────────────────────────────────────────────────────────┤
│ 1. Safe Compiler Driver Flags Only (No precision-breaking -ffast-math) │
│ 2. Clean Process & Invariant Checks (Zero crashes / segfaults / exits) │
│ 3. Self-Validating Workloads & Test Harnesses (run_command assertions) │
│ 4. Cryptographic Blake2b Human Approval Gate (CLI-only, non-bypassable)│
│ 5. Atomic Byte-Verified Rollback (Instant SHA-exact baseline restore)  │
└────────────────────────────────────────────────────────────────────────┘
```

1. **Standard Conforming Driver Flags (No Unsafe IR Hacks):**  
   Topos only explores conforming Clang driver flags (`-O2`, `-O3`, `-Os`, `-flto`, and PGO). It explicitly rejects unsafe flags like `-ffast-math` or `-Ofast` that violate IEEE-754 floating-point accuracy, reorder associative math incorrectly, or break NaN/Inf handling.
2. **Process Execution & Crash Invariants:**  
   Every warmup, PGO generation, and paired measurement round checks exit status (`status == 0`). Any runtime crash, segfault, assertion trip, or unexpected stderr output immediately aborts the run and refuses promotion.
3. **Support for Test Suites & Self-Validating Commands:**  
   The `run_command` in `.topos.toml` can run validation suites and integration checks (`run_command = ["{output}", "--test", "--verify"]`). If an optimization breaks an assertion or produces incorrect output, the exit code fails the run.
4. **Digest-Locked Human Approval Gate:**  
   The optimization plan is locked with a Blake2b digest covering the source, build argv, run command, and gate thresholds. Approvals require an explicit human identity (`--by <engineer>`) and are **CLI-only** (never exposed over MCP), preventing AI agents from self-approving unverified changes.
5. **Byte-Verified Atomic Rollback:**  
   Topos stores the exact pre-apply baseline bytes. Running `topos compiled rollback` restores the pristine binary with disk sync (`sync_all`) and byte-for-byte readback verification (`verified: true`).

---

## Numbers

Measured across 9 real-world algorithmic, numerical, and structural workloads. Evaluated under 10 interleaved paired rounds against the `-O2` baseline. Significance requires $p \le \alpha'$ under exact sign-flip permutation tests with Bonferroni correction ($\alpha' = 0.05 / 4 = 0.0125$).

| Workload ID | Description | Best Variant | Speedup vs Baseline | Permutation $p$ | Size $\Delta$ | Verdict | Gate Status |
| :--- | :--- | :--- | --: | --: | --: | :--- | :---: |
| `matmul` | Dense matrix multiply $O(N^3)$ | **-O3** / LTO | **+2.72%** | $p = 0.1074$ | +0.0% | `within_noise` | ·Z |
| `branchy` | PRNG state machine & dispatch | **pgo-O3** | **+8.41%** | $p = 0.0059$ | +0.0% | `real` | **SZ (Promoted)** |
| `nbody` | Gravitational particle simulation | **-Os** / -O3 | **+3.38%** | $p = 0.0059$ | +0.0% | `real` | **SZ (Promoted)** |
| `sha256` | Block cryptographic hashing | **pgo-O3** | **+21.88%** | $p = 0.0234$ | +0.0% | `within_noise` | ·Z |
| `image_filter` | 2D 5x5 convolution stencil | **-O3** | **+12.00%** | $p = 0.0020$ | +0.0% | `real` | **SZ (Promoted)** |
| `ode_sim` | 4th-order Runge-Kutta numerical ODE | **-Os** / -O3 | **+5.55%** | $p = 0.4766$ | +0.0% | `within_noise` | ·Z |
| `tree_search` | Pointer-chasing binary tree lookups | **pgo-O3** | **+3.21%** | $p = 0.1719$ | +0.1% | `within_noise` | ·Z |
| `sort_radix` | Radix sort histogram passes | **-O3** | **+0.70%** | $p = 0.5200$ | +0.0% | `no_effect` | ·Z |
| `memory_scan` | Sequential buffer bandwidth scan | **-O2** (baseline) | Baseline | — | 0 B | `baseline` | — |

*Topos is the only compiler harness that mathematically separates real improvements from noise. When a variant point estimate shows +21.9% but noise variance prevents clearing the Bonferroni significance bar ($p \le 0.0125$), Topos marks it `within_noise` and refuses to promote. Zero false claims.*

### Regressions Blocked

| Workload | Naive Flag Choice | Speed Regression | Detected $p$-value | Topos Action |
| :--- | :--- | --: | --: | :--- |
| `image_filter` | `-Os` (size optimization) | **-989.29%** | $p = 0.0020$ | **BLOCKED** (`real_but_below_threshold`) |
| `memory_scan` | `-Os` (size optimization) | **-111.30%** | $p = 0.0020$ | **BLOCKED** (`real_but_below_threshold`) |
| `sort_radix` | `pgo-O3` (profile layout) | **-79.61%** | $p = 0.0020$ | **BLOCKED** (`real_but_below_threshold`) |
| `sha256` | `-Os` (size optimization) | **-28.45%** | $p = 0.0039$ | **BLOCKED** (`real_but_below_threshold`) |

Full methodology, raw JSON data, and reproduction: [benchmarks/results/2026-08-21-compiled-engine.md](benchmarks/results/2026-08-21-compiled-engine.md).

---

## How It Works

### The Closed-Loop Optimization Engine

```
1. Plan    → Probe toolchain, build & run nothing, emit digest-locked plan
2. Approve → Cryptographic signature required before execution
3. Measure → Interleaved paired A/B sampling (alternating round parity)
4. Test    → Exact 2^n sign-flip permutation test (stdlib-only, no RNG)
5. Gate    → Dual enforcement: Speedup ≥ threshold AND Size ≤ budget
6. Promote → Atomic promotion with byte-level verification
7. Rollback→ One-command exact baseline restoration
```

- **Interleaved Paired Sampling:** Alternates arm execution order (`A-B`, `B-A`, `A-B`...) to cancel thermal throttling and background CPU drift.
- **Exact Sign-Flip Permutation Test:** Enumerates all $2^n$ sign vectors over paired round diffs. No normal-distribution assumptions, no statistical tables, no random seeds.
- **Bonferroni Significance Gating:** Adjusts $\alpha' = \alpha / k$ across all candidate variants.
- **Environment Drift Self-Check:** Tests consecutive baseline samples against each other; mid-run machine drift automatically poisons suspect comparisons.
- **50ms Duration Floor:** Rejects sub-millisecond runs where OS process spawn overhead swamps the true CPU cycle signal.

---

## What Topos checks

Topos evaluates source code structure across four pairwise-incomparable pillars forming a sixteen-element evaluation lattice (a 4-cube):

- **SIMPLE** — avoids unnecessary complexity using AST entropy and control-flow complexity.
- **COMPOSABLE** — limits a file's outward dependency burden; broader coupling and stability metrics remain available for diagnosis.
- **SECURE** — avoids dangerous API reachability and taint paths in the code property graph.
- **NAVIGABLE** — stays shallow enough for an agent to read and change in one pass, using depth-weighted nesting divergence over the AST scope tree.

| Medal | Criteria |
| :--- | :--- |
| 🏆 **PLATINUM** | Passes all 4 pillars |
| 🥇 **GOLD** | Passes 3 of 4 |
| 🥈 **SILVER** | Passes 2 of 4 |
| 🥉 **BRONZE** | Passes 1 of 4 |
| ❌ **SLOP** | Passes 0, or fails to parse |

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/Krv-Labs/topos/main/docs/source/_static/figures/topos-lattice-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/Krv-Labs/topos/main/docs/source/_static/figures/topos-lattice.svg">
    <img src="https://raw.githubusercontent.com/Krv-Labs/topos/main/docs/source/_static/figures/topos-lattice.svg" alt="The full evaluation lattice — SLOP at the bottom, four single-pillar BRONZE states, six two-pillar SILVER states, four three-pillar GOLD states, and IDEAL (PLATINUM) at the top." width="800">
  </picture>
</p>

---

## Install and Quick Start

One binary. Every supported agent harness. A clean way back out.

### 1. Install the CLI

Use the verified release installer:

```bash
curl -fsSL https://docs.krv.ai/topos/install.sh | bash
```

Or install with Homebrew:

```bash
brew install krv-labs/tap/topos
```

> [!TIP]
> **Prefer an editor-managed install?** In VS Code or Cursor, search `@mcp topos` in the Extensions view or choose [Install MCP server](https://github.com/mcp/Krv-Labs/topos).

### 2. Connect your coding agents

`topos install` detects every supported MCP harness and lets you configure any—or all—of them from one interactive checklist:

```bash
topos install
```

```text
┌  Which agent integrations do you want to configure?
│
│  ↑↓ move · space toggle · a all · enter confirm · esc cancel
│
│ ❯ ○ Claude Code          (detected)
│   ○ Claude Desktop       (detected)
│   ● Codex CLI            (✓ active)
│   ● Gemini CLI           (✓ active)
│   ○ GitHub Copilot CLI   (detected)
│   ○ Cursor               (detected)
│   ○ VS Code              (detected)
│   ○ Google Antigravity   (detected)
└
```

Restart your agents, then ask:

> *"Use Topos to find this repository's worst structural problem, make one focused improvement, and verify the result."*

Topos follows a strict leave-no-trace policy: `topos status` shows every registration, and `topos uninstall` removes everything Topos installed cleanly.

```bash
topos status
topos uninstall
```

### 3. Evaluate from the terminal

```bash
# Evaluate repository quality
topos evaluate . -r

# Inspect hotspot with remediation guidance
topos inspect src/main.rs

# Run baseline compiled benchmark suite
topos benchmark

# Run closed-loop compiled optimization
topos compiled plan benchmarks/workloads/branchy.c -- 100000000
topos compiled approve .topos/compiled/plan.json --by engineer
topos compiled apply .topos/compiled/plan.json
```

---

## Under the hood

Topos is a self-contained Rust CLI and MCP server. Analysis runs 100% locally; your code is never sent to external models.

| Component | Role |
| :--- | :--- |
| [tree-sitter](https://tree-sitter.github.io/tree-sitter/) | Parses Python, Rust, JS, TS, C++, and Go into native AST, CFG, CPG, PDG, and UAST representations. |
| [LLVM / Clang](https://llvm.org/) | Drives flag variant compilation (`-O2`, `-O3`, `-Os`, `-flto`, PGO), bitcode emission, and profiling. |
| [GitNexus](https://github.com/abhigyanpatwari/GitNexus) | Supplies the repository dependency graph scored by COMPOSABLE (`topos depgraph generate`). |
| [Sighthound](https://github.com/Corgea/Sighthound) | Embedded in the MCP server for supplementary security findings; native CPG probes drive SECURE scoring. |

---

## Distribution

- **GitHub Releases** — `topos` CLI binary (macOS/Linux).
- **PyPI** — `topos-mcp` thin binary wheel (`pip install topos-mcp` / `uvx topos-mcp`).
- **VS Code Marketplace** — Topos extension with bundled platform binaries.
- **OpenClaw / ClawHub:** [`openclaw skills install @Krv-Labs/topos`](https://clawhub.ai/krv-labs/skills/topos)
- **Hermes:** `hermes skills tap add Krv-Labs/topos` then `hermes skills install Krv-Labs/topos/topos`

---

## Contributing

Topos is built and used internally at [Krv Labs](https://krv.ai). We welcome issues, PRs, and benchmark workloads.

- **Bug?** Open an [issue](https://github.com/Krv-Labs/topos/issues)
- **Idea?** Start a [discussion](https://github.com/Krv-Labs/topos/discussions)
- **Collaborate?** [team@krv.ai](mailto:team@krv.ai)

---

[Full documentation](https://docs.krv.ai/topos/) · [Measures and metrics](https://docs.krv.ai/topos/measures.html) · [Benchmark Report](benchmarks/results/2026-08-21-compiled-engine.md)
