//! Optimization plan definition, thresholds, risk bounds, and validation logic.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// High-level LLVM / MLIR optimization pass specifications.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OptimizationPass {
    O2,
    O3,
    Os,
    Oz,
    LTO,
    PGO,
    LoopVectorize,
    SLPVectorize,
    Inlining,
    MlirFusion,
    Custom(String),
}

impl OptimizationPass {
    /// Returns the command-line flag or opt pass name corresponding to this pass.
    pub fn to_flag(&self) -> String {
        match self {
            OptimizationPass::O2 => "-O2".to_string(),
            OptimizationPass::O3 => "-O3".to_string(),
            OptimizationPass::Os => "-Os".to_string(),
            OptimizationPass::Oz => "-Oz".to_string(),
            OptimizationPass::LTO => "-flto".to_string(),
            OptimizationPass::PGO => "-fprofile-use".to_string(),
            OptimizationPass::LoopVectorize => "loop-vectorize".to_string(),
            OptimizationPass::SLPVectorize => "slp-vectorizer".to_string(),
            OptimizationPass::Inlining => "inline".to_string(),
            OptimizationPass::MlirFusion => "mlir-fusion".to_string(),
            OptimizationPass::Custom(flag) => flag.clone(),
        }
    }

    /// Base risk rating for this pass (0.0 = safe, 1.0 = highly risky).
    pub fn base_risk(&self) -> f64 {
        match self {
            OptimizationPass::O2 => 0.1,
            OptimizationPass::Os | OptimizationPass::Oz => 0.15,
            OptimizationPass::O3 => 0.3,
            OptimizationPass::LTO => 0.25,
            OptimizationPass::PGO => 0.2,
            OptimizationPass::LoopVectorize | OptimizationPass::SLPVectorize => 0.35,
            OptimizationPass::Inlining => 0.2,
            OptimizationPass::MlirFusion => 0.5,
            OptimizationPass::Custom(_) => 0.6,
        }
    }

    /// Whether this pass is considered an aggressive or unsafe optimization.
    pub fn is_unsafe(&self) -> bool {
        matches!(
            self,
            OptimizationPass::MlirFusion | OptimizationPass::Custom(_)
        ) || self.base_risk() >= 0.5
    }
}

/// Target performance and quality thresholds required by a plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TargetThresholds {
    pub min_speedup_pct: f64,
    pub max_size_increase_pct: f64,
    pub max_energy_increase_pct: f64,
    pub min_divergence_quality: f64,
}

impl Default for TargetThresholds {
    fn default() -> Self {
        Self {
            min_speedup_pct: 5.0,
            max_size_increase_pct: 10.0,
            max_energy_increase_pct: 0.0,
            min_divergence_quality: 0.40,
        }
    }
}

/// Risk bounds and governance controls for an optimization plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskBounds {
    pub max_risk_score: f64,
    pub allow_unsafe_optimizations: bool,
    pub max_pass_count: usize,
    pub require_human_approval: bool,
}

impl Default for RiskBounds {
    fn default() -> Self {
        Self {
            max_risk_score: 0.50,
            allow_unsafe_optimizations: false,
            max_pass_count: 10,
            require_human_approval: true,
        }
    }
}

/// Lifecycle status of an optimization plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanStatus {
    Draft,
    Validated,
    Approved,
    Rejected,
    Executed,
    RolledBack,
}

/// Errors raised during optimization plan validation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlanValidationError {
    InvalidSpeedupTarget(f64),
    InvalidRiskScore(f64),
    ExceededMaxPassCount { actual: usize, max: usize },
    ExceededRiskBound { score: f64, max_risk: f64 },
    MissingPgoProfile,
    UnsafePassNotAllowed(OptimizationPass),
    EmptyPassSequence,
}

impl fmt::Display for PlanValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanValidationError::InvalidSpeedupTarget(val) => {
                write!(f, "Invalid speedup target: {val}% (must be > 0)")
            }
            PlanValidationError::InvalidRiskScore(val) => {
                write!(
                    f,
                    "Invalid max risk score bound: {val} (must be between 0.0 and 1.0)"
                )
            }
            PlanValidationError::ExceededMaxPassCount { actual, max } => {
                write!(
                    f,
                    "Pass count {actual} exceeds maximum allowed bound of {max}"
                )
            }
            PlanValidationError::ExceededRiskBound { score, max_risk } => {
                write!(f, "Computed plan risk score {score:.2} exceeds maximum allowed risk of {max_risk:.2}")
            }
            PlanValidationError::MissingPgoProfile => {
                write!(f, "PGO pass enabled but no pgo_profile_path was provided")
            }
            PlanValidationError::UnsafePassNotAllowed(pass) => {
                write!(
                    f,
                    "Unsafe pass '{pass:?}' not allowed under current risk bounds"
                )
            }
            PlanValidationError::EmptyPassSequence => {
                write!(f, "Plan contains no optimization passes")
            }
        }
    }
}

impl std::error::Error for PlanValidationError {}

/// Structured specification for an optimization run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptimizationPlan {
    pub plan_id: String,
    pub target_binary: PathBuf,
    pub target_thresholds: TargetThresholds,
    pub risk_bounds: RiskBounds,
    pub passes: Vec<OptimizationPass>,
    pub enable_pgo: bool,
    pub pgo_profile_path: Option<PathBuf>,
    pub status: PlanStatus,
}

impl OptimizationPlan {
    pub fn new(plan_id: impl Into<String>, target_binary: impl Into<PathBuf>) -> Self {
        Self {
            plan_id: plan_id.into(),
            target_binary: target_binary.into(),
            target_thresholds: TargetThresholds::default(),
            risk_bounds: RiskBounds::default(),
            passes: vec![OptimizationPass::O2],
            enable_pgo: false,
            pgo_profile_path: None,
            status: PlanStatus::Draft,
        }
    }

    /// Compute cumulative risk score for the plan based on pass sequence.
    pub fn compute_risk_score(&self) -> f64 {
        if self.passes.is_empty() {
            return 0.0;
        }

        let base_sum: f64 = self.passes.iter().map(|p| p.base_risk()).sum();
        let avg_risk = base_sum / (self.passes.len() as f64);
        let pgo_penalty = if self.enable_pgo { 0.05 } else { 0.0 };

        (avg_risk + pgo_penalty).min(1.0)
    }

    /// Validate plan parameters against thresholds and risk bounds.
    pub fn validate(&mut self) -> Result<(), PlanValidationError> {
        if self.target_thresholds.min_speedup_pct <= 0.0 {
            return Err(PlanValidationError::InvalidSpeedupTarget(
                self.target_thresholds.min_speedup_pct,
            ));
        }

        if self.risk_bounds.max_risk_score < 0.0 || self.risk_bounds.max_risk_score > 1.0 {
            return Err(PlanValidationError::InvalidRiskScore(
                self.risk_bounds.max_risk_score,
            ));
        }

        if self.passes.is_empty() {
            return Err(PlanValidationError::EmptyPassSequence);
        }

        if self.passes.len() > self.risk_bounds.max_pass_count {
            return Err(PlanValidationError::ExceededMaxPassCount {
                actual: self.passes.len(),
                max: self.risk_bounds.max_pass_count,
            });
        }

        if self.enable_pgo && self.pgo_profile_path.is_none() {
            return Err(PlanValidationError::MissingPgoProfile);
        }

        for pass in &self.passes {
            if pass.is_unsafe() && !self.risk_bounds.allow_unsafe_optimizations {
                return Err(PlanValidationError::UnsafePassNotAllowed(pass.clone()));
            }
        }

        let score = self.compute_risk_score();
        if score > self.risk_bounds.max_risk_score {
            return Err(PlanValidationError::ExceededRiskBound {
                score,
                max_risk: self.risk_bounds.max_risk_score,
            });
        }

        self.status = PlanStatus::Validated;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_default_plan() {
        let mut plan = OptimizationPlan::new("plan-001", "target/bin");
        assert_eq!(plan.status, PlanStatus::Draft);
        assert!(plan.validate().is_ok());
        assert_eq!(plan.status, PlanStatus::Validated);
    }

    #[test]
    fn test_exceeded_pass_count() {
        let mut plan = OptimizationPlan::new("plan-002", "target/bin");
        plan.risk_bounds.max_pass_count = 2;
        plan.passes = vec![
            OptimizationPass::O2,
            OptimizationPass::LoopVectorize,
            OptimizationPass::Inlining,
        ];
        assert!(matches!(
            plan.validate(),
            Err(PlanValidationError::ExceededMaxPassCount { actual: 3, max: 2 })
        ));
    }

    #[test]
    fn test_missing_pgo_profile_error() {
        let mut plan = OptimizationPlan::new("plan-pgo", "target/bin");
        plan.enable_pgo = true;
        plan.pgo_profile_path = None;
        assert!(matches!(
            plan.validate(),
            Err(PlanValidationError::MissingPgoProfile)
        ));
    }

    #[test]
    fn test_unsafe_pass_rejection() {
        let mut plan = OptimizationPlan::new("plan-unsafe", "target/bin");
        plan.passes.push(OptimizationPass::MlirFusion);
        plan.risk_bounds.allow_unsafe_optimizations = false;
        assert!(matches!(
            plan.validate(),
            Err(PlanValidationError::UnsafePassNotAllowed(_))
        ));
    }
}
