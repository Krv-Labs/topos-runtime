//! Policy translator for SPEED generator in compiled code evaluation.

use crate::evaluation::policies::base::ScoredDecision;
use std::collections::HashMap;

pub const DEFAULT_MAX_CYCLES: f64 = 1000.0;
pub const DEFAULT_MAX_BRANCH_DENSITY: f64 = 0.30;

pub fn score_speed(estimated_cycles: Option<f64>, branch_density: Option<f64>) -> ScoredDecision {
    let mut interpretation = HashMap::new();
    let mut achieved = true;
    let mut qualities = Vec::new();

    if let Some(cycles) = estimated_cycles {
        let max_cycles = DEFAULT_MAX_CYCLES;
        let pass = cycles <= max_cycles;
        if !pass {
            achieved = false;
        }
        let quality = (1.0 - cycles / max_cycles).clamp(0.0, 1.0);
        qualities.push(quality);
        interpretation.insert(
            "bitcode.speed_cycles".to_string(),
            format!(
                "Cycles: {:.1} (max {:.1}, {})",
                cycles,
                max_cycles,
                if pass { "PASS" } else { "FAIL" }
            ),
        );
    }

    if let Some(density) = branch_density {
        let max_density = DEFAULT_MAX_BRANCH_DENSITY;
        let pass = density <= max_density;
        if !pass {
            achieved = false;
        }
        let quality = (1.0 - density / max_density).clamp(0.0, 1.0);
        qualities.push(quality);
        interpretation.insert(
            "bitcode.branch_density".to_string(),
            format!(
                "Branch density: {:.2} (max {:.2}, {})",
                density,
                max_density,
                if pass { "PASS" } else { "FAIL" }
            ),
        );
    }

    let score = if qualities.is_empty() {
        1.0
    } else {
        qualities.into_iter().fold(f64::INFINITY, f64::min)
    };

    ScoredDecision {
        score,
        achieved,
        interpretation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_speed_pass() {
        let decision = score_speed(Some(200.0), Some(0.10));
        assert!(decision.achieved);
        assert!(decision.score > 0.5);
    }

    #[test]
    fn test_score_speed_fail() {
        let decision = score_speed(Some(2000.0), Some(0.10));
        assert!(!decision.achieved);
        assert_eq!(decision.score, 0.0);
    }
}
