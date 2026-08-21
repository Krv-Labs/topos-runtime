//! `topos benchmark` — Phase 1 compiled baseline evaluation.

use std::fs;
use std::path::PathBuf;

use clap::Args;
use console::Style;
use serde_json::json;

use topos_engine::benchmarks::{
    compare_to_baseline_file, default_repo_manifest, evaluate_baseline, load_baseline,
    BenchmarkRunner,
};

use super::render::{guide, guide_line, paint, RenderOptions};

#[derive(Args)]
pub struct BenchmarkArgs {
    /// Benchmark manifest TOML (defaults to repo `benchmarks/manifest.toml`).
    #[arg(long)]
    pub manifest: Option<PathBuf>,
    /// Write baseline JSON snapshot to this path.
    #[arg(long)]
    pub write_baseline: Option<PathBuf>,
    /// Compare results against a stored baseline JSON file.
    #[arg(long)]
    pub compare_baseline: Option<PathBuf>,
    /// Allowed wall-clock regression vs baseline (percent).
    #[arg(long, default_value_t = 100.0)]
    pub tolerance_pct: f64,
    /// Output machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

pub fn run(args: BenchmarkArgs) -> Result<(), String> {
    let manifest = args.manifest.unwrap_or_else(default_repo_manifest);
    if !manifest.exists() {
        return Err(format!("manifest not found: {}", manifest.display()));
    }
    if !BenchmarkRunner::toolchain_usable() {
        return Err("benchmarks require clang on PATH".to_string());
    }

    let work_dir = std::env::temp_dir().join(format!("topos_bench_cli_{}", std::process::id()));

    let (evaluation, violations) = if let Some(ref baseline_path) = args.compare_baseline {
        // `v` is already `Option<Vec<String>>` — `Some(v)` here would double-wrap.
        compare_to_baseline_file(&manifest, &work_dir, baseline_path, args.tolerance_pct)
            .map_err(|e| e.to_string())?
    } else {
        (
            evaluate_baseline(&manifest, &work_dir).map_err(|e| e.to_string())?,
            None,
        )
    };

    if let Some(ref out_path) = args.write_baseline {
        let mut baseline = evaluation.to_baseline();
        if let Ok(existing) = load_baseline(out_path) {
            baseline.curvature_bench_ms = existing.curvature_bench_ms;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(
            out_path,
            serde_json::to_string_pretty(&baseline).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }

    let _ = fs::remove_dir_all(&work_dir);

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "phase": "baseline_evaluation",
                "manifest": manifest.display().to_string(),
                "suite": evaluation.suite,
                "baseline_violations": violations,
            }))
            .map_err(|e| e.to_string())?
        );
    } else {
        render_human(&evaluation.suite, violations.as_deref())?;
    }

    if let Some(violations) = violations {
        if !violations.is_empty() {
            return Err(format!("baseline regression: {}", violations.join("; ")));
        }
    }

    Ok(())
}

fn render_human(
    suite: &topos_engine::benchmarks::BenchmarkSuiteResult,
    violations: Option<&[String]>,
) -> Result<(), String> {
    let options = RenderOptions::stdout();
    println!(
        "{}",
        paint(
            "◇  Baseline Benchmark Evaluation",
            Style::new().bold(),
            options
        )
    );
    println!(
        "{}",
        guide_line(
            format!("Manifest: {}", suite.manifest_path),
            Style::new().dim(),
            options
        )
    );
    println!("{}", guide('│', options));
    for m in &suite.measurements {
        println!(
            "{}",
            guide_line(
                format!(
                    "{:<14} {:>8.2} ms  {:>6} inst  {:>8} B  {}",
                    m.workload_id,
                    m.median_wall_ms,
                    m.instruction_count,
                    m.binary_size_bytes,
                    m.compiled_medal
                ),
                Style::new(),
                options,
            )
        );
    }
    if let Some(violations) = violations {
        println!("{}", guide('│', options));
        let label = if violations.is_empty() {
            "Baseline comparison: PASS"
        } else {
            "Baseline comparison: FAIL"
        };
        println!("{}", guide_line(label, Style::new(), options));
        for v in violations {
            println!(
                "{}",
                guide_line(format!("  • {v}"), Style::new().red(), options)
            );
        }
    }
    println!("{}", guide('└', options));
    Ok(())
}
