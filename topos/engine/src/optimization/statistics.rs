//! Interleaved paired sampling and an exact sign-flip permutation test.
//!
//! # Why this, and not a t-test
//!
//! Wall-clock times are not normal. Welch's t needs a t-CDF (new tables or a
//! dependency). Mann–Whitney discards the pairing. Bootstrap needs an RNG and
//! a seed policy. A plain sign test is exact but discards magnitude. The
//! sign-flip permutation test enumerates all `2^n` assignments of the variant
//! label under "the label is irrelevant", counts how many give `|sum|` at
//! least as large as observed, and needs no RNG, no tables, no new crate.
//! The statistic is the sum (equivalently the mean), not the median: the
//! median of an even sample is the average of two middles and is nearly
//! powerless as a permutation statistic, and any equal-magnitude series of
//! odd length has `|median|` invariant under every sign flip (p = 1).
//! Speedup is still *reported* from medians.
//!
//! `MAX_EXACT_N = 20`. With `n` pairs the smallest two-sided p is `2^(1-n)`.
//! At α = 0.05, fewer than 6 rounds can never be significant regardless of
//! effect size.
//!
//! # Known ceiling
//!
//! Code/stack alignment and link order shift runtime several percent
//! independent of the flag under test (Mytkowicz et al., *"Producing Wrong
//! Data Without Doing Anything Obviously Wrong"*). A REAL verdict means the
//! difference is real, not that it is attributable to the flag. Mitigation
//! taken elsewhere: same-length output paths, identical cwd and environment.
//! Randomized link order (Stabilizer-style) is an explicit non-goal.

use crate::optimization::measurement::PairedRound;

pub const MAX_EXACT_N: usize = 20;
pub const MIN_ROUNDS: usize = 6;
pub const MIN_WORKLOAD_MS: f64 = 50.0;
pub const DEFAULT_ALPHA: f64 = 0.05;

/// Two-gate comparison of a variant against the baseline arm of the same
/// interleaved run.
#[derive(Debug, Clone, PartialEq)]
pub enum NoiseVerdict {
    Real {
        speedup_pct: f64,
        p_value: f64,
        alpha_corrected: f64,
    },
    RealButBelowThreshold {
        speedup_pct: f64,
        p_value: f64,
        alpha_corrected: f64,
    },
    WithinNoise {
        speedup_pct: f64,
        p_value: f64,
    },
    NoEffect {
        speedup_pct: f64,
        p_value: f64,
    },
    InsufficientSamples {
        n: usize,
        min_achievable_p: f64,
    },
    WorkloadTooShort {
        median_ms: f64,
        floor_ms: f64,
    },
    UnstableEnvironment {
        drift_p: f64,
    },
}

/// Smallest two-sided p achievable with `n` pairs: `2^(1-n)`.
pub fn min_achievable_p(n: usize) -> f64 {
    if n == 0 {
        return 1.0;
    }
    2.0_f64.powi(1 - n as i32)
}

/// Median. Even `n` is the mean of the two middle elements.
pub fn median(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut v = samples.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

fn abs_sum(diffs: &[f64]) -> f64 {
    diffs.iter().sum::<f64>().abs()
}

/// Two-sided exact sign-flip permutation p-value of `sum(diffs)`.
///
/// `diffs[i] = variant_i - baseline_i`. `n` must be `1..=MAX_EXACT_N`.
pub fn permutation_p_value(diffs: &[f64]) -> f64 {
    let n = diffs.len();
    if n == 0 || n > MAX_EXACT_N {
        return 1.0;
    }
    let observed = abs_sum(diffs);
    let total = 1u32 << n;
    let mut count = 0u32;
    let mut signed = vec![0.0; n];
    for mask in 0..total {
        for (i, diff) in diffs.iter().enumerate() {
            signed[i] = if (mask >> i) & 1 == 1 { -diff } else { *diff };
        }
        if abs_sum(&signed) >= observed {
            count += 1;
        }
    }
    f64::from(count) / f64::from(total)
}

/// Bonferroni: α divided by the number of variant comparisons in the run.
pub fn alpha_corrected(alpha: f64, comparison_count: usize) -> f64 {
    let n = comparison_count.max(1) as f64;
    alpha / n
}

/// Environment self-check: consecutive baseline samples (one interleaved
/// round apart) tested against themselves. A significant lag-1 difference
/// means the machine drifted mid-run and every comparison is suspect.
///
/// Parity-split even-vs-odd is the same pairing without overlap and yields
/// only `n/2` diffs — at the default `n=10` that can never be significant
/// at α=0.05. Lag-1 keeps `n-1` diffs so the check can actually fire.
pub fn drift_p_value(baseline_by_round: &[f64]) -> Option<f64> {
    if baseline_by_round.len() < 2 {
        return None;
    }
    let diffs: Vec<f64> = baseline_by_round.windows(2).map(|w| w[1] - w[0]).collect();
    Some(permutation_p_value(&diffs))
}

/// Compare interleaved paired rounds. `comparison_count` is the Bonferroni
/// divisor (number of variants tested against the same baseline).
pub fn evaluate_pair(
    rounds: &[PairedRound],
    min_speedup_pct: f64,
    comparison_count: usize,
) -> NoiseVerdict {
    let n = rounds.len();
    if !(MIN_ROUNDS..=MAX_EXACT_N).contains(&n) {
        return NoiseVerdict::InsufficientSamples {
            n,
            min_achievable_p: min_achievable_p(n),
        };
    }

    let baseline: Vec<f64> = rounds.iter().map(|r| r.baseline.wall_ms).collect();
    let variant: Vec<f64> = rounds.iter().map(|r| r.variant.wall_ms).collect();
    let all_wall: Vec<f64> = baseline.iter().chain(variant.iter()).copied().collect();
    let workload_median = median(&all_wall);
    if workload_median < MIN_WORKLOAD_MS {
        return NoiseVerdict::WorkloadTooShort {
            median_ms: workload_median,
            floor_ms: MIN_WORKLOAD_MS,
        };
    }

    if let Some(drift_p) = drift_p_value(&baseline) {
        if drift_p <= DEFAULT_ALPHA {
            return NoiseVerdict::UnstableEnvironment { drift_p };
        }
    }

    let diffs: Vec<f64> = rounds.iter().map(PairedRound::wall_diff_ms).collect();
    let p_value = permutation_p_value(&diffs);
    let alpha_corrected = alpha_corrected(DEFAULT_ALPHA, comparison_count);
    let median_baseline = median(&baseline);
    let speedup_pct = if median_baseline == 0.0 {
        0.0
    } else {
        (median_baseline - median(&variant)) / median_baseline * 100.0
    };
    let significant = p_value <= alpha_corrected;
    let effect = speedup_pct >= min_speedup_pct;
    match (significant, effect) {
        (true, true) => NoiseVerdict::Real {
            speedup_pct,
            p_value,
            alpha_corrected,
        },
        (true, false) => NoiseVerdict::RealButBelowThreshold {
            speedup_pct,
            p_value,
            alpha_corrected,
        },
        (false, true) => NoiseVerdict::WithinNoise {
            speedup_pct,
            p_value,
        },
        (false, false) => NoiseVerdict::NoEffect {
            speedup_pct,
            p_value,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optimization::measurement::Sample;

    fn sample(wall_ms: f64) -> Sample {
        Sample {
            wall_ms,
            max_rss_bytes: None,
            page_faults: None,
        }
    }

    fn rounds_from(baseline: &[f64], variant: &[f64]) -> Vec<PairedRound> {
        baseline
            .iter()
            .zip(variant)
            .map(|(b, v)| PairedRound {
                baseline: sample(*b),
                variant: sample(*v),
            })
            .collect()
    }

    #[test]
    fn median_of_an_even_sample_is_the_average_of_the_two_middles() {
        assert_eq!(median(&[1.0, 3.0, 2.0, 4.0]), 2.5);
        assert_eq!(median(&[10.0, 20.0]), 15.0);
        assert_eq!(median(&[7.0]), 7.0);
    }

    #[test]
    fn permutation_p_value_matches_hand_enumeration_at_n_equals_4() {
        let diffs = [1.0, 2.0, 3.0, 4.0];
        let observed = diffs.iter().sum::<f64>().abs();
        let mut count = 0;
        for mask in 0..16u32 {
            let signed: Vec<f64> = diffs
                .iter()
                .enumerate()
                .map(|(i, d)| if (mask >> i) & 1 == 1 { -d } else { *d })
                .collect();
            if signed.iter().sum::<f64>().abs() >= observed {
                count += 1;
            }
        }
        let expected = f64::from(count) / 16.0;
        assert_eq!(permutation_p_value(&diffs), expected);
    }

    #[test]
    fn five_rounds_can_never_be_significant() {
        assert_eq!(min_achievable_p(5), 0.0625);
        let baseline = [80.0, 81.0, 79.0, 80.5, 80.2];
        let variant = [1.0, 1.0, 1.0, 1.0, 1.0];
        let verdict = evaluate_pair(&rounds_from(&baseline, &variant), 5.0, 1);
        match verdict {
            NoiseVerdict::InsufficientSamples {
                n,
                min_achievable_p,
            } => {
                assert_eq!(n, 5);
                assert_eq!(min_achievable_p, 0.0625);
            }
            other => panic!("expected InsufficientSamples, got {other:?}"),
        }
    }

    #[test]
    fn identical_distributions_are_not_a_win() {
        let xs = [80.0, 82.0, 79.0, 81.0, 80.5, 81.5, 79.5, 80.2];
        let verdict = evaluate_pair(&rounds_from(&xs, &xs), 5.0, 1);
        assert!(
            matches!(
                verdict,
                NoiseVerdict::NoEffect { .. } | NoiseVerdict::WithinNoise { .. }
            ),
            "{verdict:?}"
        );
        if let NoiseVerdict::NoEffect { p_value, .. } | NoiseVerdict::WithinNoise { p_value, .. } =
            verdict
        {
            assert!(p_value > 0.05, "p={p_value}");
        }
    }

    #[test]
    fn a_clean_separation_is_significant() {
        let baseline = [100.0, 102.0, 101.0, 99.0, 100.5, 101.5, 100.2, 99.8];
        let variant = [50.0, 51.0, 49.5, 50.5, 50.2, 49.8, 50.1, 50.0];
        let verdict = evaluate_pair(&rounds_from(&baseline, &variant), 5.0, 1);
        match verdict {
            NoiseVerdict::Real {
                speedup_pct,
                p_value,
                alpha_corrected,
            } => {
                assert!(speedup_pct > 40.0, "speedup={speedup_pct}");
                assert!(
                    p_value <= alpha_corrected,
                    "p={p_value} α'={alpha_corrected}"
                );
            }
            other => panic!("expected Real, got {other:?}"),
        }
    }

    #[test]
    fn a_monotone_drift_across_rounds_is_flagged_unstable() {
        // Baseline itself climbs every round; lag-1 diffs are a clean gap.
        let baseline: Vec<f64> = (0..8).map(|i| 80.0 + (i as f64) * 20.0).collect();
        let variant: Vec<f64> = baseline.iter().map(|b| b - 1.0).collect();
        let verdict = evaluate_pair(&rounds_from(&baseline, &variant), 5.0, 1);
        assert!(
            matches!(verdict, NoiseVerdict::UnstableEnvironment { .. }),
            "{verdict:?}"
        );
    }

    #[test]
    fn bonferroni_divides_alpha_by_the_comparison_count() {
        assert_eq!(alpha_corrected(0.05, 4), 0.0125);
        assert_eq!(alpha_corrected(0.05, 1), 0.05);
        let baseline = [100.0, 102.0, 101.0, 99.0, 100.5, 101.5, 100.2, 99.8];
        let variant = [50.0, 51.0, 49.5, 50.5, 50.2, 49.8, 50.1, 50.0];
        match evaluate_pair(&rounds_from(&baseline, &variant), 5.0, 4) {
            NoiseVerdict::Real {
                alpha_corrected, ..
            } => {
                assert_eq!(alpha_corrected, 0.0125);
            }
            other => panic!("expected Real, got {other:?}"),
        }
    }
}
