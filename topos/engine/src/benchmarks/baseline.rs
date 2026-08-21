//! Phase 1 baseline evaluation.

use std::fs;
use std::path::{Path, PathBuf};

use crate::evaluation::policies::compiled::score_compiled_bitcode;

use super::manifest::BenchmarkManifest;
use super::result::{BenchmarkBaseline, BenchmarkSuiteResult};
use super::runner::{BenchmarkError, BenchmarkRunner};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BaselineEvaluation {
    pub suite: BenchmarkSuiteResult,
}

impl BaselineEvaluation {
    pub fn to_baseline(&self) -> BenchmarkBaseline {
        BenchmarkBaseline::from_suite(&self.suite)
    }
}

pub fn evaluate_baseline(
    manifest_path: &Path,
    work_dir: &Path,
) -> Result<BaselineEvaluation, BenchmarkError> {
    let manifest = BenchmarkManifest::load(manifest_path).map_err(BenchmarkError::Manifest)?;
    let runner = BenchmarkRunner::new(work_dir)?;
    Ok(BaselineEvaluation {
        suite: runner.run_manifest(&manifest)?,
    })
}

pub fn load_baseline(path: &Path) -> Result<BenchmarkBaseline, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("parsing {}: {e}", path.display()))
}

pub fn compare_to_baseline_file(
    manifest_path: &Path,
    work_dir: &Path,
    baseline_path: &Path,
    wall_tolerance_pct: f64,
) -> Result<(BaselineEvaluation, Option<Vec<String>>), BenchmarkError> {
    let current = evaluate_baseline(manifest_path, work_dir)?;
    let baseline = load_baseline(baseline_path).map_err(BenchmarkError::Manifest)?;
    let violations = baseline.diff_against(&current.suite, wall_tolerance_pct);
    // `Some` even when empty: an empty vec means "compared, no regressions",
    // which callers render differently from `None` ("no comparison requested").
    Ok((current, Some(violations)))
}

pub fn default_repo_manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/manifest.toml")
}

pub fn compiled_medal_from_metrics(metrics: &std::collections::HashMap<String, f64>) -> String {
    score_compiled_bitcode(metrics)
        .verdict
        .medal_tier()
        .to_string()
}
