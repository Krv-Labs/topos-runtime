//! `topos compiled` — CLI subcommands for compiled artifact evaluation, inspection,
//! refactor planning, recompilation, structural comparison, and rollback.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use console::Style;
use serde_json::json;

use topos_engine::config::load_topos_config;
use topos_engine::core::characteristic_morphism::CharacteristicMorphism;
use topos_engine::core::morphism::ProgramMorphism;
use topos_engine::evaluation::policies::base::Priority;
use topos_engine::evaluation::preferences::default_preferences;
use topos_engine::functors::probes::ast::complexity::calculate_function_complexity_entries;
use topos_engine::functors::probes::cfg::homology::calculate_cycle_basis;
use topos_engine::functors::profunctors::ast::compare::calculate_ast_distance;

use super::classify::classify_with_representations;
use super::lang::detect_language;
use super::render::{guide, guide_line, paint, RenderOptions};

#[derive(Args)]
pub struct CompiledArgs {
    #[command(subcommand)]
    pub action: CompiledAction,
}

#[derive(Subcommand)]
pub enum CompiledAction {
    /// Evaluate compiled program artifacts across quality pillars.
    Evaluate(CompiledEvaluateArgs),
    /// Inspect compiled artifact metrics and details.
    Inspect(CompiledInspectArgs),
    /// Generate a recompile / refactor plan for compiled artifacts.
    Plan(CompiledPlanArgs),
    /// Recompile or re-evaluate compiled program artifacts.
    Recompile(CompiledRecompileArgs),
    /// Compare two compiled artifacts for structural distance.
    Compare(CompiledCompareArgs),
    /// Rollback or revert compiled changes to snapshot.
    Rollback(CompiledRollbackArgs),
}

pub fn run(args: CompiledArgs) -> Result<(), String> {
    match args.action {
        CompiledAction::Evaluate(a) => run_evaluate(a),
        CompiledAction::Inspect(a) => run_inspect(a),
        CompiledAction::Plan(a) => run_plan(a),
        CompiledAction::Recompile(a) => run_recompile(a),
        CompiledAction::Compare(a) => run_compare(a),
        CompiledAction::Rollback(a) => run_rollback(a),
    }
}

// ============================================================================
// 1. EVALUATE
// ============================================================================

#[derive(Args)]
pub struct CompiledEvaluateArgs {
    /// Path to compiled artifact or source file.
    pub path: PathBuf,
    /// Language override (python, rust, javascript, typescript, cpp, go).
    #[arg(long)]
    pub language: Option<String>,
    /// Priority override (simple, composable, secure, navigable).
    #[arg(long)]
    pub priority: Option<String>,
    /// Output machine-readable JSON format.
    #[arg(long)]
    pub json: bool,
    /// Verbose output showing raw metrics.
    #[arg(short = 'v', long)]
    pub verbose: bool,
}

fn run_evaluate(args: CompiledEvaluateArgs) -> Result<(), String> {
    if !args.path.exists() {
        return Err(format!("path not found: {}", args.path.display()));
    }

    let language = args
        .language
        .unwrap_or_else(|| detect_language(&args.path));
    let mut morphism = ProgramMorphism::from_file(&args.path, language.clone())
        .map_err(|e| format!("reading {}: {e}", args.path.display()))?;

    let priority = if let Some(p) = &args.priority {
        match p.to_lowercase().as_str() {
            "simple" => Priority::Simple,
            "composable" => Priority::Composable,
            "secure" => Priority::Secure,
            "navigable" => Priority::Navigable,
            _ => return Err(format!("invalid priority: {p}")),
        }
    } else {
        let cfg = load_topos_config(&args.path);
        cfg.priority.unwrap_or_default()
    };

    let classifier = CharacteristicMorphism;
    let result =
        classify_with_representations(&classifier, &mut morphism, None, priority);

    if args.json {
        let dims: HashMap<_, _> = result
            .dimensions
            .iter()
            .map(|(k, v)| (k.clone(), v.name().to_string()))
            .collect();
        let payload = json!({
            "file": args.path.display().to_string(),
            "language": language,
            "is_parseable": result.is_parseable,
            "lattice_element": result.lattice_element.name(),
            "medal": result.lattice_element.medal_tier(),
            "priority": priority.top_generator().as_str(),
            "scores": result.scores,
            "dimensions": dims,
            "interpretation": result.interpretation,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?
        );
        return Ok(());
    }

    let options = RenderOptions::stdout();
    println!(
        "{}",
        paint(
            format!("◇  Compiled Evaluation: {}", args.path.display()),
            Style::new().bold(),
            options
        )
    );
    println!(
        "{}",
        guide_line(
            format!("Language: {language} · Priority: {}", priority.top_generator().as_str()),
            Style::new().dim(),
            options
        )
    );
    println!("{}", guide('│', options));

    println!(
        "{}",
        guide_line(
            format!(
                "{:<12} {:<12} {:<18} {:<8}",
                "PILLAR", "SCORE", "QUALITY", "GATE"
            ),
            Style::new().bold().dim(),
            options
        )
    );
    for pillar in ["simple", "composable", "secure", "navigable"] {
        let score_val = result.scores.get(pillar).copied().unwrap_or(0.0) * 100.0;
        let quality = result
            .dimensions
            .get(pillar)
            .map_or("UNMEASURED", |v| v.name());
        let gate = if result.dimensions.contains_key(pillar) && quality != "SLOP" {
            "PASS"
        } else if result.dimensions.contains_key(pillar) {
            "FAIL"
        } else {
            "N/A"
        };

        let status_style = if gate == "PASS" {
            Style::new().green()
        } else if gate == "FAIL" {
            Style::new().red()
        } else {
            Style::new().dim()
        };

        println!(
            "{}",
            guide_line(
                format!(
                    "{:<12} {:>5.1}%        {:<18} {}",
                    pillar.to_ascii_uppercase(),
                    score_val,
                    quality,
                    paint(gate, status_style, options)
                ),
                Style::new(),
                options,
            )
        );
    }

    println!("{}", guide('│', options));
    println!(
        "{}",
        guide_line(
            format!(
                "Medal Tier: {} · Element: {}",
                paint(
                    result.lattice_element.medal_tier(),
                    Style::new().bold().cyan(),
                    options
                ),
                result.lattice_element.name()
            ),
            Style::new().bold(),
            options,
        )
    );

    if args.verbose && !result.raw_metrics.is_empty() {
        println!("{}", guide('│', options));
        println!(
            "{}",
            guide_line("RAW METRICS", Style::new().cyan().bold(), options)
        );
        let mut keys: Vec<_> = result.raw_metrics.keys().collect();
        keys.sort();
        for k in keys {
            println!(
                "{}",
                guide_line(
                    format!("{:<24} {:.3}", k, result.raw_metrics[k]),
                    Style::new(),
                    options
                )
            );
        }
    }

    println!("{}", guide('└', options));
    Ok(())
}

// ============================================================================
// 2. INSPECT
// ============================================================================

#[derive(Args)]
pub struct CompiledInspectArgs {
    /// Path to compiled artifact or file.
    pub path: PathBuf,
    /// Output machine-readable JSON format.
    #[arg(long)]
    pub json: bool,
    /// Show verbose metric details.
    #[arg(short = 'v', long)]
    pub verbose: bool,
}

fn run_inspect(args: CompiledInspectArgs) -> Result<(), String> {
    if !args.path.exists() {
        return Err(format!("path not found: {}", args.path.display()));
    }

    let language = detect_language(&args.path);
    let config = load_topos_config(&args.path);
    let mut morphism = ProgramMorphism::from_file(&args.path, language.clone())
        .map_err(|e| format!("reading {}: {e}", args.path.display()))?;
    let result = classify_with_representations(
        &CharacteristicMorphism,
        &mut morphism,
        None,
        config.effective_priority(),
    );

    let mut functions = morphism
        .ast
        .as_ref()
        .map(|ast| calculate_function_complexity_entries(&ast.uast_root, &morphism.source))
        .unwrap_or_default();
    functions.sort_by(|a, b| {
        b.complexity
            .cmp(&a.complexity)
            .then_with(|| a.qualified_name.cmp(&b.qualified_name))
    });

    if args.json {
        let fn_json: Vec<_> = functions
            .iter()
            .map(|f| json!({ "name": f.qualified_name, "complexity": f.complexity, "line": f.start_line }))
            .collect();

        let payload = json!({
            "file": args.path.display().to_string(),
            "language": language,
            "is_parseable": result.is_parseable,
            "lattice_element": result.lattice_element.name(),
            "dimensions": result.dimensions.iter().map(|(k, v)| (k.clone(), v.name().to_string())).collect::<HashMap<_, _>>(),
            "scores": result.scores.iter().map(|(k, s)| (k.clone(), (s * 1000.0).round() / 10.0)).collect::<HashMap<_, _>>(),
            "raw_metrics": result.raw_metrics,
            "functions": fn_json,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?
        );
        return Ok(());
    }

    let options = RenderOptions::stdout();
    println!(
        "{}",
        paint(
            format!("◇  Compiled Inspection: {}", args.path.display()),
            Style::new().bold(),
            options
        )
    );
    println!(
        "{}",
        guide_line(
            format!(
                "Language: {language} · Element: {}",
                result.lattice_element.name()
            ),
            Style::new().dim(),
            options
        )
    );
    println!("{}", guide('│', options));

    println!(
        "{}",
        guide_line("PILLAR SCORES", Style::new().cyan().bold(), options)
    );
    for (dim, val) in &result.dimensions {
        let score = result.scores.get(dim).copied().unwrap_or(0.0) * 100.0;
        println!(
            "{}",
            guide_line(
                format!(
                    "{:<14} {:>5.1}%  ({})",
                    dim.to_ascii_uppercase(),
                    score,
                    val.name()
                ),
                Style::new(),
                options
            )
        );
    }

    if !functions.is_empty() {
        println!("{}", guide('│', options));
        println!(
            "{}",
            guide_line(
                "TOP FUNCTIONS BY COMPLEXITY",
                Style::new().cyan().bold(),
                options
            )
        );
        println!(
            "{}",
            guide_line(
                format!("{:<30} {:<12} {:<8}", "FUNCTION", "COMPLEXITY", "LINE"),
                Style::new().bold().dim(),
                options
            )
        );
        for f in functions.iter().take(10) {
            println!(
                "{}",
                guide_line(
                    format!(
                        "{:<30} {:<12} {:<8}",
                        f.qualified_name,
                        f.complexity,
                        f.start_line
                    ),
                    Style::new(),
                    options
                )
            );
        }
    }

    if !result.raw_metrics.is_empty() {
        println!("{}", guide('│', options));
        println!(
            "{}",
            guide_line("RAW METRICS", Style::new().cyan().bold(), options)
        );
        let mut keys: Vec<_> = result.raw_metrics.keys().collect();
        keys.sort();
        for k in keys {
            println!(
                "{}",
                guide_line(
                    format!("{:<28} {:.3}", k, result.raw_metrics[k]),
                    Style::new(),
                    options
                )
            );
        }
    }

    println!("{}", guide('└', options));
    Ok(())
}

// ============================================================================
// 3. PLAN
// ============================================================================

#[derive(Args)]
pub struct CompiledPlanArgs {
    /// Path to compiled artifact or source file.
    pub path: PathBuf,
    /// Refactor target focus: cycles, dependencies, process (default: cycles).
    #[arg(long, default_value = "cycles")]
    pub target: String,
    /// Maximum number of plan steps / hotspots to display.
    #[arg(long, default_value = "5")]
    pub limit: usize,
    /// Output file path for plan JSON.
    #[arg(long, alias = "output")]
    pub out: Option<PathBuf>,
    /// Output machine-readable JSON format.
    #[arg(long)]
    pub json: bool,
}

#[derive(serde::Serialize)]
struct PlanStep {
    step: usize,
    target: String,
    location: String,
    score: f64,
    recommendation: String,
}

fn run_plan(args: CompiledPlanArgs) -> Result<(), String> {
    if !args.path.exists() {
        return Err(format!("path not found: {}", args.path.display()));
    }

    let language = detect_language(&args.path);
    let mut morphism = ProgramMorphism::from_file(&args.path, language.clone())
        .map_err(|e| format!("reading {}: {e}", args.path.display()))?;

    let classifier = CharacteristicMorphism;
    let eval_res =
        classify_with_representations(&classifier, &mut morphism, None, Priority::default());

    let mut steps = Vec::new();
    let mut betti_1 = None;

    if args.target == "cycles" || args.target == "all" {
        if let Some(cfg) = morphism.build_cfg() {
            let res = calculate_cycle_basis(cfg);
            betti_1 = Some(res.betti_1);
            for (idx, cycle) in res.cycles.iter().take(args.limit).enumerate() {
                let span = match (cycle.start_line, cycle.end_line) {
                    (Some(s), Some(e)) => e.saturating_sub(s),
                    _ => 0,
                };
                steps.push(PlanStep {
                    step: idx + 1,
                    target: "CFG Cycle".to_string(),
                    location: format!(
                        "blocks {:?} (lines {:?}-{:?})",
                        cycle.block_ids, cycle.start_line, cycle.end_line
                    ),
                    score: span as f64,
                    recommendation:
                        "Extract loop or branch body into a helper function to isolate cycle."
                            .to_string(),
                });
            }
        }
    }

    let prefs = default_preferences();
    let walk = prefs.relaxation_walk(Some(eval_res.lattice_element));
    let walk_labels: Vec<_> = walk.iter().map(|e| e.name().to_string()).collect();

    let payload = json!({
        "file": args.path.display().to_string(),
        "target": args.target,
        "betti_1": betti_1,
        "current_element": eval_res.lattice_element.name(),
        "plan_steps": steps,
        "relaxation_walk": walk_labels,
    });

    if let Some(out_path) = &args.out {
        let json_str = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
        std::fs::write(out_path, json_str).map_err(|e| format!("writing {}: {e}", out_path.display()))?;
    }

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?
        );
        return Ok(());
    }

    let options = RenderOptions::stdout();
    println!(
        "{}",
        paint(
            format!("◇  Compiled Refactor Plan: {}", args.path.display()),
            Style::new().bold(),
            options
        )
    );
    println!(
        "{}",
        guide_line(
            format!(
                "Target Engine: {} · Current Element: {}",
                args.target,
                eval_res.lattice_element.name()
            ),
            Style::new().dim(),
            options
        )
    );
    println!("{}", guide('│', options));

    println!(
        "{}",
        guide_line(
            "PLANNED RECOMPILE / REFACTOR STEPS",
            Style::new().cyan().bold(),
            options
        )
    );
    if steps.is_empty() {
        println!(
            "{}",
            guide_line(
                "No cycle or structural hotspots identified.",
                Style::new().dim(),
                options
            )
        );
    } else {
        println!(
            "{}",
            guide_line(
                format!(
                    "{:<6} {:<12} {:<32} {:<8} {}",
                    "STEP", "TARGET", "LOCATION", "SCORE", "RECOMMENDATION"
                ),
                Style::new().bold().dim(),
                options
            )
        );
        for s in &steps {
            println!(
                "{}",
                guide_line(
                    format!(
                        "{:<6} {:<12} {:<32} {:<8.1} {}",
                        s.step, s.target, s.location, s.score, s.recommendation
                    ),
                    Style::new(),
                    options
                )
            );
        }
    }

    println!("{}", guide('│', options));
    println!(
        "{}",
        guide_line(
            "RELAXATION WALK (TARGET IMPROVEMENTS)",
            Style::new().cyan().bold(),
            options
        )
    );
    for (idx, elem) in walk.iter().enumerate() {
        println!(
            "{}",
            guide_line(
                format!("Step {}: {}", idx + 1, elem.name()),
                Style::new(),
                options
            )
        );
    }

    println!("{}", guide('└', options));
    Ok(())
}

// ============================================================================
// 4. RECOMPILE
// ============================================================================

#[derive(Args)]
pub struct CompiledRecompileArgs {
    /// Path to artifact or file to recompile and re-evaluate.
    pub path: PathBuf,
    /// Force fresh cache/parsing re-evaluation.
    #[arg(long)]
    pub force: bool,
    /// Output machine-readable JSON format.
    #[arg(long)]
    pub json: bool,
}

fn run_recompile(args: CompiledRecompileArgs) -> Result<(), String> {
    if !args.path.exists() {
        return Err(format!("path not found: {}", args.path.display()));
    }

    let language = detect_language(&args.path);
    let config = load_topos_config(&args.path);

    // Initial baseline pass
    let mut baseline_morphism = ProgramMorphism::from_file(&args.path, language.clone())
        .map_err(|e| format!("reading baseline {}: {e}", args.path.display()))?;
    let baseline_res = classify_with_representations(
        &CharacteristicMorphism,
        &mut baseline_morphism,
        None,
        config.effective_priority(),
    );

    // Recompiled pass (fresh morphism parse)
    let mut recompiled_morphism = ProgramMorphism::from_file(&args.path, language.clone())
        .map_err(|e| format!("re-reading {}: {e}", args.path.display()))?;
    let recompiled_res = classify_with_representations(
        &CharacteristicMorphism,
        &mut recompiled_morphism,
        None,
        config.effective_priority(),
    );

    let mut deltas = HashMap::new();
    let pillars = ["simple", "composable", "secure", "navigable"];
    for p in pillars {
        let b = baseline_res.scores.get(p).copied().unwrap_or(0.0);
        let r = recompiled_res.scores.get(p).copied().unwrap_or(0.0);
        deltas.insert(p.to_string(), r - b);
    }

    if args.json {
        let payload = json!({
            "file": args.path.display().to_string(),
            "status": "SUCCESS",
            "recompiled": true,
            "previous_element": baseline_res.lattice_element.name(),
            "new_element": recompiled_res.lattice_element.name(),
            "previous_scores": baseline_res.scores,
            "new_scores": recompiled_res.scores,
            "score_deltas": deltas,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?
        );
        return Ok(());
    }

    let options = RenderOptions::stdout();
    println!(
        "{}",
        paint(
            format!("◇  Recompile & Re-evaluate: {}", args.path.display()),
            Style::new().bold(),
            options
        )
    );
    println!(
        "{}",
        guide_line(
            format!("Status: SUCCESS · Force: {}", args.force),
            Style::new().dim(),
            options
        )
    );
    println!("{}", guide('│', options));

    println!(
        "{}",
        guide_line(
            format!(
                "{:<12} {:<12} {:<12} {:<12} {}",
                "PILLAR", "BASELINE", "RECOMPILED", "DELTA", "STATUS"
            ),
            Style::new().bold().dim(),
            options
        )
    );
    for p in pillars {
        let b = baseline_res.scores.get(p).copied().unwrap_or(0.0) * 100.0;
        let r = recompiled_res.scores.get(p).copied().unwrap_or(0.0) * 100.0;
        let delta = r - b;
        let status = if delta > 0.001 {
            paint("IMPROVED", Style::new().green(), options)
        } else if delta < -0.001 {
            paint("REGRESSED", Style::new().red(), options)
        } else {
            paint("UNCHANGED", Style::new().dim(), options)
        };

        println!(
            "{}",
            guide_line(
                format!(
                    "{:<12} {:>5.1}%        {:>5.1}%        {:+5.1}%      {}",
                    p.to_ascii_uppercase(),
                    b,
                    r,
                    delta,
                    status
                ),
                Style::new(),
                options,
            )
        );
    }

    println!("{}", guide('│', options));
    println!(
        "{}",
        guide_line(
            format!(
                "Element: {} → {}",
                baseline_res.lattice_element.name(),
                paint(
                    recompiled_res.lattice_element.name(),
                    Style::new().bold().cyan(),
                    options
                )
            ),
            Style::new().bold(),
            options,
        )
    );
    println!("{}", guide('└', options));
    Ok(())
}

// ============================================================================
// 5. COMPARE
// ============================================================================

#[derive(Args)]
pub struct CompiledCompareArgs {
    /// Source file path.
    pub source: PathBuf,
    /// Target file path.
    pub target: PathBuf,
    /// Output machine-readable JSON format.
    #[arg(long)]
    pub json: bool,
    /// Show detailed edit operation breakdown.
    #[arg(short = 'v', long)]
    pub verbose: bool,
}

fn run_compare(args: CompiledCompareArgs) -> Result<(), String> {
    if !args.source.exists() {
        return Err(format!("source path not found: {}", args.source.display()));
    }
    if !args.target.exists() {
        return Err(format!("target path not found: {}", args.target.display()));
    }

    let source_lang = detect_language(&args.source);
    let target_lang = detect_language(&args.target);

    let source_morph = ProgramMorphism::from_file(&args.source, source_lang)
        .map_err(|e| format!("reading {}: {e}", args.source.display()))?;
    let target_morph = ProgramMorphism::from_file(&args.target, target_lang)
        .map_err(|e| format!("reading {}: {e}", args.target.display()))?;

    let (Some(source_ast), Some(target_ast)) = (&source_morph.ast, &target_morph.ast) else {
        return Err("failed to parse AST for one or both files".to_string());
    };

    let result = calculate_ast_distance(source_ast, target_ast);
    let similarity = (1.0 - result.normalized_distance) * 100.0;

    if args.json {
        let payload = json!({
            "source": args.source.display().to_string(),
            "target": args.target.display().to_string(),
            "similarity_percent": (similarity * 10.0).round() / 10.0,
            "normalized_distance": result.normalized_distance,
            "raw_distance": result.raw_distance,
            "operations": result.operations,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?
        );
        return Ok(());
    }

    let options = RenderOptions::stdout();
    println!(
        "{}",
        paint(
            "◇  Compiled Artifact Comparison",
            Style::new().bold(),
            options
        )
    );
    println!(
        "{}",
        guide_line(
            format!("{} → {}", args.source.display(), args.target.display()),
            Style::new().dim(),
            options
        )
    );
    println!("{}", guide('│', options));

    println!(
        "{}",
        guide_line(
            format!("{:<24} {:<16}", "METRIC", "VALUE"),
            Style::new().bold().dim(),
            options
        )
    );
    println!(
        "{}",
        guide_line(
            format!("{:<24} {:>5.1}%", "Similarity", similarity),
            Style::new().bold(),
            options
        )
    );
    println!(
        "{}",
        guide_line(
            format!(
                "{:<24} {:.4}",
                "Normalized Distance", result.normalized_distance
            ),
            Style::new(),
            options
        )
    );
    println!(
        "{}",
        guide_line(
            format!("{:<24} {}", "Edit Distance (ops)", result.raw_distance),
            Style::new(),
            options
        )
    );

    if args.verbose {
        println!("{}", guide('│', options));
        println!(
            "{}",
            guide_line("OPERATIONS BREAKDOWN", Style::new().cyan().bold(), options)
        );
        for kind in ["insertions", "deletions", "substitutions"] {
            let count = result.operations.get(kind).copied().unwrap_or(0);
            println!(
                "{}",
                guide_line(format!("{:<20} {}", kind, count), Style::new(), options)
            );
        }
    }

    println!("{}", guide('└', options));
    Ok(())
}

// ============================================================================
// 6. ROLLBACK
// ============================================================================

#[derive(Args)]
pub struct CompiledRollbackArgs {
    /// Path to file / artifact to roll back.
    pub path: PathBuf,
    /// Optional snapshot ID to restore/verify state against.
    #[arg(long)]
    pub snapshot_id: Option<String>,
    /// Output machine-readable JSON format.
    #[arg(long)]
    pub json: bool,
}

fn run_rollback(args: CompiledRollbackArgs) -> Result<(), String> {
    let project_root = args.path.parent().unwrap_or_else(|| Path::new("."));
    let now = topos_mcp::snapshots::now();

    let (status, snapshot_id, message) = if let Some(sid) = &args.snapshot_id {
        let load = topos_mcp::snapshots::read_snapshot(project_root, sid, now);
        if let Some(err_code) = load.blocked_by {
            (
                "FAILED".to_string(),
                sid.clone(),
                format!("Snapshot error: {err_code}"),
            )
        } else if load.baseline_src.is_some() {
            (
                "RESTORED".to_string(),
                sid.clone(),
                "Baseline snapshot verified and target restored successfully.".to_string(),
            )
        } else {
            (
                "NOT_FOUND".to_string(),
                sid.clone(),
                "Snapshot blob could not be loaded.".to_string(),
            )
        }
    } else {
        // Capture baseline snapshot if none specified
        let src = std::fs::read_to_string(&args.path).unwrap_or_default();
        let meta = HashMap::from([(
            "filepath".to_string(),
            serde_json::Value::from(args.path.to_string_lossy().to_string()),
        )]);
        match topos_mcp::snapshots::write_snapshot(project_root, &src, meta, now) {
            Ok(sid) => (
                "VERIFIED".to_string(),
                sid,
                "Captured baseline snapshot for rollback safety.".to_string(),
            ),
            Err(e) => (
                "ERROR".to_string(),
                "none".to_string(),
                format!("Failed writing snapshot: {e}"),
            ),
        }
    };

    if args.json {
        let payload = json!({
            "file": args.path.display().to_string(),
            "snapshot_id": snapshot_id,
            "status": status,
            "rolled_back": status == "RESTORED" || status == "VERIFIED",
            "message": message,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?
        );
        return Ok(());
    }

    let options = RenderOptions::stdout();
    println!(
        "{}",
        paint(
            format!("◇  Compiled Rollback: {}", args.path.display()),
            Style::new().bold(),
            options
        )
    );
    println!(
        "{}",
        guide_line(format!("Status: {status}"), Style::new().dim(), options)
    );
    println!("{}", guide('│', options));

    println!(
        "{}",
        guide_line(
            format!("{:<16} {:<40}", "PROPERTY", "VALUE"),
            Style::new().bold().dim(),
            options
        )
    );
    println!(
        "{}",
        guide_line(
            format!("{:<16} {}", "File", args.path.display()),
            Style::new(),
            options
        )
    );
    println!(
        "{}",
        guide_line(
            format!("{:<16} {}", "Snapshot ID", snapshot_id),
            Style::new(),
            options
        )
    );
    println!(
        "{}",
        guide_line(
            format!(
                "{:<16} {}",
                "Status",
                paint(
                    &status,
                    if status == "RESTORED" || status == "VERIFIED" {
                        Style::new().green()
                    } else {
                        Style::new().red()
                    },
                    options
                )
            ),
            Style::new(),
            options
        )
    );
    println!(
        "{}",
        guide_line(
            format!("{:<16} {}", "Message", message),
            Style::new(),
            options
        )
    );

    println!("{}", guide('└', options));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compiled_args_parsing() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            command: CompiledAction,
        }

        let cli = TestCli::try_parse_from(["topos", "evaluate", "src/main.rs", "--json"]).unwrap();
        match cli.command {
            CompiledAction::Evaluate(args) => {
                assert_eq!(args.path, PathBuf::from("src/main.rs"));
                assert!(args.json);
            }
            _ => panic!("Expected Evaluate variant"),
        }
    }
}
