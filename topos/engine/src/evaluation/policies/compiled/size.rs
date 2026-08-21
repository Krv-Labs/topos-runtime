//! SIZE: deterministic binary-size delta vs the baseline arm.
//!
//! SIZE-satisfied means "did not regress beyond budget"
//! (`size_increase_pct ≤ max`). SPEED-satisfied means "improved by ≥
//! threshold". This mirrors the spec's own `speedup ≥ 5% / size increase ≤ 10%`.
//! `p_value` is always `None`.

use crate::evaluation::policies::compiled::outcome::GeneratorOutcome;

pub fn score_size(
    baseline_bytes: u64,
    variant_bytes: u64,
    max_increase_pct: f64,
) -> GeneratorOutcome {
    if baseline_bytes == 0 {
        return GeneratorOutcome::Unmeasured {
            reason: "baseline binary size is zero; SIZE delta is undefined",
        };
    }
    let delta_pct = (variant_bytes as f64 - baseline_bytes as f64) / baseline_bytes as f64 * 100.0;
    let detail = format!(
        "size {variant_bytes} B vs baseline {baseline_bytes} B ({delta_pct:+.2}%, budget {max_increase_pct:.1}%)"
    );
    if delta_pct <= max_increase_pct {
        GeneratorOutcome::Satisfied {
            delta_pct,
            p_value: None,
            detail,
        }
    } else {
        GeneratorOutcome::Violated {
            delta_pct,
            p_value: None,
            detail,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ten_percent_growth_is_satisfied_at_the_default_budget() {
        let outcome = score_size(1000, 1100, 10.0);
        assert!(outcome.is_satisfied());
    }

    #[test]
    fn growth_past_the_budget_is_violated() {
        let outcome = score_size(1000, 1200, 10.0);
        assert!(matches!(outcome, GeneratorOutcome::Violated { .. }));
    }
}
