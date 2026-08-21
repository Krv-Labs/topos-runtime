//! Policy translator for LOCALITY generator in compiled code evaluation.

use crate::evaluation::policies::base::ScoredDecision;
use std::collections::HashMap;

pub const DEFAULT_MAX_CACHE_MISS_RATIO: f64 = 0.15;
pub const DEFAULT_MIN_LOCALITY_SCORE: f64 = 0.50;

pub fn score_locality(
    cache_miss_ratio: Option<f64>,
    locality_score: Option<f64>,
) -> ScoredDecision {
    let mut interpretation = HashMap::new();
    let mut achieved = true;
    let mut qualities = Vec::new();

    if let Some(miss_ratio) = cache_miss_ratio {
        let max_miss = DEFAULT_MAX_CACHE_MISS_RATIO;
        let pass = miss_ratio <= max_miss;
        if !pass {
            achieved = false;
        }
        let quality = (1.0 - miss_ratio / max_miss).clamp(0.0, 1.0);
        qualities.push(quality);
        interpretation.insert(
            "bitcode.cache_miss_ratio".to_string(),
            format!(
                "Cache miss ratio: {:.2} (max {:.2}, {})",
                miss_ratio,
                max_miss,
                if pass { "PASS" } else { "FAIL" }
            ),
        );
    }

    if let Some(locality) = locality_score {
        let min_locality = DEFAULT_MIN_LOCALITY_SCORE;
        let pass = locality >= min_locality;
        if !pass {
            achieved = false;
        }
        let quality = locality.clamp(0.0, 1.0);
        qualities.push(quality);
        interpretation.insert(
            "bitcode.locality_score".to_string(),
            format!(
                "Locality score: {:.2} (min {:.2}, {})",
                locality,
                min_locality,
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
    fn test_score_locality_pass() {
        let decision = score_locality(Some(0.05), Some(0.85));
        assert!(decision.achieved);
        assert!(decision.score > 0.5);
    }

    #[test]
    fn test_score_locality_fail() {
        let decision = score_locality(Some(0.30), Some(0.85));
        assert!(!decision.achieved);
        assert_eq!(decision.score, 0.0);
    }
}
