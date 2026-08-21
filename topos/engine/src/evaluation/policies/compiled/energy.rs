//! Policy translator for ENERGY generator in compiled code evaluation.

use crate::evaluation::policies::base::ScoredDecision;
use std::collections::HashMap;

pub const DEFAULT_MAX_JOULES: f64 = 10.0;
pub const DEFAULT_MAX_MEMORY_ENERGY_RATIO: f64 = 0.50;

pub fn score_energy(
    energy_joules: Option<f64>,
    memory_energy_ratio: Option<f64>,
) -> ScoredDecision {
    let mut interpretation = HashMap::new();
    let mut achieved = true;
    let mut qualities = Vec::new();

    if let Some(joules) = energy_joules {
        let max_joules = DEFAULT_MAX_JOULES;
        let pass = joules <= max_joules;
        if !pass {
            achieved = false;
        }
        let quality = (1.0 - joules / max_joules).clamp(0.0, 1.0);
        qualities.push(quality);
        interpretation.insert(
            "bitcode.energy_joules".to_string(),
            format!(
                "Energy: {:.3} J (max {:.1}, {})",
                joules,
                max_joules,
                if pass { "PASS" } else { "FAIL" }
            ),
        );
    }

    if let Some(ratio) = memory_energy_ratio {
        let max_ratio = DEFAULT_MAX_MEMORY_ENERGY_RATIO;
        let pass = ratio <= max_ratio;
        if !pass {
            achieved = false;
        }
        let quality = (1.0 - ratio / max_ratio).clamp(0.0, 1.0);
        qualities.push(quality);
        interpretation.insert(
            "bitcode.memory_energy_ratio".to_string(),
            format!(
                "Memory energy ratio: {:.2} (max {:.2}, {})",
                ratio,
                max_ratio,
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
    fn test_score_energy_pass() {
        let decision = score_energy(Some(1.2), Some(0.20));
        assert!(decision.achieved);
        assert!(decision.score > 0.5);
    }

    #[test]
    fn test_score_energy_fail() {
        let decision = score_energy(Some(15.0), Some(0.20));
        assert!(!decision.achieved);
        assert_eq!(decision.score, 0.0);
    }
}
