//! Opportunity identification and candidate ranking for compiler optimizations.

use super::plan::OptimizationPass;
use crate::adapters::perf::SystemPerformanceProfile;
use crate::evaluation::preferences::UserPreferences;
use serde::{Deserialize, Serialize};

/// Types of optimization opportunities identified during analysis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpportunityType {
    HotFunction,
    VectorizationPotential,
    MlirFusionGap,
    Custom(String),
}

/// An identified optimization candidate for function-level or loop-level pass targeting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptimizationCandidate {
    pub candidate_id: String,
    pub function_name: String,
    pub location: Option<String>,
    pub opportunity_type: OpportunityType,
    pub priority_score: f64,
    pub estimated_speedup_pct: f64,
    pub risk_rating: f64,
    pub recommended_passes: Vec<OptimizationPass>,
}

impl OptimizationCandidate {
    /// Compute or re-evaluate the priority score for this candidate.
    pub fn compute_priority_score(&mut self) {
        let base_benefit = self.estimated_speedup_pct * 1.5;
        let risk_penalty = 1.0 + self.risk_rating;
        self.priority_score = (base_benefit / risk_penalty).max(0.0);
    }
}

/// Opportunity identifier and analyzer.
pub struct OpportunityIdentifier;

impl OpportunityIdentifier {
    /// Analyze a system performance profile and identify optimization candidates.
    pub fn identify_from_profile(profile: &SystemPerformanceProfile) -> Vec<OptimizationCandidate> {
        let mut candidates = Vec::new();

        for (idx, hot_fn) in profile.hot_functions.iter().enumerate() {
            let candidate_id = format!("cand-hot-{}", idx + 1);

            if hot_fn.percentage >= 10.0 {
                candidates.push(OptimizationCandidate {
                    candidate_id: candidate_id.clone(),
                    function_name: hot_fn.name.clone(),
                    location: hot_fn.location.clone(),
                    opportunity_type: OpportunityType::HotFunction,
                    priority_score: 0.0,
                    estimated_speedup_pct: hot_fn.percentage * 0.4,
                    risk_rating: 0.2,
                    recommended_passes: vec![
                        OptimizationPass::O3,
                        OptimizationPass::Inlining,
                        OptimizationPass::PGO,
                    ],
                });
            }

            // High execution time functions with loop indicators are candidates for vectorization
            if hot_fn.name.contains("loop")
                || hot_fn.name.contains("vector")
                || hot_fn.name.contains("compute")
                || hot_fn.name.contains("matmul")
                || hot_fn.name.contains("sum")
            {
                candidates.push(OptimizationCandidate {
                    candidate_id: format!("cand-vec-{}", idx + 1),
                    function_name: hot_fn.name.clone(),
                    location: hot_fn.location.clone(),
                    opportunity_type: OpportunityType::VectorizationPotential,
                    priority_score: 0.0,
                    estimated_speedup_pct: hot_fn.percentage * 0.6,
                    risk_rating: 0.35,
                    recommended_passes: vec![
                        OptimizationPass::LoopVectorize,
                        OptimizationPass::SLPVectorize,
                    ],
                });
            }
        }

        for cand in &mut candidates {
            cand.compute_priority_score();
        }

        candidates
    }

    /// Identify MLIR fusion gap candidates across kernel / tensor function signatures.
    pub fn identify_mlir_fusion_gaps(function_names: &[&str]) -> Vec<OptimizationCandidate> {
        let mut candidates = Vec::new();

        for (idx, &fn_name) in function_names.iter().enumerate() {
            if fn_name.contains("tensor")
                || fn_name.contains("conv")
                || fn_name.contains("fusion")
                || fn_name.contains("kernel")
            {
                let mut cand = OptimizationCandidate {
                    candidate_id: format!("cand-mlir-{}", idx + 1),
                    function_name: fn_name.to_string(),
                    location: None,
                    opportunity_type: OpportunityType::MlirFusionGap,
                    priority_score: 0.0,
                    estimated_speedup_pct: 12.5,
                    risk_rating: 0.5,
                    recommended_passes: vec![OptimizationPass::MlirFusion],
                };
                cand.compute_priority_score();
                candidates.push(cand);
            }
        }

        candidates
    }
}

/// Rank optimization candidates based on priority score and optional user preferences.
pub fn rank_candidates(
    candidates: &mut [OptimizationCandidate],
    _preferences: Option<&UserPreferences>,
) {
    for cand in candidates.iter_mut() {
        cand.compute_priority_score();
    }

    candidates.sort_by(|a, b| {
        b.priority_score
            .partial_cmp(&a.priority_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::perf::HotFunctionInfo;

    #[test]
    fn test_identify_hot_functions() {
        let profile = SystemPerformanceProfile {
            platform: "linux".into(),
            hot_functions: vec![
                HotFunctionInfo {
                    name: "matrix_multiply".into(),
                    sample_count: 5000,
                    percentage: 35.0,
                    location: Some("src/math.rs".into()),
                },
                HotFunctionInfo {
                    name: "idle_wait".into(),
                    sample_count: 200,
                    percentage: 2.0,
                    location: None,
                },
            ],
            is_degraded: false,
            ..Default::default()
        };

        let candidates = OpportunityIdentifier::identify_from_profile(&profile);
        assert!(!candidates.is_empty());
        assert_eq!(candidates[0].function_name, "matrix_multiply");
        assert!(candidates[0].priority_score > 0.0);
    }

    #[test]
    fn test_identify_mlir_fusion_gaps() {
        let fn_list = vec!["conv2d_bias_relu", "scalar_add"];
        let candidates = OpportunityIdentifier::identify_mlir_fusion_gaps(&fn_list);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].function_name, "conv2d_bias_relu");
        assert_eq!(
            candidates[0].opportunity_type,
            OpportunityType::MlirFusionGap
        );
    }

    #[test]
    fn test_ranking_candidates() {
        let mut candidates = vec![
            OptimizationCandidate {
                candidate_id: "c1".into(),
                function_name: "low_impact".into(),
                location: None,
                opportunity_type: OpportunityType::HotFunction,
                priority_score: 0.0,
                estimated_speedup_pct: 2.0,
                risk_rating: 0.1,
                recommended_passes: vec![],
            },
            OptimizationCandidate {
                candidate_id: "c2".into(),
                function_name: "high_impact".into(),
                location: None,
                opportunity_type: OpportunityType::HotFunction,
                priority_score: 0.0,
                estimated_speedup_pct: 25.0,
                risk_rating: 0.2,
                recommended_passes: vec![],
            },
        ];

        rank_candidates(&mut candidates, None);
        assert_eq!(candidates[0].candidate_id, "c2");
        assert_eq!(candidates[1].candidate_id, "c1");
    }
}
