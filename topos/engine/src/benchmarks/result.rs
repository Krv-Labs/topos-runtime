//! Benchmark measurement result types.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkloadMeasurement {
    pub workload_id: String,
    pub description: String,
    pub median_wall_ms: f64,
    pub min_wall_ms: f64,
    pub max_wall_ms: f64,
    pub binary_size_bytes: u64,
    pub bitcode_size_bytes: u64,
    pub instruction_count: u64,
    pub compiled_medal: String,
    pub bitcode_metrics: HashMap<String, f64>,
    pub exit_code: Option<i32>,
    pub measure_runs: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkSuiteResult {
    pub manifest_path: String,
    pub toolchain_usable: bool,
    pub measurements: Vec<WorkloadMeasurement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkBaseline {
    pub version: u32,
    pub platform: String,
    pub manifest_path: String,
    pub measurements: Vec<BaselineWorkloadEntry>,
    #[serde(default)]
    pub curvature_bench_ms: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineWorkloadEntry {
    pub workload_id: String,
    pub median_wall_ms: f64,
    pub instruction_count: u64,
    pub compiled_medal: String,
}

impl BenchmarkBaseline {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn from_suite(suite: &BenchmarkSuiteResult) -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            platform: std::env::consts::OS.to_string(),
            manifest_path: suite.manifest_path.clone(),
            measurements: suite
                .measurements
                .iter()
                .map(|m| BaselineWorkloadEntry {
                    workload_id: m.workload_id.clone(),
                    median_wall_ms: m.median_wall_ms,
                    instruction_count: m.instruction_count,
                    compiled_medal: m.compiled_medal.clone(),
                })
                .collect(),
            curvature_bench_ms: None,
        }
    }

    pub fn diff_against(
        &self,
        current: &BenchmarkSuiteResult,
        wall_tolerance_pct: f64,
    ) -> Vec<String> {
        let mut violations = Vec::new();
        for expected in &self.measurements {
            let Some(actual) = current
                .measurements
                .iter()
                .find(|m| m.workload_id == expected.workload_id)
            else {
                violations.push(format!("missing workload `{}`", expected.workload_id));
                continue;
            };
            let max_allowed = expected.median_wall_ms * (1.0 + wall_tolerance_pct / 100.0);
            if actual.median_wall_ms > max_allowed {
                violations.push(format!(
                    "workload `{}`: wall {:.2} ms exceeds baseline {:.2} ms (+{:.0}%)",
                    expected.workload_id,
                    actual.median_wall_ms,
                    expected.median_wall_ms,
                    wall_tolerance_pct
                ));
            }
        }
        violations
    }
}
