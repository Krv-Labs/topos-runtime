//! Compiled benchmark harness for Phase 1 baseline evaluation.

pub mod baseline;
pub mod manifest;
pub mod result;
pub mod runner;

pub use baseline::{
    compare_to_baseline_file, compiled_medal_from_metrics, default_repo_manifest,
    evaluate_baseline, load_baseline, BaselineEvaluation,
};
pub use manifest::{BenchmarkDefaults, BenchmarkManifest, WorkloadSpec};
pub use result::{
    BaselineWorkloadEntry, BenchmarkBaseline, BenchmarkSuiteResult, WorkloadMeasurement,
};
pub use runner::{BenchmarkError, BenchmarkRunner};
