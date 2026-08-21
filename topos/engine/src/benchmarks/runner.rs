//! Compile-and-run benchmark driver.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::adapters::llvm::{LlvmError, LlvmToolchain};
use crate::adapters::perf::ProfileCollector;
use crate::adapters::{run_with_timeout, RunError};
use crate::graphs::base::Representation;
use crate::graphs::bitcode::parse_ll_assembly;

use super::baseline::compiled_medal_from_metrics;
use super::manifest::{BenchmarkDefaults, BenchmarkManifest, WorkloadSpec};
use super::result::{BenchmarkSuiteResult, WorkloadMeasurement};

#[derive(Debug)]
pub enum BenchmarkError {
    ToolchainUnavailable,
    Manifest(String),
    Compile(LlvmError),
    Run(String),
    Io(std::io::Error),
}

impl std::fmt::Display for BenchmarkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ToolchainUnavailable => write!(f, "clang not available on PATH"),
            Self::Manifest(msg) => write!(f, "manifest error: {msg}"),
            Self::Compile(err) => write!(f, "compile error: {err}"),
            Self::Run(msg) => write!(f, "run error: {msg}"),
            Self::Io(err) => write!(f, "I/O error: {err}"),
        }
    }
}

impl std::error::Error for BenchmarkError {}
impl From<std::io::Error> for BenchmarkError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

pub struct BenchmarkRunner {
    toolchain: LlvmToolchain,
    work_dir: PathBuf,
}

impl BenchmarkRunner {
    pub fn new(work_dir: impl AsRef<Path>) -> std::io::Result<Self> {
        let work_dir = work_dir.as_ref().to_path_buf();
        fs::create_dir_all(&work_dir)?;
        Ok(Self {
            toolchain: LlvmToolchain::new(),
            work_dir,
        })
    }

    pub fn toolchain_usable() -> bool {
        LlvmToolchain::detect().is_usable()
    }

    pub fn run_manifest(
        &self,
        manifest: &BenchmarkManifest,
    ) -> Result<BenchmarkSuiteResult, BenchmarkError> {
        if !Self::toolchain_usable() {
            return Err(BenchmarkError::ToolchainUnavailable);
        }
        let mut measurements = Vec::with_capacity(manifest.workload.len());
        for workload in &manifest.workload {
            measurements.push(self.run_workload(manifest, workload)?);
        }
        Ok(BenchmarkSuiteResult {
            manifest_path: manifest
                .manifest_path()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "manifest.toml".into()),
            toolchain_usable: true,
            measurements,
        })
    }

    pub fn run_workload(
        &self,
        manifest: &BenchmarkManifest,
        workload: &WorkloadSpec,
    ) -> Result<WorkloadMeasurement, BenchmarkError> {
        let defaults = &manifest.defaults;
        let source = manifest.resolve_source(workload);
        if !source.exists() {
            return Err(BenchmarkError::Manifest(format!(
                "workload `{}`: source not found at {}",
                workload.id,
                source.display()
            )));
        }

        let artifact_dir = self.work_dir.join(&workload.id);
        fs::create_dir_all(&artifact_dir)?;
        let bc_path = artifact_dir.join("module.bc");
        let ll_path = artifact_dir.join("module.ll");
        let bin_path = artifact_dir.join("workload");
        let flags = manifest.compile_flags_for(workload);

        self.toolchain
            .compile_to_bitcode(&source, &bc_path, &flags)
            .map_err(BenchmarkError::Compile)?;
        self.toolchain
            .disassemble_bitcode(&bc_path, &ll_path)
            .map_err(BenchmarkError::Compile)?;
        self.toolchain
            .compile_bitcode_to_binary(&bc_path, &bin_path, &[])
            .map_err(BenchmarkError::Compile)?;

        let ll_text = fs::read_to_string(&ll_path).map_err(BenchmarkError::Io)?;
        let bitcode_obj = parse_ll_assembly(&workload.id, &ll_text);
        let bitcode_metrics = bitcode_obj.metrics();
        let compiled_medal = compiled_medal_from_metrics(&bitcode_metrics);

        let (median_ms, min_ms, max_ms, exit_code) =
            self.timed_runs(&bin_path, &workload.args, defaults)?;
        let profile = ProfileCollector::collect_for_command(&bin_path, &workload.args, None);

        Ok(WorkloadMeasurement {
            workload_id: workload.id.clone(),
            description: workload.description.clone(),
            median_wall_ms: median_ms,
            min_wall_ms: min_ms,
            max_wall_ms: max_ms,
            binary_size_bytes: fs::metadata(&bin_path).map_err(BenchmarkError::Io)?.len(),
            bitcode_size_bytes: fs::metadata(&bc_path).map_err(BenchmarkError::Io)?.len(),
            instruction_count: bitcode_obj.total_instructions as u64,
            compiled_medal,
            bitcode_metrics,
            exit_code,
            measure_runs: defaults.measure_runs,
            profile: Some(profile),
        })
    }

    fn timed_runs(
        &self,
        binary: &Path,
        args: &[String],
        defaults: &BenchmarkDefaults,
    ) -> Result<(f64, f64, f64, Option<i32>), BenchmarkError> {
        let timeout = Duration::from_secs(defaults.timeout_secs);
        let total = defaults.warmup_runs + defaults.measure_runs;
        let mut samples = Vec::new();
        let mut last_status = None;
        for run_idx in 0..total {
            let mut cmd = Command::new(binary);
            for arg in args {
                cmd.arg(arg);
            }
            let start = Instant::now();
            let output = run_with_timeout(cmd, None, true, Some(timeout)).map_err(|e| match e {
                RunError::TimedOut => {
                    BenchmarkError::Run(format!("{} timed out", binary.display()))
                }
                RunError::Io(err) => BenchmarkError::Io(err),
            })?;
            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
            last_status = output.status_code;
            if output.status_code != Some(0) {
                return Err(BenchmarkError::Run(output.stderr));
            }
            if run_idx >= defaults.warmup_runs {
                samples.push(elapsed_ms);
            }
        }
        if samples.is_empty() {
            return Ok((0.0, 0.0, 0.0, last_status));
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        Ok((
            samples[samples.len() / 2],
            samples[0],
            samples[samples.len() - 1],
            last_status,
        ))
    }
}
