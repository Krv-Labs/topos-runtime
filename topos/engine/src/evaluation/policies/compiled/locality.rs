//! LOCALITY, rendered as **MEMORY FOOTPRINT**: peak RSS + page faults vs the
//! baseline arm of the same interleaved run.
//!
//! `Satisfied` iff `rss_increase_pct ≤ max` and the page-fault delta is not
//! significantly worse. Keep [`CompiledGenerator::Locality`] so
//! `compiled_omega.rs` is untouched.

use crate::evaluation::policies::compiled::outcome::GeneratorOutcome;

pub const RSS_UNMEASURED_REASON: &str = "peak RSS not available from /usr/bin/time";

/// `fault_p` is the two-sided p-value of the page-fault paired delta, when
/// observed. A significant *increase* (`delta > 0` and `p ≤ 0.05`) fails
/// the generator even if RSS stayed in budget.
pub fn score_locality(
    baseline_rss: Option<u64>,
    variant_rss: Option<u64>,
    max_rss_increase_pct: f64,
    fault_increase_pct: Option<f64>,
    fault_p: Option<f64>,
) -> GeneratorOutcome {
    let (Some(baseline), Some(variant)) = (baseline_rss, variant_rss) else {
        return GeneratorOutcome::Unmeasured {
            reason: RSS_UNMEASURED_REASON,
        };
    };
    if baseline == 0 {
        return GeneratorOutcome::Unmeasured {
            reason: "baseline peak RSS is zero; MEMORY FOOTPRINT delta is undefined",
        };
    }
    let delta_pct = (variant as f64 - baseline as f64) / baseline as f64 * 100.0;
    let faults_worse = matches!(
        (fault_increase_pct, fault_p),
        (Some(delta), Some(p)) if delta > 0.0 && p <= 0.05
    );
    let detail = match fault_increase_pct {
        Some(fd) => format!(
            "RSS {delta_pct:+.2}% (budget {max_rss_increase_pct:.1}%), page faults {fd:+.2}%"
        ),
        None => format!("RSS {delta_pct:+.2}% (budget {max_rss_increase_pct:.1}%)"),
    };
    if delta_pct <= max_rss_increase_pct && !faults_worse {
        GeneratorOutcome::Satisfied {
            delta_pct,
            p_value: fault_p,
            detail,
        }
    } else {
        GeneratorOutcome::Violated {
            delta_pct,
            p_value: fault_p,
            detail,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_rss_is_unmeasured() {
        let outcome = score_locality(None, None, 10.0, None, None);
        assert!(matches!(
            outcome,
            GeneratorOutcome::Unmeasured {
                reason: RSS_UNMEASURED_REASON
            }
        ));
    }

    #[test]
    fn rss_in_budget_without_worse_faults_is_satisfied() {
        let outcome = score_locality(Some(1000), Some(1050), 10.0, Some(1.0), Some(0.8));
        assert!(outcome.is_satisfied());
    }

    #[test]
    fn significantly_worse_page_faults_violate_even_when_rss_is_in_budget() {
        let outcome = score_locality(Some(1000), Some(1000), 10.0, Some(50.0), Some(0.01));
        assert!(matches!(outcome, GeneratorOutcome::Violated { .. }));
    }
}
