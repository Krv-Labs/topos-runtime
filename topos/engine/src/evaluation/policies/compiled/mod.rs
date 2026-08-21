//! Compiled quality policies for bitcode / IR evaluation.

pub mod compiled_omega;
pub mod energy;
pub mod locality;
pub mod size;
pub mod speed;

use self::compiled_omega::{
    verdict_from_compiled_generators, CompiledEvaluationValue, CompiledGenerator,
};
use crate::evaluation::policies::base::ScoredDecision;
use std::collections::HashMap;

/// Result of evaluating all 4 compiled quality pillars over a bitcode object's metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledEvaluationResult {
    pub speed: ScoredDecision,
    pub size: ScoredDecision,
    pub energy: ScoredDecision,
    pub locality: ScoredDecision,
    pub verdict: CompiledEvaluationValue,
}

/// Evaluates all 4 compiled policies from a metrics map.
pub fn score_compiled_bitcode(metrics: &HashMap<String, f64>) -> CompiledEvaluationResult {
    let speed = speed::score_speed(
        metrics.get("bitcode.speed_cycles").copied(),
        metrics.get("bitcode.branch_density").copied(),
    );

    let size = size::score_size(
        metrics.get("bitcode.code_size_bytes").copied(),
        metrics.get("bitcode.instruction_count").copied(),
    );

    let energy = energy::score_energy(
        metrics.get("bitcode.energy_joules").copied(),
        metrics.get("bitcode.memory_energy_ratio").copied(),
    );

    let locality = locality::score_locality(
        metrics.get("bitcode.cache_miss_ratio").copied(),
        metrics.get("bitcode.locality_score").copied(),
    );

    let mut satisfied = Vec::new();
    if speed.achieved {
        satisfied.push(CompiledGenerator::Speed);
    }
    if size.achieved {
        satisfied.push(CompiledGenerator::Size);
    }
    if energy.achieved {
        satisfied.push(CompiledGenerator::Energy);
    }
    if locality.achieved {
        satisfied.push(CompiledGenerator::Locality);
    }

    let verdict = verdict_from_compiled_generators(&satisfied);

    CompiledEvaluationResult {
        speed,
        size,
        energy,
        locality,
        verdict,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_compiled_bitcode_all_pass() {
        let mut m = HashMap::new();
        m.insert("bitcode.speed_cycles".to_string(), 100.0);
        m.insert("bitcode.branch_density".to_string(), 0.10);
        m.insert("bitcode.code_size_bytes".to_string(), 1024.0);
        m.insert("bitcode.instruction_count".to_string(), 200.0);
        m.insert("bitcode.energy_joules".to_string(), 0.50);
        m.insert("bitcode.memory_energy_ratio".to_string(), 0.10);
        m.insert("bitcode.cache_miss_ratio".to_string(), 0.02);
        m.insert("bitcode.locality_score".to_string(), 0.90);

        let result = score_compiled_bitcode(&m);
        assert_eq!(result.verdict, CompiledEvaluationValue::Ideal);
        assert_eq!(result.verdict.medal_tier(), "PLATINUM");
    }
}
