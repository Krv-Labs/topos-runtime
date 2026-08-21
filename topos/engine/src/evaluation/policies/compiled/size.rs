//! Policy translator for SIZE generator in compiled code evaluation.

use crate::evaluation::policies::base::ScoredDecision;
use std::collections::HashMap;

pub const DEFAULT_MAX_CODE_BYTES: f64 = 65536.0;
pub const DEFAULT_MAX_INSTRUCTIONS: f64 = 10000.0;

pub fn score_size(code_size_bytes: Option<f64>, instruction_count: Option<f64>) -> ScoredDecision {
    let mut interpretation = HashMap::new();
    let mut achieved = true;
    let mut qualities = Vec::new();

    if let Some(bytes) = code_size_bytes {
        let max_bytes = DEFAULT_MAX_CODE_BYTES;
        let pass = bytes <= max_bytes;
        if !pass {
            achieved = false;
        }
        let quality = (1.0 - bytes / max_bytes).clamp(0.0, 1.0);
        qualities.push(quality);
        interpretation.insert(
            "bitcode.code_size_bytes".to_string(),
            format!(
                "Code size: {:.0} bytes (max {:.0}, {})",
                bytes,
                max_bytes,
                if pass { "PASS" } else { "FAIL" }
            ),
        );
    }

    if let Some(insts) = instruction_count {
        let max_insts = DEFAULT_MAX_INSTRUCTIONS;
        let pass = insts <= max_insts;
        if !pass {
            achieved = false;
        }
        let quality = (1.0 - insts / max_insts).clamp(0.0, 1.0);
        qualities.push(quality);
        interpretation.insert(
            "bitcode.instruction_count".to_string(),
            format!(
                "Instruction count: {:.0} (max {:.0}, {})",
                insts,
                max_insts,
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
    fn test_score_size_pass() {
        let decision = score_size(Some(1024.0), Some(500.0));
        assert!(decision.achieved);
        assert!(decision.score > 0.8);
    }

    #[test]
    fn test_score_size_fail() {
        let decision = score_size(Some(100000.0), Some(500.0));
        assert!(!decision.achieved);
        assert_eq!(decision.score, 0.0);
    }
}
