//! SPEED: measured wall-clock delta vs the baseline arm of the same run.
//!
//! `Satisfied` if and only if the noise verdict is [`NoiseVerdict::Real`]
//! *and* `speedup_pct >= min_speedup_pct`. A point estimate that clears the
//! bar with `WithinNoise` is `Violated` — never `Satisfied`.

use crate::evaluation::policies::compiled::outcome::GeneratorOutcome;
use crate::optimization::statistics::NoiseVerdict;

pub fn score_speed(verdict: &NoiseVerdict, min_speedup_pct: f64) -> GeneratorOutcome {
    match verdict {
        NoiseVerdict::Real {
            speedup_pct,
            p_value,
            alpha_corrected,
        } if *speedup_pct >= min_speedup_pct => GeneratorOutcome::Satisfied {
            delta_pct: *speedup_pct,
            p_value: Some(*p_value),
            detail: format!(
                "speedup {speedup_pct:.2}% (p={p_value:.4} ≤ α'={alpha_corrected:.4})"
            ),
        },
        NoiseVerdict::Real {
            speedup_pct,
            p_value,
            alpha_corrected,
        } => GeneratorOutcome::Violated {
            delta_pct: *speedup_pct,
            p_value: Some(*p_value),
            detail: format!(
                "measurable {speedup_pct:.2}% but below {min_speedup_pct}% (p={p_value:.4} ≤ α'={alpha_corrected:.4})"
            ),
        },
        NoiseVerdict::RealButBelowThreshold {
            speedup_pct,
            p_value,
            alpha_corrected,
        } => GeneratorOutcome::Violated {
            delta_pct: *speedup_pct,
            p_value: Some(*p_value),
            detail: format!(
                "measurable {speedup_pct:.2}% but below {min_speedup_pct}% (p={p_value:.4} ≤ α'={alpha_corrected:.4})"
            ),
        },
        NoiseVerdict::WithinNoise {
            speedup_pct,
            p_value,
        } => GeneratorOutcome::Violated {
            delta_pct: *speedup_pct,
            p_value: Some(*p_value),
            detail: format!("within noise (p={p_value:.2} > threshold)"),
        },
        NoiseVerdict::NoEffect {
            speedup_pct,
            p_value,
        } => GeneratorOutcome::Violated {
            delta_pct: *speedup_pct,
            p_value: Some(*p_value),
            detail: format!("no effect (p={p_value:.2})"),
        },
        NoiseVerdict::InsufficientSamples { .. } => GeneratorOutcome::Unmeasured {
            reason: "insufficient samples for a significant SPEED comparison",
        },
        NoiseVerdict::WorkloadTooShort { .. } => GeneratorOutcome::Unmeasured {
            reason: "workload shorter than 50ms; spawn overhead swamps SPEED",
        },
        NoiseVerdict::UnstableEnvironment { .. } => GeneratorOutcome::Unmeasured {
            reason: "unstable environment; SPEED comparison is poisoned",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn within_noise_is_violated_never_satisfied() {
        let outcome = score_speed(
            &NoiseVerdict::WithinNoise {
                speedup_pct: 20.0,
                p_value: 0.31,
            },
            5.0,
        );
        match outcome {
            GeneratorOutcome::Violated { detail, .. } => {
                assert!(detail.contains("within noise"), "{detail}");
                assert!(detail.contains("0.31"), "{detail}");
            }
            other => panic!("expected Violated, got {other:?}"),
        }
    }

    #[test]
    fn real_above_threshold_is_satisfied() {
        let outcome = score_speed(
            &NoiseVerdict::Real {
                speedup_pct: 8.0,
                p_value: 0.01,
                alpha_corrected: 0.05,
            },
            5.0,
        );
        assert!(outcome.is_satisfied());
    }
}
