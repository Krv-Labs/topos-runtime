//! Compiled tools suite — offline, batch, and compiled agent refactor loop.

use std::collections::HashMap;
use std::process::Command;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use topos_engine::core::morphism::ProgramMorphism;
use topos_engine::evaluation::policies::base::Priority;
use topos_engine::functors::profunctors::ast::compare::calculate_ast_distance;

use crate::evaluation::{
    classify_code_string, classify_file, detect_language, resolve_override_for_root,
};
use crate::formatting::{
    error_md, render_evaluation_md, to_evaluation_result, to_tool_result, EvalResultOptions,
};
use crate::refactor_targets::build_refactor_targets;
use crate::schemas::{
    lattice_to_str, resolve_priority, AssessmentStatus, CompiledEvaluateInput,
    CompiledIdentifyOpportunitiesInput, CompiledOpportunity, CompiledPlanResult, CompiledPlanStep,
    CompiledProposePlanInput, CompiledRecompileInput, CompiledRecompileResult,
    CompiledRollbackInput, CompiledRollbackResult, CompiledVerifyGainsInput,
    CompiledVerifyGainsResult, EvaluationResult, LatticeElement, PrioritySource,
};
use crate::security::{
    composable_default_root, read_safe_utf8_file, resolve_project_path, resolve_within_root,
};
use crate::server::ToposServer;
use crate::snapshots::{now as snapshot_now, read_snapshot, sha256_hex, write_snapshot};

fn err_eval(
    description: &str,
    priority: Priority,
    source: PrioritySource,
    msg: String,
) -> CallToolResult {
    let model = EvaluationResult::error_result(description, priority, source, msg);
    to_tool_result(&model, error_md(&model))
}

#[tool_router(router = compiled_tools_router, vis = "pub(crate)")]
impl ToposServer {
    /// Score code string or file in compiled execution mode on the quality lattice.
    #[tool(
        name = "topos_compiled_evaluate",
        annotations(
            title = "Topos Compiled Evaluate",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    pub fn topos_compiled_evaluate(
        &self,
        Parameters(params): Parameters<CompiledEvaluateInput>,
    ) -> CallToolResult {
        let (priority, priority_source) = resolve_priority(params.preferences.as_ref());
        let prefs = match params.preferences.as_ref().map(|p| p.to_preferences()) {
            Some(Err(exc)) => return err_eval("compiled evaluate", priority, priority_source, exc),
            Some(Ok(p)) => Some(p),
            None => None,
        };
        if params.filepath.is_none() && params.code.is_none() {
            return err_eval(
                "compiled evaluate",
                priority,
                priority_source,
                "Provide at least one of `filepath` or `code`.".to_string(),
            );
        }

        if let Some(ref filepath) = params.filepath {
            let resolved = match resolve_within_root(filepath) {
                Ok(path) => path,
                Err(err) => return err_eval("compiled evaluate", priority, priority_source, err),
            };
            let default_root = composable_default_root(&resolved);
            let override_dir =
                resolve_override_for_root(params.gitnexus_dir.as_deref(), &default_root);
            let (result, dep_graph, warning) = match classify_file(
                &resolved,
                priority,
                override_dir.as_deref().map(std::path::Path::new),
            ) {
                Ok(tuple) => tuple,
                Err(err) => return err_eval("compiled evaluate", priority, priority_source, err),
            };
            let coupling_available = dep_graph.is_some();
            let mut opts = EvalResultOptions::new();
            opts.preferences = prefs.as_ref();
            opts.priority_source = priority_source;
            if let Some(w) = warning {
                opts.warnings.push(w);
            }

            let model = to_evaluation_result(&result, coupling_available, opts);
            let md = render_evaluation_md(&model, None, true);
            to_tool_result(&model, md)
        } else if let Some(ref code) = params.code {
            let result = match classify_code_string(code, "python", priority) {
                Ok(res) => res,
                Err(err) => return err_eval("compiled evaluate", priority, priority_source, err),
            };

            let mut opts = EvalResultOptions::new();
            opts.preferences = prefs.as_ref();
            opts.priority_source = priority_source;

            let model = to_evaluation_result(&result, false, opts);
            let md = render_evaluation_md(&model, None, true);
            to_tool_result(&model, md)
        } else {
            err_eval(
                "compiled evaluate",
                priority,
                priority_source,
                "Invalid parameters".to_string(),
            )
        }
    }

    /// Identify refactoring and quality optimization opportunities for a target file.
    #[tool(
        name = "topos_compiled_identify_opportunities",
        annotations(
            title = "Topos Compiled Identify Opportunities",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    pub fn topos_compiled_identify_opportunities(
        &self,
        Parameters(params): Parameters<CompiledIdentifyOpportunitiesInput>,
    ) -> CallToolResult {
        let (priority, priority_source) = resolve_priority(params.preferences.as_ref());
        let resolved = match resolve_within_root(&params.filepath) {
            Ok(path) => path,
            Err(err) => return err_eval("identify opportunities", priority, priority_source, err),
        };

        let default_root = composable_default_root(&resolved);
        let override_dir = resolve_override_for_root(params.gitnexus_dir.as_deref(), &default_root);
        let (classification, _, _) = match classify_file(
            &resolved,
            priority,
            override_dir.as_deref().map(std::path::Path::new),
        ) {
            Ok(res) => res,
            Err(err) => return err_eval("identify opportunities", priority, priority_source, err),
        };

        let limit = params.limit.unwrap_or(10);
        let targets = build_refactor_targets(
            &params.filepath,
            &classification,
            &[],
            &HashMap::new(),
            None,
            limit,
        );
        let opportunities: Vec<CompiledOpportunity> = targets
            .into_iter()
            .map(|t| {
                let l_start = t.line_start.unwrap_or(1);
                let l_end = t.line_end.unwrap_or(l_start);
                let sym_str = t.symbol.as_deref().unwrap_or(&t.metric);
                CompiledOpportunity {
                    kind: t.kind,
                    location: format!("{}:{}", l_start, l_end),
                    line_start: l_start,
                    line_end: l_end,
                    score: t.current_value.unwrap_or(0.0),
                    suggestion: format!("Address `{sym_str}` in `{}`", params.filepath),
                }
            })
            .collect();

        let mut md = format!(
            "# Compiled Refactoring Opportunities: `{}`\n\nFound {} opportunities:\n\n",
            params.filepath,
            opportunities.len()
        );
        for (idx, opp) in opportunities.iter().enumerate() {
            md.push_str(&format!(
                "{}. **{}** at `{}` (Score: {:.2})\n   *Suggestion:* {}\n\n",
                idx + 1,
                opp.kind,
                opp.location,
                opp.score,
                opp.suggestion
            ));
        }

        to_tool_result(&opportunities, md)
    }

    /// Propose a structured refactoring plan targeting specific lattice levels.
    #[tool(
        name = "topos_compiled_propose_plan",
        annotations(
            title = "Topos Compiled Propose Plan",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    pub fn topos_compiled_propose_plan(
        &self,
        Parameters(params): Parameters<CompiledProposePlanInput>,
    ) -> CallToolResult {
        let (priority, priority_source) = resolve_priority(params.preferences.as_ref());
        let resolved = match resolve_within_root(&params.filepath) {
            Ok(path) => path,
            Err(err) => return err_eval("propose plan", priority, priority_source, err),
        };

        let default_root = composable_default_root(&resolved);
        let override_dir = resolve_override_for_root(params.gitnexus_dir.as_deref(), &default_root);
        let (classification, _, _) = match classify_file(
            &resolved,
            priority,
            override_dir.as_deref().map(std::path::Path::new),
        ) {
            Ok(res) => res,
            Err(err) => return err_eval("propose plan", priority, priority_source, err),
        };

        let current_verdict = lattice_to_str(classification.lattice_element);
        let target_verdict = params.target_verdict.unwrap_or(LatticeElement::IDEAL);

        let mut steps = Vec::new();
        let targets = build_refactor_targets(
            &params.filepath,
            &classification,
            &[],
            &HashMap::new(),
            None,
            5,
        );

        for (i, target) in targets.iter().enumerate() {
            let l_start = target.line_start.unwrap_or(1);
            let sym_str = target.symbol.as_deref().unwrap_or(&target.metric);
            steps.push(CompiledPlanStep {
                step: i + 1,
                target_pillar: target.kind.clone(),
                action: format!("Address `{sym_str}` at line {l_start}"),
                expected_gain: format!(
                    "Improve {} score from {:.2} (Threshold: {:.2})",
                    target.metric,
                    target.current_value.unwrap_or(0.0),
                    target.threshold.unwrap_or(1.0)
                ),
            });
        }

        if steps.is_empty() {
            steps.push(CompiledPlanStep {
                step: 1,
                target_pillar: "ALL".to_string(),
                action: "Maintain existing passing gates and verify structural stability."
                    .to_string(),
                expected_gain: "Ensure code remains at current passing level.".to_string(),
            });
        }

        let plan_id = format!("plan-{}", &sha256_hex(&params.filepath)[..8]);
        let estimated_iterations = steps.len();

        let plan_result = CompiledPlanResult {
            plan_id: plan_id.clone(),
            filepath: params.filepath.clone(),
            current_verdict,
            target_verdict,
            steps: steps.clone(),
            estimated_iterations,
        };

        let mut md = format!(
            "# Compiled Refactoring Plan: `{}`\n\n- **Plan ID:** `{}`\n- **Current Verdict:** `{:?}`\n- **Target Verdict:** `{:?}`\n\n## Steps:\n\n",
            params.filepath, plan_id, current_verdict, target_verdict
        );

        for step in &steps {
            md.push_str(&format!(
                "{}. **Step {} [{}]**: {}\n   - *Expected Gain:* {}\n\n",
                step.step, step.step, step.target_pillar, step.action, step.expected_gain
            ));
        }

        to_tool_result(&plan_result, md)
    }

    /// Execute recompilation and capture baseline snapshot for verification.
    #[tool(
        name = "topos_compiled_recompile",
        annotations(
            title = "Topos Compiled Recompile",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    pub fn topos_compiled_recompile(
        &self,
        Parameters(params): Parameters<CompiledRecompileInput>,
    ) -> CallToolResult {
        let (priority, priority_source) = resolve_priority(None);
        let resolved = match resolve_within_root(&params.filepath) {
            Ok(path) => path,
            Err(err) => return err_eval("recompile", priority, priority_source, err),
        };

        let source = match read_safe_utf8_file(&resolved.to_string_lossy()) {
            Ok(s) => s,
            Err(err) => return err_eval("recompile", priority, priority_source, err),
        };

        let project_root = resolve_project_path(&params.filepath)
            .map(|(_, proj)| proj)
            .unwrap_or_else(|_| composable_default_root(&resolved));

        let meta = std::collections::HashMap::from([(
            "filepath".to_string(),
            serde_json::Value::from(params.filepath.as_str()),
        )]);

        let snapshot_id = match write_snapshot(&project_root, &source, meta, snapshot_now()) {
            Ok(id) => id,
            Err(err) => {
                return err_eval(
                    "recompile",
                    priority,
                    priority_source,
                    format!("Failed to create snapshot: {err}"),
                )
            }
        };

        let res = CompiledRecompileResult {
            filepath: params.filepath.clone(),
            snapshot_id: snapshot_id.clone(),
            status: "compiled".to_string(),
            plan_id: params.plan_id.clone(),
            step: params.step,
            message: format!(
                "Recompiled and captured snapshot `{snapshot_id}` for `{}`.",
                params.filepath
            ),
        };

        let md = format!(
            "# Compiled Recompile Status: SUCCESS\n\n- **File:** `{}`\n- **Snapshot ID:** `{}`\n- **Plan ID:** `{:?}`\n- **Step:** `{:?}`\n\nCaptured content snapshot for gain verification.",
            params.filepath, snapshot_id, params.plan_id, params.step
        );

        to_tool_result(&res, md)
    }

    /// Verify quality gains against pre-compilation baseline or snapshot.
    #[tool(
        name = "topos_compiled_verify_gains",
        annotations(
            title = "Topos Compiled Verify Gains",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    pub fn topos_compiled_verify_gains(
        &self,
        Parameters(params): Parameters<CompiledVerifyGainsInput>,
    ) -> CallToolResult {
        let (priority, priority_source) = resolve_priority(params.preferences.as_ref());
        let resolved = match resolve_within_root(&params.filepath) {
            Ok(path) => path,
            Err(err) => return err_eval("verify gains", priority, priority_source, err),
        };

        let current_src = match read_safe_utf8_file(&resolved.to_string_lossy()) {
            Ok(s) => s,
            Err(err) => return err_eval("verify gains", priority, priority_source, err),
        };

        let project_root = resolve_project_path(&params.filepath)
            .map(|(root, _)| root)
            .unwrap_or_else(|_| composable_default_root(&resolved));

        let baseline_src = if let Some(ref snap_id) = params.snapshot_id {
            let load = read_snapshot(&project_root, snap_id, snapshot_now());
            match load.baseline_src {
                Some(src) => src,
                None => {
                    let msg = load.blocked_by.unwrap_or("snapshot_not_found").to_string();
                    return err_eval(
                        "verify gains",
                        priority,
                        priority_source,
                        format!("Snapshot error: {msg}"),
                    );
                }
            }
        } else {
            let ref_name = params.baseline_ref.as_deref().unwrap_or("HEAD");
            let output = Command::new("git")
                .arg("show")
                .arg(format!("{ref_name}:{}", params.filepath))
                .current_dir(&project_root)
                .output();

            match output {
                Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
                _ => current_src.clone(),
            }
        };

        let lang = detect_language(&resolved);
        let morphism_before = ProgramMorphism::new(&baseline_src, lang);
        let morphism_after = ProgramMorphism::new(&current_src, lang);

        let ast_distance = match (&morphism_before.ast, &morphism_after.ast) {
            (Some(ast_before), Some(ast_after)) => {
                calculate_ast_distance(ast_before, ast_after).normalized_distance
            }
            _ => 0.0,
        };

        let default_root = composable_default_root(&resolved);
        let override_dir = resolve_override_for_root(params.gitnexus_dir.as_deref(), &default_root);

        let class_before = match classify_code_string(&baseline_src, lang, priority) {
            Ok(c) => c,
            Err(err) => return err_eval("verify gains", priority, priority_source, err),
        };
        let class_after = match classify_file(
            &resolved,
            priority,
            override_dir.as_deref().map(std::path::Path::new),
        ) {
            Ok(tuple) => tuple.0,
            Err(err) => return err_eval("verify gains", priority, priority_source, err),
        };

        let verdict_before = lattice_to_str(class_before.summary());
        let verdict_after = lattice_to_str(class_after.summary());

        let mut score_deltas = HashMap::new();
        for (k, v_after) in &class_after.scores {
            let v_before = class_before.scores.get(k).copied().unwrap_or(0.0);
            score_deltas.insert(k.clone(), v_after - v_before);
        }

        let status = if class_after.summary().bits() > class_before.summary().bits() {
            AssessmentStatus::IMPROVEMENT
        } else if class_after.summary().bits() == class_before.summary().bits() {
            if score_deltas.values().any(|&d| d > 0.0) {
                AssessmentStatus::IMPROVEMENT_SCORE
            } else if score_deltas.values().any(|&d| d < 0.0) {
                AssessmentStatus::REGRESSION_SCORE
            } else {
                AssessmentStatus::LATERAL_MOVE
            }
        } else {
            AssessmentStatus::REGRESSION
        };

        let accepted = matches!(
            status,
            AssessmentStatus::IMPROVEMENT | AssessmentStatus::IMPROVEMENT_SCORE
        );
        let requires_rollback = matches!(
            status,
            AssessmentStatus::REGRESSION
                | AssessmentStatus::REGRESSION_SCORE
                | AssessmentStatus::SUSPICIOUS_NO_STRUCTURAL_CHANGE
        );

        let result = CompiledVerifyGainsResult {
            filepath: params.filepath.clone(),
            status,
            accepted,
            verdict_before,
            verdict_after,
            ast_distance: Some(ast_distance),
            score_deltas,
            requires_rollback,
        };

        let md = format!(
            "# Compiled Verification Gains: `{}`\n\n- **Status:** `{:?}`\n- **Accepted:** `{}`\n- **Verdict Before:** `{:?}`\n- **Verdict After:** `{:?}`\n- **AST Distance:** `{:.4}`\n- **Requires Rollback:** `{}`\n",
            params.filepath, status, accepted, verdict_before, verdict_after, ast_distance, requires_rollback
        );

        to_tool_result(&result, md)
    }

    /// Roll back changes to captured snapshot if quality regressed or verification failed.
    #[tool(
        name = "topos_compiled_rollback",
        annotations(
            title = "Topos Compiled Rollback",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    pub fn topos_compiled_rollback(
        &self,
        Parameters(params): Parameters<CompiledRollbackInput>,
    ) -> CallToolResult {
        let (priority, priority_source) = resolve_priority(None);
        let resolved = match resolve_within_root(&params.filepath) {
            Ok(path) => path,
            Err(err) => return err_eval("rollback", priority, priority_source, err),
        };

        let project_root = resolve_project_path(&params.filepath)
            .map(|(root, _)| root)
            .unwrap_or_else(|_| composable_default_root(&resolved));

        if let Some(ref snap_id) = params.snapshot_id {
            let load = read_snapshot(&project_root, snap_id, snapshot_now());
            if let Some(ref baseline_src) = load.baseline_src {
                if let Err(err) = std::fs::write(&resolved, baseline_src) {
                    return err_eval(
                        "rollback",
                        priority,
                        priority_source,
                        format!("Failed to restore file: {err}"),
                    );
                }
                let res = CompiledRollbackResult {
                    filepath: params.filepath.clone(),
                    snapshot_id: Some(snap_id.clone()),
                    status: "restored".to_string(),
                    reason: params.reason.clone(),
                    message: format!(
                        "Successfully rolled back `{}` to snapshot `{snap_id}`.",
                        params.filepath
                    ),
                };

                let md = format!(
                    "# Compiled Rollback: RESTORED\n\n- **File:** `{}`\n- **Snapshot ID:** `{}`\n- **Reason:** `{:?}`\n\nRestored file content from snapshot baseline.",
                    params.filepath, snap_id, params.reason
                );

                to_tool_result(&res, md)
            } else {
                let msg = load.blocked_by.unwrap_or("snapshot_not_found").to_string();
                err_eval(
                    "rollback",
                    priority,
                    priority_source,
                    format!("Snapshot error: {msg}"),
                )
            }
        } else {
            let res = CompiledRollbackResult {
                filepath: params.filepath.clone(),
                snapshot_id: None,
                status: "no_snapshot".to_string(),
                reason: params.reason.clone(),
                message: format!(
                    "No snapshot_id provided for rollback of `{}`.",
                    params.filepath
                ),
            };

            let md = format!(
                "# Compiled Rollback: NO SNAPSHOT\n\n- **File:** `{}`\n\nNo snapshot ID provided.",
                params.filepath
            );

            to_tool_result(&res, md)
        }
    }
}
