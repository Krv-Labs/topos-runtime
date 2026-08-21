//! ENERGY is permanently unmeasured.
//!
//! No portable energy counter is available: Linux RAPL needs `perf`, and
//! macOS `powermetrics` needs root. The generator stays in the 4-cube so
//! `compiled_omega.rs` is untouched; the medal *ceiling* discloses the hole
//! rather than hiding it.

use crate::evaluation::policies::compiled::outcome::GeneratorOutcome;

pub const UNMEASURED_REASON: &str =
    "no energy counter available: needs Linux RAPL via perf, or root powermetrics on macOS";

pub fn score_energy() -> GeneratorOutcome {
    GeneratorOutcome::Unmeasured {
        reason: UNMEASURED_REASON,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn energy_is_always_unmeasured_on_every_platform() {
        match score_energy() {
            GeneratorOutcome::Unmeasured { reason } => {
                assert_eq!(reason, UNMEASURED_REASON);
            }
            other => panic!("expected Unmeasured, got {other:?}"),
        }
    }
}
