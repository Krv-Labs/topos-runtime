//! Serializable compiled-optimizer report. No rendering.

use serde::{Deserialize, Serialize};

use crate::evaluation::policies::compiled::outcome::{CompiledVerdict, GeneratorOutcome};
use crate::optimization::statistics::NoiseVerdict;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptimizationReport {
    pub run_id: String,
    pub plan_digest: String,
    pub baseline_variant: String,
    pub baseline_bytes: u64,
    pub candidates: Vec<CandidateReport>,
    pub winner: Option<String>,
    pub medal: String,
    pub medal_ceiling: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateReport {
    pub id: String,
    pub variant: String,
    pub speedup_pct: f64,
    pub p_value: Option<f64>,
    pub noise: String,
    pub size_increase_pct: f64,
    pub binary_size_bytes: u64,
    pub speed_satisfied: bool,
    pub size_satisfied: bool,
}

impl OptimizationReport {
    pub fn from_parts(
        run_id: String,
        plan_digest: String,
        baseline_variant: String,
        baseline_bytes: u64,
        candidates: Vec<CandidateReport>,
        verdict: &CompiledVerdict,
    ) -> Self {
        let winner = candidates
            .iter()
            .filter(|c| c.speed_satisfied && c.size_satisfied)
            .max_by(|a, b| {
                a.speedup_pct
                    .partial_cmp(&b.speedup_pct)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|c| c.id.clone());
        Self {
            run_id,
            plan_digest,
            baseline_variant,
            baseline_bytes,
            candidates,
            winner,
            medal: verdict.medal_tier().to_string(),
            medal_ceiling: verdict.medal_ceiling().to_string(),
        }
    }
}

pub fn noise_label(verdict: &NoiseVerdict) -> String {
    match verdict {
        NoiseVerdict::Real { .. } => "real".into(),
        NoiseVerdict::RealButBelowThreshold { .. } => "real_but_below_threshold".into(),
        NoiseVerdict::WithinNoise { .. } => "within_noise".into(),
        NoiseVerdict::NoEffect { .. } => "no_effect".into(),
        NoiseVerdict::InsufficientSamples { .. } => "insufficient_samples".into(),
        NoiseVerdict::WorkloadTooShort { .. } => "workload_too_short".into(),
        NoiseVerdict::UnstableEnvironment { .. } => "unstable_environment".into(),
    }
}

pub fn speedup_pct(verdict: &NoiseVerdict) -> f64 {
    match verdict {
        NoiseVerdict::Real { speedup_pct, .. }
        | NoiseVerdict::RealButBelowThreshold { speedup_pct, .. }
        | NoiseVerdict::WithinNoise { speedup_pct, .. }
        | NoiseVerdict::NoEffect { speedup_pct, .. } => *speedup_pct,
        _ => 0.0,
    }
}

pub fn p_value(verdict: &NoiseVerdict) -> Option<f64> {
    match verdict {
        NoiseVerdict::Real { p_value, .. }
        | NoiseVerdict::RealButBelowThreshold { p_value, .. }
        | NoiseVerdict::WithinNoise { p_value, .. }
        | NoiseVerdict::NoEffect { p_value, .. } => Some(*p_value),
        _ => None,
    }
}

pub fn is_satisfied(outcome: &GeneratorOutcome) -> bool {
    outcome.is_satisfied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluation::policies::compiled::energy;
    use crate::evaluation::policies::compiled::locality::score_locality;
    use crate::evaluation::policies::compiled::size::score_size;
    use crate::evaluation::policies::compiled::speed::score_speed;
    use crate::evaluation::policies::compiled::CompiledVerdict;
    use crate::optimization::measurement::{PairedRound, Sample};
    use crate::optimization::statistics::evaluate_pair;

    fn sample(wall_ms: f64) -> Sample {
        Sample {
            wall_ms,
            max_rss_bytes: Some(1000),
            page_faults: Some(0),
        }
    }

    #[test]
    fn reported_speedup_is_computed_from_samples_not_the_threshold() {
        let min_speedup_pct = 20.0;
        // Jittered, not monotone — a climbing baseline would trip the
        // environment self-check and report speedup 0.
        let baseline = [80.0, 80.4, 79.7, 80.2, 79.9, 80.3, 79.8, 80.1, 80.0, 79.6];
        let variant: Vec<f64> = baseline.iter().map(|b| b * 0.98).collect();
        let rounds: Vec<PairedRound> = baseline
            .iter()
            .zip(&variant)
            .map(|(b, v)| PairedRound {
                baseline: sample(*b),
                variant: sample(*v),
            })
            .collect();
        let noise = evaluate_pair(&rounds, min_speedup_pct, 1);
        let reported = speedup_pct(&noise);
        assert!(
            (reported - 2.0).abs() < 0.1,
            "reported {reported}, expected ~2.0 not {min_speedup_pct}"
        );
        assert_ne!(reported, min_speedup_pct);
        let speed = score_speed(&noise, min_speedup_pct);
        assert!(
            !speed.is_satisfied(),
            "2% must not satisfy a 20% threshold: {speed:?}"
        );
        let size = score_size(1000, 1000, 10.0);
        let verdict = CompiledVerdict::from_outcomes(&[
            speed,
            size,
            energy::score_energy(),
            score_locality(Some(1000), Some(1000), 10.0, Some(0.0), Some(1.0)),
        ]);
        let report = OptimizationReport::from_parts(
            "run".into(),
            "digest".into(),
            "O0".into(),
            1000,
            vec![CandidateReport {
                id: "c01".into(),
                variant: "O2".into(),
                speedup_pct: reported,
                p_value: p_value(&noise),
                noise: noise_label(&noise),
                size_increase_pct: 0.0,
                binary_size_bytes: 1000,
                speed_satisfied: false,
                size_satisfied: true,
            }],
            &verdict,
        );
        assert!((report.candidates[0].speedup_pct - 2.0).abs() < 0.1);
        assert!(!report.candidates[0].speed_satisfied);
        assert!(report.winner.is_none());
    }
}
