//! Shared plan/apply orchestration used by the CLI and MCP tools.
//!
//! `prepare_plan` probes the toolchain and never builds. `apply_approved`
//! measures, then promotes a winner only when SPEED and SIZE both pass.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::adapters::llvm::Toolchain;
use crate::config::load_topos_config;
use crate::optimization::approval::ApprovedPlan;
use crate::optimization::artifact_store::ArtifactStore;
use crate::optimization::harness::run_optimization;
use crate::optimization::plan::{OptimizationPlan, PlanSpec};
use crate::optimization::report::OptimizationReport;
use crate::optimization::target::CompiledTarget;
use crate::optimization::variant::FlagVariant;

/// Stand-in for the not-yet-known candidate binary path in `plan` previews.
const PLACEHOLDER_NAME: &str = "<output>";
use crate::paths::resolve_path_within;

#[derive(Debug, Clone)]
pub struct PlanRequest {
    pub target: PathBuf,
    pub variants: Vec<String>,
    pub min_speedup: f64,
    pub max_size_increase: f64,
    pub runs: Option<u32>,
    pub warmup: Option<u32>,
    pub run_args: Vec<String>,
    pub plan_path_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantProbe {
    pub id: String,
    pub buildable: bool,
    pub reason: String,
    pub build_command: Vec<String>,
    pub run_command: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PreparedPlan {
    pub plan: OptimizationPlan,
    pub probes: Vec<VariantProbe>,
    pub next_step: String,
}

#[derive(Debug, Clone)]
pub struct ApplyOutcome {
    pub report: OptimizationReport,
    pub dest: PathBuf,
    pub promoted: Option<PathBuf>,
}

pub fn default_variants() -> Vec<FlagVariant> {
    vec![
        FlagVariant::O2,
        FlagVariant::O3,
        FlagVariant::Os,
        FlagVariant::Lto,
        FlagVariant::PgoO3,
    ]
}

pub fn approve_next_step(plan_path: &str) -> String {
    format!(
        "next: topos compiled approve {plan_path} --by <identity>  (approve is CLI-only; not an MCP tool)"
    )
}

pub fn prepare_plan(req: PlanRequest) -> Result<PreparedPlan, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let target_path = if req.target.is_absolute() {
        req.target.clone()
    } else {
        cwd.join(&req.target)
    };
    let config = load_topos_config(&target_path);
    if let Some(err) = config.compiled_error {
        return Err(err);
    }
    let compiled = config.compiled.as_ref();
    let mut run_command = compiled
        .map(|c| c.run_command.clone())
        .unwrap_or_else(|| vec!["{output}".into()]);
    run_command.extend(req.run_args);
    let compiled_target = if let Some(c) = compiled {
        CompiledTarget::from_command(c.build_command.clone(), run_command.clone())
            .map_err(|e| e.to_string())?
    } else {
        let mut t = CompiledTarget::from_source(target_path.clone()).map_err(|e| e.to_string())?;
        t.run_command = run_command.clone();
        t
    };
    let variants = parse_variants(&req.variants)?;
    let warmup = req.warmup.or(compiled.map(|c| c.warmup_runs)).unwrap_or(2);
    let runs = req.runs.or(compiled.map(|c| c.measured_runs)).unwrap_or(10);
    let timeout_ms = compiled.map(|c| c.timeout_ms).unwrap_or(60_000);
    let source = if compiled.is_some() {
        None
    } else {
        Some(target_path)
    };
    let plan = OptimizationPlan::new(PlanSpec {
        source,
        build_command: compiled.map(|c| c.build_command.clone()),
        run_command,
        variants: variants.clone(),
        min_speedup_pct: req.min_speedup,
        max_size_increase_pct: req.max_size_increase,
        max_rss_increase_pct: 10.0,
        warmup_runs: warmup,
        measured_runs: runs,
        timeout_ms,
    })
    .map_err(|e| e.to_string())?;
    let toolchain = Toolchain::discover();
    let placeholder = PathBuf::from(PLACEHOLDER_NAME);
    let mut probes = Vec::new();
    for variant in &variants {
        let (buildable, reason) = if !toolchain.has_clang() {
            (false, "clang not found".to_string())
        } else if variant.needs_pgo() && !toolchain.supports_pgo() {
            (false, "llvm-profdata not found".to_string())
        } else {
            (true, String::new())
        };
        let build_command = if variant.needs_pgo() {
            compiled_target
                .instrument_argv(*variant, &placeholder)
                .map_err(|e| e.to_string())?
        } else {
            compiled_target
                .build_argv(*variant, &placeholder, None)
                .map_err(|e| e.to_string())?
        };
        probes.push(VariantProbe {
            id: variant.id().to_string(),
            buildable,
            reason,
            build_command: strip_placeholder_prefix(build_command),
            run_command: strip_placeholder_prefix(compiled_target.run_argv(&placeholder)),
        });
    }
    Ok(PreparedPlan {
        plan,
        probes,
        next_step: approve_next_step(&req.plan_path_hint),
    })
}

/// `build_argv` absolutizes the output path, which turns the `<output>`
/// preview placeholder into a cwd-prefixed path that does not exist and never
/// will. The plan is a preview — it builds nothing — so render the placeholder
/// as a bare token rather than a plausible-looking lie.
fn strip_placeholder_prefix(argv: Vec<String>) -> Vec<String> {
    argv.into_iter()
        .map(|arg| match arg.rsplit_once('/') {
            Some((_, PLACEHOLDER_NAME)) => PLACEHOLDER_NAME.to_string(),
            _ => arg,
        })
        .collect()
}

pub fn apply_approved(
    approved: &ApprovedPlan,
    project_root: &Path,
    now: f64,
) -> Result<ApplyOutcome, String> {
    let compiled_target = reconstruct_target(approved.plan())?;
    let dest = promotion_dest(approved.plan(), project_root)?;
    let original = if dest.exists() {
        Some((
            fs::read(&dest).map_err(|e| e.to_string())?,
            file_mode(&dest),
        ))
    } else {
        None
    };
    let store = ArtifactStore::open(project_root).map_err(|e| e.to_string())?;
    let toolchain = Toolchain::discover();
    let report = run_optimization(approved, &compiled_target, &toolchain, &store, now)
        .map_err(|e| e.to_string())?;
    if let Some((bytes, mode)) = original.as_ref() {
        store
            .set_rollback_snapshot(&report.run_id, &dest, Some(bytes), Some(*mode))
            .map_err(|e| e.to_string())?;
    } else if report.winner.is_some() {
        store
            .set_rollback_snapshot(&report.run_id, &dest, None, None)
            .map_err(|e| e.to_string())?;
    }
    let promoted = if let Some(winner_id) = &report.winner {
        let slot = parse_slot(winner_id)?;
        let src = store
            .candidate_dir(&report.run_id, slot)
            .map_err(|e| e.to_string())?
            .join("binary");
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(&src, &dest).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&src)
                .map(|m| m.permissions().mode())
                .unwrap_or(0o755);
            fs::set_permissions(&dest, fs::Permissions::from_mode(mode))
                .map_err(|e| e.to_string())?;
        }
        Some(dest.clone())
    } else {
        None
    };
    Ok(ApplyOutcome {
        report,
        dest,
        promoted,
    })
}

pub fn reconstruct_target(plan: &OptimizationPlan) -> Result<CompiledTarget, String> {
    if let Some(argv) = &plan.build_command {
        CompiledTarget::from_command(argv.clone(), plan.run_command.clone())
            .map_err(|e| e.to_string())
    } else {
        let source = plan
            .source
            .clone()
            .ok_or_else(|| "plan has neither source nor build_command".to_string())?;
        let mut t = CompiledTarget::from_source(source).map_err(|e| e.to_string())?;
        t.run_command = plan.run_command.clone();
        Ok(t)
    }
}

fn parse_variants(raw: &[String]) -> Result<Vec<FlagVariant>, String> {
    if raw.is_empty() {
        return Ok(default_variants());
    }
    raw.iter()
        .map(|id| FlagVariant::parse(id).map_err(|e| e.to_string()))
        .collect()
}

fn promotion_dest(plan: &OptimizationPlan, project_root: &Path) -> Result<PathBuf, String> {
    let dest = match &plan.source {
        Some(src) => {
            let name = src.file_stem().unwrap_or_default();
            project_root.join(name)
        }
        None => project_root.join("compiled-output"),
    };
    resolve_path_within(&dest.to_string_lossy(), project_root)
}

fn parse_slot(id: &str) -> Result<usize, String> {
    id.strip_prefix('c')
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("invalid candidate id {id}"))
}

fn file_mode(path: &Path) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
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
