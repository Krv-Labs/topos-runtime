//! Optimization plan: variants, thresholds, and a blake2 digest.
//!
//! No invented risk scores, no "unsafe pass" list, no `opt` pass names.
//! The digest is blake2b-16 over canonical plan JSON (the digest field
//! itself is excluded) so an approval cannot be replayed against a
//! different plan.

use std::fmt;
use std::path::PathBuf;

use blake2::digest::{Update, VariableOutput};
use blake2::Blake2bVar;
use serde::{Deserialize, Serialize};

use super::variant::{FlagVariant, VariantError};

const MIN_MEASURED_RUNS: u32 = 6;
const MAX_MEASURED_RUNS: u32 = 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptimizationPlan {
    pub source: Option<PathBuf>,
    pub build_command: Option<Vec<String>>,
    pub run_command: Vec<String>,
    pub variants: Vec<String>,
    pub min_speedup_pct: f64,
    pub max_size_increase_pct: f64,
    pub max_rss_increase_pct: f64,
    pub warmup_runs: u32,
    pub measured_runs: u32,
    pub timeout_ms: u64,
    pub digest: String,
}

#[derive(Serialize)]
struct DigestBody<'a> {
    source: &'a Option<PathBuf>,
    build_command: &'a Option<Vec<String>>,
    run_command: &'a [String],
    variants: &'a [String],
    min_speedup_pct: f64,
    max_size_increase_pct: f64,
    max_rss_increase_pct: f64,
    warmup_runs: u32,
    measured_runs: u32,
    timeout_ms: u64,
}

#[derive(Debug, Clone)]
pub struct PlanSpec {
    pub source: Option<PathBuf>,
    pub build_command: Option<Vec<String>>,
    pub run_command: Vec<String>,
    pub variants: Vec<FlagVariant>,
    pub min_speedup_pct: f64,
    pub max_size_increase_pct: f64,
    pub max_rss_increase_pct: f64,
    pub warmup_runs: u32,
    pub measured_runs: u32,
    pub timeout_ms: u64,
}

impl OptimizationPlan {
    pub fn new(spec: PlanSpec) -> Result<Self, PlanError> {
        if spec.variants.is_empty() {
            return Err(PlanError::NoVariants);
        }
        if !(MIN_MEASURED_RUNS..=MAX_MEASURED_RUNS).contains(&spec.measured_runs) {
            return Err(PlanError::MeasuredRunsOutOfRange {
                measured_runs: spec.measured_runs,
                min: MIN_MEASURED_RUNS,
                max: MAX_MEASURED_RUNS,
            });
        }
        if spec.min_speedup_pct <= 0.0 {
            return Err(PlanError::InvalidSpeedup(spec.min_speedup_pct.to_string()));
        }
        let mut plan = Self {
            source: spec.source,
            build_command: spec.build_command,
            run_command: spec.run_command,
            variants: spec.variants.iter().map(|v| v.id().to_string()).collect(),
            min_speedup_pct: spec.min_speedup_pct,
            max_size_increase_pct: spec.max_size_increase_pct,
            max_rss_increase_pct: spec.max_rss_increase_pct,
            warmup_runs: spec.warmup_runs,
            measured_runs: spec.measured_runs,
            timeout_ms: spec.timeout_ms,
            digest: String::new(),
        };
        plan.digest = plan.compute_digest();
        Ok(plan)
    }

    pub fn parsed_variants(&self) -> Result<Vec<FlagVariant>, VariantError> {
        self.variants
            .iter()
            .map(|id| FlagVariant::parse(id))
            .collect()
    }

    pub fn compute_digest(&self) -> String {
        let body = DigestBody {
            source: &self.source,
            build_command: &self.build_command,
            run_command: &self.run_command,
            variants: &self.variants,
            min_speedup_pct: self.min_speedup_pct,
            max_size_increase_pct: self.max_size_increase_pct,
            max_rss_increase_pct: self.max_rss_increase_pct,
            warmup_runs: self.warmup_runs,
            measured_runs: self.measured_runs,
            timeout_ms: self.timeout_ms,
        };
        let bytes = serde_json::to_vec(&body).expect("digest body is always serializable");
        blake2b16_hex(&bytes)
    }

    pub fn verify_digest(&self) -> bool {
        self.digest == self.compute_digest()
    }

    pub fn from_json(text: &str) -> Result<Self, PlanError> {
        let plan: Self = serde_json::from_str(text).map_err(|e| PlanError::Json(e.to_string()))?;
        if !plan.verify_digest() {
            return Err(PlanError::DigestMismatch);
        }
        Ok(plan)
    }
}

pub fn blake2b16_hex(bytes: &[u8]) -> String {
    let mut hasher = Blake2bVar::new(16).expect("16 is a valid BLAKE2b-var digest size");
    hasher.update(bytes);
    let mut out = [0u8; 16];
    hasher
        .finalize_variable(&mut out)
        .expect("16-byte BLAKE2b output");
    out.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    NoVariants,
    MeasuredRunsOutOfRange {
        measured_runs: u32,
        min: u32,
        max: u32,
    },
    InvalidSpeedup(String),
    DigestMismatch,
    Json(String),
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanError::NoVariants => write!(f, "plan must name at least one variant"),
            PlanError::MeasuredRunsOutOfRange {
                measured_runs,
                min,
                max,
            } => write!(
                f,
                "measured_runs must be {min}..={max}, got {measured_runs}"
            ),
            PlanError::InvalidSpeedup(val) => {
                write!(f, "min_speedup_pct must be > 0, got {val}")
            }
            PlanError::DigestMismatch => {
                write!(f, "plan digest does not match canonical contents")
            }
            PlanError::Json(err) => write!(f, "plan JSON: {err}"),
        }
    }
}

impl std::error::Error for PlanError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(runs: u32, min_speedup_pct: f64) -> PlanSpec {
        PlanSpec {
            source: Some(PathBuf::from("a.c")),
            build_command: None,
            run_command: vec!["{output}".into()],
            variants: vec![FlagVariant::O2, FlagVariant::O3],
            min_speedup_pct,
            max_size_increase_pct: 10.0,
            max_rss_increase_pct: 10.0,
            warmup_runs: 2,
            measured_runs: runs,
            timeout_ms: 60_000,
        }
    }

    fn minimal(runs: u32) -> Result<OptimizationPlan, PlanError> {
        OptimizationPlan::new(spec(runs, 5.0))
    }

    #[test]
    fn digest_covers_thresholds_not_a_hardcoded_timestamp() {
        let plan = minimal(10).unwrap();
        assert_eq!(plan.digest.len(), 32);
        assert_ne!(plan.digest, "2026-08-20T19:00:00Z");
        assert!(plan.verify_digest());
    }

    #[test]
    fn changing_a_threshold_changes_the_digest() {
        let a = minimal(10).unwrap();
        let b = OptimizationPlan::new(spec(10, 20.0)).unwrap();
        assert_ne!(a.digest, b.digest);
    }

    #[test]
    fn measured_runs_below_six_are_rejected() {
        let err = minimal(3).unwrap_err();
        assert!(matches!(
            err,
            PlanError::MeasuredRunsOutOfRange {
                measured_runs: 3,
                ..
            }
        ));
    }
}
