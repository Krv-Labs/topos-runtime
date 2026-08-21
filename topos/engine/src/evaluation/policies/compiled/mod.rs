//! Compiled quality policies: measured deltas vs the baseline arm of the
//! same interleaved run.
//!
//! SIZE-satisfied means "did not regress beyond budget". SPEED-satisfied
//! means "improved by ≥ threshold". This mirrors `speedup ≥ 5% / size
//! increase ≤ 10%`. IR stats (instruction/function/block/vector-op counts)
//! are context for reports and must never contribute to a
//! [`GeneratorOutcome`]. LOCALITY is rendered as **MEMORY FOOTPRINT**.
//! ENERGY is permanently unmeasured.

pub mod compiled_omega;
pub mod energy;
pub mod locality;
pub mod outcome;
pub mod size;
pub mod speed;

use self::compiled_omega::CompiledEvaluationValue;
pub use self::outcome::{CompiledVerdict, GeneratorOutcome};

/// Result of evaluating all 4 compiled quality pillars.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledEvaluationResult {
    pub speed: GeneratorOutcome,
    pub size: GeneratorOutcome,
    pub energy: GeneratorOutcome,
    pub locality: GeneratorOutcome,
    pub verdict: CompiledVerdict,
}

impl CompiledEvaluationResult {
    pub fn from_outcomes(
        speed: GeneratorOutcome,
        size: GeneratorOutcome,
        energy: GeneratorOutcome,
        locality: GeneratorOutcome,
    ) -> Self {
        let verdict = CompiledVerdict::from_outcomes(&[
            speed.clone(),
            size.clone(),
            energy.clone(),
            locality.clone(),
        ]);
        Self {
            speed,
            size,
            energy,
            locality,
            verdict,
        }
    }

    pub fn lattice_value(&self) -> CompiledEvaluationValue {
        self.verdict.satisfied()
    }
}

/// IR metrics are context, never a gate. A solo metrics map cannot produce
/// a SPEED/SIZE/LOCALITY delta, so those generators are unmeasured here.
pub fn score_compiled_bitcode(
    _metrics: &std::collections::HashMap<String, f64>,
) -> CompiledEvaluationResult {
    CompiledEvaluationResult::from_outcomes(
        GeneratorOutcome::Unmeasured {
            reason: "SPEED requires an interleaved baseline/variant run, not IR stats",
        },
        GeneratorOutcome::Unmeasured {
            reason: "SIZE requires a baseline binary, not IR stats",
        },
        energy::score_energy(),
        GeneratorOutcome::Unmeasured {
            reason: "MEMORY FOOTPRINT requires /usr/bin/time RSS from an interleaved run",
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ir_stats_alone_cannot_yield_platinum() {
        let mut m = std::collections::HashMap::new();
        m.insert("bitcode.speed_cycles".to_string(), 100.0);
        m.insert("bitcode.energy_joules".to_string(), 0.50);
        let result = score_compiled_bitcode(&m);
        assert_ne!(result.verdict.medal_tier(), "PLATINUM");
        assert_eq!(result.verdict.medal_tier(), "SLOP");
        assert_eq!(result.lattice_value(), CompiledEvaluationValue::Slop);
    }
}
