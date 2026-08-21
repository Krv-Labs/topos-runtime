use std::path::PathBuf;
use topos_engine::benchmarks::{BenchmarkManifest, BenchmarkRunner};

#[test]
fn benchmark_workloads_end_to_end() {
    if std::env::var("TOPOS_BENCHMARK").ok().as_deref() != Some("1") {
        return;
    }
    if !BenchmarkRunner::toolchain_usable() {
        return;
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/manifest.toml");
    let manifest = BenchmarkManifest::load(&path).expect("manifest");
    let temp = std::env::temp_dir().join(format!("topos_bench_{}", std::process::id()));
    let runner = BenchmarkRunner::new(&temp).expect("runner");
    let result = runner.run_manifest(&manifest).expect("suite");
    assert_eq!(result.measurements.len(), 3);
    let _ = std::fs::remove_dir_all(temp);
}
