//! `topos compiled` — plan, approve, apply, rollback.
//!
//! `plan` builds and runs nothing. `approve` is CLI-only. `apply` promotes
//! a winner only when SPEED and SIZE both pass; otherwise it prints
//! `NO CANDIDATE MET THRESHOLDS` and leaves the destination untouched.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Args, Subcommand};
use console::Style;
use serde_json::json;
use topos_engine::config::load_topos_config;
use topos_engine::optimization::approval::{ApprovalError, ApprovedPlan};
use topos_engine::optimization::artifact_store::ArtifactStore;
use topos_engine::optimization::ops::{apply_approved, prepare_plan, PlanRequest};
use topos_engine::optimization::plan::OptimizationPlan;

use super::render::{guide, guide_line, paint, RenderOptions};

/// Where `plan` writes when `--out` is not given. Gitignored alongside the
/// rest of the run artifacts.
const DEFAULT_PLAN_PATH: &str = ".topos/compiled/plan.json";

#[derive(Args)]
pub struct CompiledArgs {
    #[command(subcommand)]
    pub command: CompiledCommand,
}

#[derive(Subcommand)]
pub enum CompiledCommand {
    /// Resolve a target and emit a plan. Builds and runs nothing.
    Plan(PlanArgs),
    /// Sign a plan. Required before apply; not exposed over MCP.
    Approve(ApproveArgs),
    /// Measure variants and promote a winner that passes both gates.
    Apply(ApplyArgs),
    /// Restore the pre-apply baseline binary.
    Rollback(RollbackArgs),
}

#[derive(Args)]
pub struct PlanArgs {
    /// C/C++ source file, or any path used to locate `.topos.toml`.
    pub target: PathBuf,
    #[arg(long = "variant")]
    pub variants: Vec<String>,
    #[arg(long, default_value_t = 5.0)]
    pub min_speedup: f64,
    #[arg(long, default_value_t = 10.0)]
    pub max_size_increase: f64,
    #[arg(long)]
    pub runs: Option<u32>,
    #[arg(long)]
    pub warmup: Option<u32>,
    #[arg(long)]
    pub out: Option<PathBuf>,
    #[arg(long)]
    pub json: bool,
    /// Arguments after `--` are appended to the run command.
    #[arg(last = true)]
    pub run_args: Vec<String>,
}

#[derive(Args)]
pub struct ApproveArgs {
    pub plan: PathBuf,
    #[arg(long)]
    pub by: String,
    #[arg(long)]
    pub note: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct ApplyArgs {
    pub plan: PathBuf,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct RollbackArgs {
    #[arg(long = "run")]
    pub run_id: Option<String>,
    #[arg(long)]
    pub json: bool,
}

pub fn run(args: CompiledArgs) -> Result<(), String> {
    match args.command {
        CompiledCommand::Plan(args) => run_plan(args),
        CompiledCommand::Approve(args) => run_approve(args),
        CompiledCommand::Apply(args) => run_apply(args),
        CompiledCommand::Rollback(args) => run_rollback(args),
    }
}

fn run_plan(args: PlanArgs) -> Result<(), String> {
    // A plan that is only printed cannot be approved, and `approve` takes a
    // path. Always write one, defaulting to the documented location.
    let out = args
        .out
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_PLAN_PATH));
    let prepared = prepare_plan(PlanRequest {
        target: args.target.clone(),
        variants: args.variants.clone(),
        min_speedup: args.min_speedup,
        max_size_increase: args.max_size_increase,
        runs: args.runs,
        warmup: args.warmup,
        run_args: args.run_args.clone(),
        plan_path_hint: out.display().to_string(),
    })?;
    let plan = prepared.plan;
    let probes = prepared.probes;
    let next_step = prepared.next_step;
    let plan_json = serde_json::to_string_pretty(&plan).map_err(|e| e.to_string())?;
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("creating {}: {e}", parent.display()))?;
        }
    }
    fs::write(&out, &plan_json).map_err(|e| format!("writing {}: {e}", out.display()))?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "plan": plan,
                "variants": probes,
                "next_step": next_step,
                "note": "plan does not build or run anything",
            }))
            .map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    let options = RenderOptions::stdout();
    println!(
        "{}",
        paint("◇  Compiled plan", Style::new().bold(), options)
    );
    println!(
        "{}",
        guide_line(
            format!("digest  {}", plan.digest),
            Style::new().dim(),
            options
        )
    );
    for probe in &probes {
        let status = if probe.buildable {
            "buildable"
        } else {
            "unbuildable"
        };
        println!(
            "{}",
            guide_line(
                format!("{:<8} {status}  {}", probe.id, probe.reason),
                Style::new(),
                options
            )
        );
        println!(
            "{}",
            guide_line(
                format!("         build  {}", probe.build_command.join(" ")),
                Style::new().dim(),
                options
            )
        );
        println!(
            "{}",
            guide_line(
                format!("         run    {}", probe.run_command.join(" ")),
                Style::new().dim(),
                options
            )
        );
    }
    println!("{}", guide('│', options));
    println!(
        "{}",
        guide_line(
            format!("written {}", out.display()),
            Style::new().dim(),
            options
        )
    );
    println!("{}", guide_line(next_step, Style::new().yellow(), options));
    println!("{}", guide('└', options));
    Ok(())
}

fn run_approve(args: ApproveArgs) -> Result<(), String> {
    let text = fs::read_to_string(&args.plan)
        .map_err(|e| format!("reading plan {}: {e}", args.plan.display()))?;
    let plan = OptimizationPlan::from_json(&text).or_else(|_| {
        ApprovedPlan::from_json(&text)
            .map(|a| a.plan().clone())
            .map_err(|e| e.to_string())
    })?;
    let approved =
        ApprovedPlan::approve(plan, args.by, args.note, unix_now()).map_err(|e| e.to_string())?;
    let json = approved.to_json()?;
    fs::write(&args.plan, &json).map_err(|e| e.to_string())?;
    if args.json {
        println!("{json}");
        return Ok(());
    }
    let options = RenderOptions::stdout();
    println!(
        "{}",
        paint("◇  Plan approved", Style::new().bold(), options)
    );
    println!(
        "{}",
        guide_line(
            format!("by     {}", approved.approved_by()),
            Style::new(),
            options
        )
    );
    println!(
        "{}",
        guide_line(
            format!("digest {}", approved.plan().digest),
            Style::new().dim(),
            options
        )
    );
    println!(
        "{}",
        guide_line(
            format!("next   topos compiled apply {}", args.plan.display()),
            Style::new().yellow(),
            options
        )
    );
    println!("{}", guide('└', options));
    Ok(())
}

fn run_apply(args: ApplyArgs) -> Result<(), String> {
    let text = fs::read_to_string(&args.plan)
        .map_err(|e| format!("reading plan {}: {e}", args.plan.display()))?;
    let approved = ApprovedPlan::from_json(&text).map_err(|e| match e {
        ApprovalError::NotApproved => e.to_string(),
        other => other.to_string(),
    })?;
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_topos_config(&cwd);
    if let Some(err) = config.compiled_error {
        return Err(err);
    }
    let project_root = config.root.clone().unwrap_or(cwd);
    let outcome = apply_approved(&approved, &project_root, unix_now())?;
    let report = outcome.report;
    let promoted = outcome.promoted;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "report": report,
                "promoted": promoted.as_ref().map(|p| p.display().to_string()),
                "message": if promoted.is_some() {
                    "promoted"
                } else {
                    "NO CANDIDATE MET THRESHOLDS"
                },
            }))
            .map_err(|e| e.to_string())?
        );
        return if promoted.is_some() {
            Ok(())
        } else {
            Err("NO CANDIDATE MET THRESHOLDS".into())
        };
    }
    let options = RenderOptions::stdout();
    println!(
        "{}",
        paint("◇  Compiled apply", Style::new().bold(), options)
    );
    println!(
        "{}",
        guide_line(
            format!("run {}  ·  digest {}", report.run_id, report.plan_digest),
            Style::new().dim(),
            options
        )
    );
    println!("{}", guide('│', options));
    println!(
        "{}",
        guide_line(
            format!(
                "{:<9} {:>9} {:>9} {:<20} {:>9} {:>7}",
                "VARIANT", "SPEEDUP", "p", "VERDICT", "SIZE", "GATES"
            ),
            Style::new().bold().dim(),
            options
        )
    );
    // The baseline is the arm every candidate is measured against; without it
    // the deltas below have no referent.
    println!(
        "{}",
        guide_line(
            format!(
                "{:<9} {:>9} {:>9} {:<20} {:>9}",
                report.baseline_variant,
                "—",
                "—",
                "baseline",
                format!("{} B", report.baseline_bytes)
            ),
            Style::new().dim(),
            options
        )
    );
    for c in &report.candidates {
        // A p-value is absent when no statistical test ran (size-only, or a
        // refusal such as workload_too_short). Show that, never a fake number.
        let p = match c.p_value {
            Some(p) => format!("{p:.4}"),
            None => "—".to_string(),
        };
        let gates = format!(
            "{}{}",
            if c.speed_satisfied { "S" } else { "·" },
            if c.size_satisfied { "Z" } else { "·" }
        );
        println!(
            "{}",
            guide_line(
                format!(
                    "{:<9} {:>+8.1}% {:>9} {:<20} {:>+8.1}% {:>7}",
                    c.variant, c.speedup_pct, p, c.noise, c.size_increase_pct, gates
                ),
                Style::new(),
                options
            )
        );
    }
    println!("{}", guide('│', options));
    // Always disclose the ceiling: it is what stops "we did not measure it"
    // from reading as "it passed".
    let medal_line = if report.medal == report.medal_ceiling {
        format!("Ω_bitcode  {}", report.medal)
    } else {
        format!(
            "Ω_bitcode  {}   (ceiling {} — unmeasured generators cannot be satisfied)",
            report.medal, report.medal_ceiling
        )
    };
    println!("{}", guide_line(medal_line, Style::new().bold(), options));
    if let Some(path) = &promoted {
        println!(
            "{}",
            guide_line(
                format!("promoted {}", path.display()),
                Style::new().green(),
                options
            )
        );
        println!("{}", guide('└', options));
        Ok(())
    } else {
        println!(
            "{}",
            guide_line("NO CANDIDATE MET THRESHOLDS", Style::new().red(), options)
        );
        println!("{}", guide('└', options));
        Err("NO CANDIDATE MET THRESHOLDS".into())
    }
}

fn run_rollback(args: RollbackArgs) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_topos_config(&cwd);
    let project_root = config.root.clone().unwrap_or(cwd);
    let store = ArtifactStore::open(&project_root).map_err(|e| e.to_string())?;
    let result = store
        .rollback_recorded(args.run_id.as_deref())
        .map_err(|e| e.to_string())?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "bytes_restored": result.bytes_restored,
                "verified": result.verified,
            }))
            .map_err(|e| e.to_string())?
        );
        return if result.verified {
            Ok(())
        } else {
            Err("rollback read-back did not match stored baseline".into())
        };
    }
    let options = RenderOptions::stdout();
    println!(
        "{}",
        paint("◇  Compiled rollback", Style::new().bold(), options)
    );
    println!(
        "{}",
        guide_line(
            format!(
                "restored {} bytes  verified={}",
                result.bytes_restored, result.verified
            ),
            Style::new(),
            options
        )
    );
    println!("{}", guide('└', options));
    if result.verified {
        Ok(())
    } else {
        Err("rollback read-back did not match stored baseline".into())
    }
}

fn unix_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "topos_compiled_cli_{label}_{}_{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_c(dir: &Path) -> PathBuf {
        let src = dir.join("tiny.c");
        fs::write(&src, "int main(void) { return 0; }\n").unwrap();
        src
    }

    #[test]
    fn plan_does_not_build_or_run_anything() {
        let dir = temp_dir("plan");
        let src = write_c(&dir);
        let out = dir.join("plan.json");
        run_plan(PlanArgs {
            target: src,
            variants: vec!["O2".into(), "O3".into()],
            min_speedup: 5.0,
            max_size_increase: 10.0,
            runs: Some(6),
            warmup: Some(1),
            out: Some(out.clone()),
            json: true,
            run_args: vec!["1".into()],
        })
        .unwrap();
        let plan: OptimizationPlan = serde_json::from_slice(&fs::read(&out).unwrap()).unwrap();
        assert!(plan.verify_digest());
        assert!(
            !dir.join(".topos").exists() || {
                let runs = dir.join(".topos/compiled/runs");
                !runs.exists() || fs::read_dir(&runs).map(|rd| rd.count()).unwrap_or(0) == 0
            }
        );
        assert!(!dir.join("tiny").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_refuses_an_unapproved_plan() {
        let dir = temp_dir("unapproved");
        let src = write_c(&dir);
        let out = dir.join("plan.json");
        run_plan(PlanArgs {
            target: src,
            variants: vec!["O2".into()],
            min_speedup: 5.0,
            max_size_increase: 10.0,
            runs: Some(6),
            warmup: Some(0),
            out: Some(out.clone()),
            json: true,
            run_args: vec![],
        })
        .unwrap();
        let err = run_apply(ApplyArgs {
            plan: out,
            json: true,
        })
        .unwrap_err();
        assert!(err.contains("not approved"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn approve_rewrites_the_plan_with_the_injected_identity() {
        let dir = temp_dir("approve");
        let src = write_c(&dir);
        let out = dir.join("plan.json");
        run_plan(PlanArgs {
            target: src,
            variants: vec!["O2".into()],
            min_speedup: 5.0,
            max_size_increase: 10.0,
            runs: Some(6),
            warmup: Some(0),
            out: Some(out.clone()),
            json: true,
            run_args: vec![],
        })
        .unwrap();
        run_approve(ApproveArgs {
            plan: out.clone(),
            by: "reviewer@topos".into(),
            note: Some("ok".into()),
            json: true,
        })
        .unwrap();
        let approved = ApprovedPlan::from_json(&fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(approved.approved_by(), "reviewer@topos");
        assert_eq!(approved.note(), Some("ok"));
        let _ = fs::remove_dir_all(&dir);
    }
}
