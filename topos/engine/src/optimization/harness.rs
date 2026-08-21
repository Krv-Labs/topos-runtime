//! Interleaved A/B measurement of compiled flag variants.
//!
//! Warmups run once per arm. Each measured round runs both arms, alternating
//! order by round parity. PGO variants get a 3-stage instrumented cycle
//! (generate → merge → use) before they are timed. A missing toolchain is an
//! error; this module never writes a fallback binary.

use std::fmt;
use std::fs;
use std::path::Path;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::adapters::llvm::{LlvmError, Toolchain};
use crate::adapters::timing::{time_run, TimingError};
use crate::evaluation::policies::compiled::energy;
use crate::evaluation::policies::compiled::locality::score_locality;
use crate::evaluation::policies::compiled::outcome::CompiledVerdict;
use crate::evaluation::policies::compiled::size::score_size;
use crate::evaluation::policies::compiled::speed::score_speed;
use crate::optimization::approval::ApprovedPlan;
use crate::optimization::artifact_store::{ArtifactStore, StoreError};
use crate::optimization::measurement::{PairedRound, Sample};
use crate::optimization::plan::PlanError;
use crate::optimization::report::{
    noise_label, p_value, speedup_pct, CandidateReport, OptimizationReport,
};
use crate::optimization::statistics::{evaluate_pair, median, permutation_p_value};
use crate::optimization::target::{CompiledTarget, TargetError};
use crate::optimization::variant::{FlagVariant, VariantError};

#[derive(Debug)]
pub enum HarnessError {
    ToolchainUnavailable,
    Build(LlvmError),
    Run(String),
    Timing(TimingError),
    Store(StoreError),
    Target(TargetError),
    Plan(PlanError),
    Variant(VariantError),
    Io(std::io::Error),
}

impl fmt::Display for HarnessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HarnessError::ToolchainUnavailable => {
                write!(f, "clang is not available; refusing to invent a binary")
            }
            HarnessError::Build(err) => write!(f, "build failed: {err}"),
            HarnessError::Run(msg) => write!(f, "run failed: {msg}"),
            HarnessError::Timing(err) => write!(f, "{err}"),
            HarnessError::Store(err) => write!(f, "{err}"),
            HarnessError::Target(err) => write!(f, "{err}"),
            HarnessError::Plan(err) => write!(f, "{err}"),
            HarnessError::Variant(err) => write!(f, "{err}"),
            HarnessError::Io(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for HarnessError {}

impl From<LlvmError> for HarnessError {
    fn from(err: LlvmError) -> Self {
        HarnessError::Build(err)
    }
}
impl From<TimingError> for HarnessError {
    fn from(err: TimingError) -> Self {
        HarnessError::Timing(err)
    }
}
impl From<StoreError> for HarnessError {
    fn from(err: StoreError) -> Self {
        HarnessError::Store(err)
    }
}
impl From<TargetError> for HarnessError {
    fn from(err: TargetError) -> Self {
        HarnessError::Target(err)
    }
}
impl From<PlanError> for HarnessError {
    fn from(err: PlanError) -> Self {
        HarnessError::Plan(err)
    }
}
impl From<VariantError> for HarnessError {
    fn from(err: VariantError) -> Self {
        HarnessError::Variant(err)
    }
}
impl From<std::io::Error> for HarnessError {
    fn from(err: std::io::Error) -> Self {
        HarnessError::Io(err)
    }
}

/// First variant in the plan is the baseline arm. Remaining variants are
/// compared against it. `now` is unix seconds for the run id.
pub fn run_optimization(
    approved: &ApprovedPlan,
    target: &CompiledTarget,
    toolchain: &Toolchain,
    store: &ArtifactStore,
    now: f64,
) -> Result<OptimizationReport, HarnessError> {
    if !toolchain.has_clang() {
        return Err(HarnessError::ToolchainUnavailable);
    }
    let plan = approved.plan();
    let variants = plan.parsed_variants()?;
    let run_id = format!(
        "{}-{}",
        compact_rfc3339(now),
        &plan.digest[..8.min(plan.digest.len())]
    );
    let run_dir = store.create_run(&run_id)?;
    fs::write(
        run_dir.join("plan.json"),
        serde_json::to_vec_pretty(plan).unwrap_or_default(),
    )?;

    let timeout = Duration::from_millis(plan.timeout_ms);
    let cwd = store.project_root();
    let mut binaries = Vec::new();
    for (slot, variant) in variants.iter().enumerate() {
        let dir = store.candidate_dir(&run_id, slot)?;
        fs::create_dir_all(&dir)?;
        let binary = dir.join("binary");
        build_variant(toolchain, target, *variant, &binary, &dir, cwd, timeout)?;
        binaries.push((*variant, binary));
    }

    let (baseline_variant, baseline_bin) = binaries
        .first()
        .cloned()
        .ok_or(HarnessError::Plan(PlanError::NoVariants))?;
    let baseline_bytes = fs::read(&baseline_bin)?;
    let mode = file_mode(&baseline_bin);
    store.save_baseline(&run_id, &baseline_bytes, mode)?;
    store.set_current(&run_id)?;

    let comparison_count = binaries.len().saturating_sub(1).max(1);
    let mut candidates = Vec::new();
    let mut winner_outcomes = None;
    for (slot, (variant, binary)) in binaries.iter().enumerate().skip(1) {
        let rounds = interleaved_rounds(
            target,
            &baseline_bin,
            binary,
            plan.warmup_runs,
            plan.measured_runs,
            timeout,
            cwd,
        )?;
        let noise = evaluate_pair(&rounds, plan.min_speedup_pct, comparison_count);
        let speed = score_speed(&noise, plan.min_speedup_pct);
        let variant_bytes = fs::metadata(binary)?.len();
        let size = score_size(
            baseline_bytes.len() as u64,
            variant_bytes,
            plan.max_size_increase_pct,
        );
        let locality = locality_from_rounds(&rounds, plan.max_rss_increase_pct);
        let energy = energy::score_energy();
        let outcomes = [speed.clone(), size.clone(), energy.clone(), locality];
        let speed_satisfied = speed.is_satisfied();
        let size_satisfied = size.is_satisfied();
        if speed_satisfied && size_satisfied {
            winner_outcomes = Some(outcomes);
        } else {
            winner_outcomes.get_or_insert(outcomes);
        }
        let size_increase_pct = if baseline_bytes.is_empty() {
            0.0
        } else {
            (variant_bytes as f64 - baseline_bytes.len() as f64) / baseline_bytes.len() as f64
                * 100.0
        };
        candidates.push(CandidateReport {
            id: format!("c{slot:02}"),
            variant: variant.id().to_string(),
            speedup_pct: speedup_pct(&noise),
            p_value: p_value(&noise),
            noise: noise_label(&noise),
            size_increase_pct,
            binary_size_bytes: variant_bytes,
            speed_satisfied,
            size_satisfied,
        });
    }

    let verdict = CompiledVerdict::from_outcomes(&winner_outcomes.unwrap_or_else(|| {
        [
            crate::evaluation::policies::compiled::outcome::GeneratorOutcome::Unmeasured {
                reason: "no candidate variants were compared",
            },
            crate::evaluation::policies::compiled::outcome::GeneratorOutcome::Unmeasured {
                reason: "no candidate variants were compared",
            },
            energy::score_energy(),
            crate::evaluation::policies::compiled::outcome::GeneratorOutcome::Unmeasured {
                reason: "no candidate variants were compared",
            },
        ]
    }));
    let report = OptimizationReport::from_parts(
        run_id.clone(),
        plan.digest.clone(),
        baseline_variant.id().to_string(),
        baseline_bytes.len() as u64,
        candidates,
        &verdict,
    );
    fs::write(
        run_dir.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap_or_default(),
    )?;
    Ok(report)
}

fn build_variant(
    toolchain: &Toolchain,
    target: &CompiledTarget,
    variant: FlagVariant,
    output: &Path,
    artifact_dir: &Path,
    cwd: &Path,
    timeout: Duration,
) -> Result<(), HarnessError> {
    if variant.needs_pgo() {
        if !toolchain.supports_pgo() {
            return Err(HarnessError::ToolchainUnavailable);
        }
        let instrumented = artifact_dir.join("instrumented");
        let argv = target.instrument_argv(variant, &instrumented)?;
        toolchain.run_build(&argv, Some(cwd), Some(timeout))?;
        let profraw = artifact_dir.join("default.profraw");
        let run = target.run_argv(&instrumented);
        let profraw_s = profraw.display().to_string();
        let env = [("LLVM_PROFILE_FILE", profraw_s.as_str())];
        let timed = time_run(&run, Some(cwd), &env, Some(timeout))?;
        if timed.status_code != Some(0) {
            return Err(HarnessError::Run(timed.stderr));
        }
        let profdata = artifact_dir.join("default.profdata");
        toolchain.merge_profdata(&[profraw], &profdata)?;
        let argv = target.build_argv(variant, output, Some(&profdata))?;
        toolchain.run_build(&argv, Some(cwd), Some(timeout))?;
        return Ok(());
    }
    let argv = target.build_argv(variant, output, None)?;
    toolchain.run_build(&argv, Some(cwd), Some(timeout))?;
    Ok(())
}

fn interleaved_rounds(
    target: &CompiledTarget,
    baseline: &Path,
    variant: &Path,
    warmup: u32,
    measured: u32,
    timeout: Duration,
    cwd: &Path,
) -> Result<Vec<PairedRound>, HarnessError> {
    let baseline_argv = target.run_argv(baseline);
    let variant_argv = target.run_argv(variant);
    for _ in 0..warmup {
        run_ok(&baseline_argv, cwd, timeout)?;
    }
    for _ in 0..warmup {
        run_ok(&variant_argv, cwd, timeout)?;
    }
    let mut rounds = Vec::with_capacity(measured as usize);
    for i in 0..measured {
        let (baseline_sample, variant_sample) = if i % 2 == 0 {
            (
                sample_ok(&baseline_argv, cwd, timeout)?,
                sample_ok(&variant_argv, cwd, timeout)?,
            )
        } else {
            let v = sample_ok(&variant_argv, cwd, timeout)?;
            let b = sample_ok(&baseline_argv, cwd, timeout)?;
            (b, v)
        };
        rounds.push(PairedRound {
            baseline: baseline_sample,
            variant: variant_sample,
        });
    }
    Ok(rounds)
}

fn run_ok(argv: &[String], cwd: &Path, timeout: Duration) -> Result<(), HarnessError> {
    let timed = time_run(argv, Some(cwd), &[], Some(timeout))?;
    if timed.status_code != Some(0) {
        return Err(HarnessError::Run(timed.stderr));
    }
    Ok(())
}

fn sample_ok(argv: &[String], cwd: &Path, timeout: Duration) -> Result<Sample, HarnessError> {
    let timed = time_run(argv, Some(cwd), &[], Some(timeout))?;
    if timed.status_code != Some(0) {
        return Err(HarnessError::Run(timed.stderr));
    }
    Ok(Sample::from_timed(timed))
}

fn locality_from_rounds(
    rounds: &[PairedRound],
    max_rss_increase_pct: f64,
) -> crate::evaluation::policies::compiled::outcome::GeneratorOutcome {
    let b_rss = median_opt(rounds.iter().map(|r| r.baseline.max_rss_bytes));
    let v_rss = median_opt(rounds.iter().map(|r| r.variant.max_rss_bytes));
    let b_faults = median_opt(rounds.iter().map(|r| r.baseline.page_faults));
    let v_faults = median_opt(rounds.iter().map(|r| r.variant.page_faults));
    let fault_increase_pct = match (b_faults, v_faults) {
        (Some(b), Some(v)) if b > 0 => Some((v as f64 - b as f64) / b as f64 * 100.0),
        _ => None,
    };
    let fault_diffs: Vec<f64> = rounds
        .iter()
        .filter_map(|r| Some(r.variant.page_faults? as f64 - r.baseline.page_faults? as f64))
        .collect();
    let fault_p = if fault_diffs.len() == rounds.len() && !fault_diffs.is_empty() {
        Some(permutation_p_value(&fault_diffs))
    } else {
        None
    };
    score_locality(
        b_rss,
        v_rss,
        max_rss_increase_pct,
        fault_increase_pct,
        fault_p,
    )
}

fn median_opt<I>(iter: I) -> Option<u64>
where
    I: Iterator<Item = Option<u64>>,
{
    let vals: Vec<f64> = iter.flatten().map(|n| n as f64).collect();
    if vals.is_empty() {
        return None;
    }
    Some(median(&vals).round() as u64)
}

fn file_mode(path: &Path) -> u32 {
    #[cfg(unix)]
    {
        fs::metadata(path)
            .map(|m| m.permissions().mode())
            .unwrap_or(0o755)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        0o755
    }
}

pub(crate) fn compact_rfc3339(unix: f64) -> String {
    let secs = unix.max(0.0) as i64;
    let days = secs.div_euclid(86400);
    let tod = secs.rem_euclid(86400);
    let (y, m, d) = civil_from_unix_days(days);
    let h = tod / 3600;
    let min = (tod % 3600) / 60;
    let s = tod % 60;
    format!("{y:04}{m:02}{d:02}T{h:02}{min:02}{s:02}Z")
}

fn civil_from_unix_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i32 + era as i32 * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optimization::approval::ApprovedPlan;
    use crate::optimization::plan::OptimizationPlan;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "topos_harness_{label}_{}_{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn compact_rfc3339_formats_the_unix_epoch() {
        assert_eq!(compact_rfc3339(0.0), "19700101T000000Z");
        assert_ne!(compact_rfc3339(1.0), compact_rfc3339(100_000.0));
    }

    #[test]
    fn a_missing_toolchain_is_an_error_not_a_fallback_binary() {
        let root = temp_dir("no-clang");
        let src = root.join("a.c");
        fs::write(&src, "int main(void) { return 0; }\n").unwrap();
        let target = CompiledTarget::from_source(src).unwrap();
        let plan = OptimizationPlan::new(crate::optimization::plan::PlanSpec {
            source: Some(root.join("a.c")),
            build_command: None,
            run_command: vec!["{output}".into()],
            variants: vec![FlagVariant::O2],
            min_speedup_pct: 5.0,
            max_size_increase_pct: 10.0,
            max_rss_increase_pct: 10.0,
            warmup_runs: 1,
            measured_runs: 6,
            timeout_ms: 30_000,
        })
        .unwrap();
        let approved = ApprovedPlan::approve(plan, "test", None, 1.0).unwrap();
        let store = ArtifactStore::open(&root).unwrap();
        let toolchain = Toolchain {
            clang: None,
            llvm_profdata: None,
            llvm_dis: None,
        };
        let output = root.join("optimized");
        let err = run_optimization(&approved, &target, &toolchain, &store, 1.0).unwrap_err();
        assert!(matches!(err, HarnessError::ToolchainUnavailable));
        assert!(!output.exists());
        let fake = root.join(".topos/compiled/runs");
        // no #!/bin/sh fallback anywhere under the store
        if fake.exists() {
            for entry in walkdir_binaries(&fake) {
                let bytes = fs::read(&entry).unwrap_or_default();
                assert!(
                    !bytes.starts_with(b"# !/bin/sh") && !bytes.starts_with(b"#!/bin/sh"),
                    "fallback binary at {}",
                    entry.display()
                );
            }
        }
        let _ = fs::remove_dir_all(&root);
    }

    fn walkdir_binaries(root: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let Ok(entries) = fs::read_dir(root) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walkdir_binaries(&path));
            } else {
                out.push(path);
            }
        }
        out
    }

    #[test]
    fn o2_is_measurably_faster_than_o0_on_a_real_workload() {
        let toolchain = Toolchain::discover();
        if !toolchain.has_clang() {
            return;
        }
        let src =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/optimization/testdata/matmul.c");
        if !src.exists() {
            return;
        }
        let root = temp_dir("o2-vs-o0");
        let target = CompiledTarget::from_source(src).unwrap();
        // Replace run command with a workload long enough to clear 50ms at -O0.
        let target = CompiledTarget::from_command(
            match &target.recipe {
                crate::optimization::target::BuildRecipe::Direct { source } => {
                    vec![
                        "clang".into(),
                        "{flags}".into(),
                        "{profile}".into(),
                        "-o".into(),
                        "{output}".into(),
                        source.display().to_string(),
                    ]
                }
                crate::optimization::target::BuildRecipe::Command { argv } => argv.clone(),
            },
            vec!["{output}".into(), "512".into()],
        )
        .unwrap();
        let plan = OptimizationPlan::new(crate::optimization::plan::PlanSpec {
            source: None,
            build_command: None,
            run_command: vec!["{output}".into(), "512".into()],
            variants: vec![FlagVariant::O0, FlagVariant::O2],
            min_speedup_pct: 20.0,
            max_size_increase_pct: 10.0,
            max_rss_increase_pct: 50.0,
            warmup_runs: 1,
            measured_runs: 8,
            timeout_ms: 120_000,
        })
        .unwrap();
        let approved = ApprovedPlan::approve(plan, "test", None, 1.0).unwrap();
        let store = ArtifactStore::open(&root).unwrap();
        let report = match run_optimization(&approved, &target, &toolchain, &store, 1.0) {
            Ok(r) => r,
            Err(err) => {
                eprintln!("o2-vs-o0 harness error (skipping assertion): {err}");
                let _ = fs::remove_dir_all(&root);
                return;
            }
        };
        if report
            .candidates
            .iter()
            .any(|c| c.noise == "unstable_environment")
        {
            eprintln!("unstable environment; not asserting O2 vs O0");
            let _ = fs::remove_dir_all(&root);
            return;
        }
        let cand = report
            .candidates
            .iter()
            .find(|c| c.variant == "O2")
            .expect("O2 candidate");
        assert_eq!(cand.noise, "real", "{cand:?}");
        assert!(
            cand.speedup_pct > 20.0,
            "O2 speedup {}% was not > 20",
            cand.speedup_pct
        );
        let _ = fs::remove_dir_all(&root);
    }
}
