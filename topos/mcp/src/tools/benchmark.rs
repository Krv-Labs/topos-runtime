//! Benchmark workload tools — Phase 1 baseline evaluation.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use topos_engine::benchmarks::{
    compare_to_baseline_file, default_repo_manifest, evaluate_baseline, load_baseline,
    BenchmarkRunner,
};

use crate::formatting::to_tool_result;
use crate::schemas::{
    CompiledBenchmarkInput, CompiledBenchmarkMeasurement, CompiledBenchmarkResult,
};
use crate::server::ToposServer;

#[tool_router(router = benchmark_router, vis = "pub(crate)")]
impl ToposServer {
    #[tool(
        name = "topos_benchmark",
        annotations(
            title = "Topos Benchmark",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    pub fn topos_benchmark(
        &self,
        Parameters(params): Parameters<CompiledBenchmarkInput>,
    ) -> CallToolResult {
        if !BenchmarkRunner::toolchain_usable() {
            return CallToolResult::error(vec![rmcp::model::ContentBlock::text(
                "benchmarks require clang on PATH",
            )]);
        }

        let manifest = params
            .manifest_path
            .as_deref()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(default_repo_manifest);

        if !manifest.exists() {
            return CallToolResult::error(vec![rmcp::model::ContentBlock::text(format!(
                "manifest not found: {}",
                manifest.display()
            ))]);
        }

        let work_dir = std::env::temp_dir().join(format!("topos_mcp_bench_{}", std::process::id()));
        let tolerance = params.tolerance_pct.unwrap_or(100.0);

        let (evaluation, violations) = if let Some(ref baseline_path) = params.compare_baseline {
            match compare_to_baseline_file(
                &manifest,
                &work_dir,
                std::path::Path::new(baseline_path),
                tolerance,
            ) {
                Ok(pair) => pair,
                Err(err) => {
                    let _ = std::fs::remove_dir_all(&work_dir);
                    return CallToolResult::error(vec![rmcp::model::ContentBlock::text(
                        err.to_string(),
                    )]);
                }
            }
        } else {
            match evaluate_baseline(&manifest, &work_dir) {
                Ok(eval) => (eval, None),
                Err(err) => {
                    let _ = std::fs::remove_dir_all(&work_dir);
                    return CallToolResult::error(vec![rmcp::model::ContentBlock::text(
                        err.to_string(),
                    )]);
                }
            }
        };

        if let Some(ref out_path) = params.write_baseline {
            let mut baseline = evaluation.to_baseline();
            if let Ok(existing) = load_baseline(std::path::Path::new(out_path)) {
                baseline.curvature_bench_ms = existing.curvature_bench_ms;
            }
            if let Ok(json_text) = serde_json::to_string_pretty(&baseline) {
                if let Some(parent) = std::path::Path::new(out_path).parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(out_path, json_text);
            }
        }

        let _ = std::fs::remove_dir_all(&work_dir);

        let measurements: Vec<CompiledBenchmarkMeasurement> = evaluation
            .suite
            .measurements
            .iter()
            .map(|m| CompiledBenchmarkMeasurement {
                workload_id: m.workload_id.clone(),
                median_wall_ms: m.median_wall_ms,
                instruction_count: m.instruction_count,
                compiled_medal: m.compiled_medal.clone(),
                binary_size_bytes: m.binary_size_bytes,
            })
            .collect();

        // `None` (no comparison requested) and `Some(vec![])` (compared, clean)
        // both render as an empty list on the wire; the markdown below only
        // grows a Regressions section when there is something to report.
        let violations = violations.unwrap_or_default();

        let result = CompiledBenchmarkResult {
            manifest_path: manifest.display().to_string(),
            measurements: measurements.clone(),
            baseline_violations: violations.clone(),
        };

        let mut md = format!(
            "# Baseline Benchmark\n\n**Manifest:** `{}`\n\n| Workload | Wall (ms) | Instructions | Medal |\n|---|---:|---:|---|\n",
            manifest.display()
        );
        for m in &measurements {
            md.push_str(&format!(
                "| {} | {:.2} | {} | {} |\n",
                m.workload_id, m.median_wall_ms, m.instruction_count, m.compiled_medal
            ));
        }
        if !violations.is_empty() {
            md.push_str("\n## Regressions\n");
            for v in &violations {
                md.push_str(&format!("- {v}\n"));
            }
        }

        to_tool_result(&result, md)
    }
}
