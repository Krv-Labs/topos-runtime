//! Optimization loop for compiled binaries.
//!
//! Driver flags and instrumented PGO only. `opt` pass names are not part of
//! this surface — feeding them to clang as argv is how the fabricated loop
//! produced unbuildable plans.

pub mod approval;
pub mod artifact_store;
pub mod harness;
pub mod measurement;
pub mod ops;
pub mod plan;
pub mod report;
pub mod statistics;
pub mod target;
pub mod variant;
