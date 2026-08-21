//! Measured samples from one compiled-binary run.
//!
//! Every optional signal is `Option`. There is no defaulted zero that could
//! be mistaken for a measurement.

#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub wall_ms: f64,
    pub max_rss_bytes: Option<u64>,
    pub page_faults: Option<u64>,
}

impl Sample {
    pub fn from_timed(run: crate::adapters::timing::TimedRun) -> Self {
        Self {
            wall_ms: run.wall_ms,
            max_rss_bytes: run.max_rss_bytes,
            page_faults: run.page_faults,
        }
    }
}

/// One interleaved round: both arms, same environment.
#[derive(Debug, Clone, PartialEq)]
pub struct PairedRound {
    pub baseline: Sample,
    pub variant: Sample,
}

impl PairedRound {
    pub fn wall_diff_ms(&self) -> f64 {
        self.variant.wall_ms - self.baseline.wall_ms
    }
}
