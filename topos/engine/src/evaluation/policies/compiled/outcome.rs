//! Parallel measured-mask for `Ω_bitcode`.
//!
//! [`CompiledEvaluationValue`] is a 4-bit satisfied-set with no third state.
//! [`CompiledVerdict`] adds a measured mask and enforces
//! `satisfied.bits() & !measured == 0`: you cannot be satisfied on a
//! generator you did not measure. PLATINUM therefore requires all four
//! measured *and* all four satisfied, with no extra rule.

use crate::core::omega::EvaluationValue;
use crate::evaluation::policies::compiled::compiled_omega::{
    CompiledEvaluationValue, CompiledGenerator,
};

#[derive(Debug, Clone, PartialEq)]
pub enum GeneratorOutcome {
    Satisfied {
        delta_pct: f64,
        p_value: Option<f64>,
        detail: String,
    },
    Violated {
        delta_pct: f64,
        p_value: Option<f64>,
        detail: String,
    },
    Unmeasured {
        reason: &'static str,
    },
}

impl GeneratorOutcome {
    pub fn is_measured(&self) -> bool {
        !matches!(self, GeneratorOutcome::Unmeasured { .. })
    }

    pub fn is_satisfied(&self) -> bool {
        matches!(self, GeneratorOutcome::Satisfied { .. })
    }
}

/// Verdict that knows what it did not measure. Private fields; the only
/// constructor is [`CompiledVerdict::from_outcomes`].
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledVerdict {
    satisfied: CompiledEvaluationValue,
    measured: u8,
    outcomes: [GeneratorOutcome; 4],
}

impl CompiledVerdict {
    /// `outcomes` is ordered SPEED, SIZE, ENERGY, LOCALITY.
    pub fn from_outcomes(outcomes: &[GeneratorOutcome; 4]) -> Self {
        let mut measured = 0u8;
        let mut satisfied_bits = 0u8;
        for (generator, outcome) in CompiledGenerator::ALL.iter().zip(outcomes.iter()) {
            match outcome {
                GeneratorOutcome::Satisfied { .. } => {
                    measured |= generator.bit();
                    satisfied_bits |= generator.bit();
                }
                GeneratorOutcome::Violated { .. } => {
                    measured |= generator.bit();
                }
                GeneratorOutcome::Unmeasured { .. } => {}
            }
        }
        debug_assert_eq!(
            satisfied_bits & !measured,
            0,
            "satisfied bits must be a subset of measured bits"
        );
        Self {
            satisfied: CompiledEvaluationValue::from_bits(satisfied_bits)
                .expect("a 4-bit mask is always a valid CompiledEvaluationValue"),
            measured,
            outcomes: outcomes.clone(),
        }
    }

    pub fn satisfied(&self) -> CompiledEvaluationValue {
        self.satisfied
    }

    pub fn measured_mask(&self) -> u8 {
        self.measured
    }

    pub fn outcomes(&self) -> &[GeneratorOutcome; 4] {
        &self.outcomes
    }

    pub fn outcome(&self, generator: CompiledGenerator) -> &GeneratorOutcome {
        match generator {
            CompiledGenerator::Speed => &self.outcomes[0],
            CompiledGenerator::Size => &self.outcomes[1],
            CompiledGenerator::Energy => &self.outcomes[2],
            CompiledGenerator::Locality => &self.outcomes[3],
        }
    }

    /// Delegates to [`EvaluationValue::medal_tier`] so renderers never
    /// re-derive popcount thresholds.
    pub fn medal_tier(&self) -> &'static str {
        EvaluationValue::from_bits(self.satisfied.bits())
            .expect("a 4-bit mask is always a valid EvaluationValue")
            .medal_tier()
    }

    /// Best tier this run could have produced (every measured generator
    /// treated as satisfied). ENERGY unmeasured caps the ceiling at GOLD.
    pub fn medal_ceiling(&self) -> &'static str {
        EvaluationValue::from_bits(self.measured)
            .expect("a 4-bit mask is always a valid EvaluationValue")
            .medal_tier()
    }

    pub fn ceiling_differs(&self) -> bool {
        self.medal_tier() != self.medal_ceiling()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn satisfied() -> GeneratorOutcome {
        GeneratorOutcome::Satisfied {
            delta_pct: 10.0,
            p_value: Some(0.01),
            detail: "ok".into(),
        }
    }

    fn violated() -> GeneratorOutcome {
        GeneratorOutcome::Violated {
            delta_pct: 1.0,
            p_value: Some(0.4),
            detail: "no".into(),
        }
    }

    fn unmeasured() -> GeneratorOutcome {
        GeneratorOutcome::Unmeasured { reason: "n/a" }
    }

    fn all_states() -> [GeneratorOutcome; 3] {
        [satisfied(), violated(), unmeasured()]
    }

    #[test]
    fn an_unmeasured_generator_can_never_be_satisfied() {
        for a in all_states() {
            for b in all_states() {
                for c in all_states() {
                    for d in all_states() {
                        let verdict = CompiledVerdict::from_outcomes(&[
                            a.clone(),
                            b.clone(),
                            c.clone(),
                            d.clone(),
                        ]);
                        assert_eq!(verdict.satisfied().bits() & !verdict.measured_mask(), 0);
                        for generator in CompiledGenerator::ALL {
                            if matches!(
                                verdict.outcome(generator),
                                GeneratorOutcome::Unmeasured { .. }
                            ) {
                                assert_eq!(verdict.satisfied().bits() & generator.bit(), 0);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn no_measurements_cannot_yield_platinum() {
        let verdict = CompiledVerdict::from_outcomes(&[
            unmeasured(),
            unmeasured(),
            unmeasured(),
            unmeasured(),
        ]);
        assert_ne!(verdict.medal_tier(), "PLATINUM");
        assert_eq!(verdict.medal_tier(), "SLOP");
        assert_eq!(verdict.medal_ceiling(), "SLOP");
    }

    #[test]
    fn platinum_requires_all_four_measured_and_satisfied() {
        let verdict =
            CompiledVerdict::from_outcomes(&[satisfied(), satisfied(), satisfied(), satisfied()]);
        assert_eq!(verdict.medal_tier(), "PLATINUM");
        assert_eq!(verdict.medal_ceiling(), "PLATINUM");
    }

    #[test]
    fn energy_unmeasured_caps_the_ceiling_at_gold() {
        let verdict =
            CompiledVerdict::from_outcomes(&[satisfied(), satisfied(), unmeasured(), satisfied()]);
        assert_eq!(verdict.medal_tier(), "GOLD");
        assert_eq!(verdict.medal_ceiling(), "GOLD");
        assert!(!verdict.ceiling_differs());
    }

    #[test]
    fn a_failed_measured_generator_lowers_the_medal_below_the_ceiling() {
        let verdict =
            CompiledVerdict::from_outcomes(&[satisfied(), violated(), unmeasured(), satisfied()]);
        assert_eq!(verdict.medal_tier(), "SILVER");
        assert_eq!(verdict.medal_ceiling(), "GOLD");
        assert!(verdict.ceiling_differs());
    }
}
