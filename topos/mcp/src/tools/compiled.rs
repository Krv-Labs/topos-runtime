//! Compiled-binary optimizer tools.
//!
//! Three tools only. `approve` is CLI-only and must never appear here.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{tool, tool_router};
use topos_engine::config::load_topos_config;
use topos_engine::optimization::approval::ApprovedPlan;
use topos_engine::optimization::artifact_store::ArtifactStore;
use topos_engine::optimization::ops::{apply_approved, prepare_plan, PlanRequest};

use crate::formatting::to_tool_result;
use crate::schemas::{
    CompiledApplyInput, CompiledApplyResult, CompiledCandidateRow, CompiledPlanInput,
    CompiledPlanResult, CompiledRollbackInput, CompiledRollbackResult, CompiledVariantProbe,
};
use crate::security::resolve_project_path;
use crate::server::ToposServer;

#[tool_router(router = compiled_router, vis = "pub(crate)")]
impl ToposServer {
    /// Probe clang and emit a compiled-optimization plan. Builds and runs
    /// nothing. Approval is CLI-only (`topos compiled approve --by`); this
    /// tool never signs a plan.
    #[tool(
        name = "topos_compiled_plan",
        annotations(
            title = "Topos Compiled Plan",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    pub fn topos_compiled_plan(
        &self,
        Parameters(params): Parameters<CompiledPlanInput>,
    ) -> CallToolResult {
        let (target, _) = match resolve_project_path(&params.target) {
            Ok(pair) => pair,
            Err(err) => return text_error(err),
        };
        let prepared = match prepare_plan(PlanRequest {
            target,
            variants: params.variants.unwrap_or_default(),
            min_speedup: params.min_speedup.unwrap_or(5.0),
            max_size_increase: params.max_size_increase.unwrap_or(10.0),
            runs: params.runs,
            warmup: params.warmup,
            run_args: params.run_args.unwrap_or_default(),
            plan_path_hint: "plan.json".into(),
        }) {
            Ok(p) => p,
            Err(err) => return text_error(err),
        };
        let plan = match serde_json::to_string_pretty(&prepared.plan) {
            Ok(s) => s,
            Err(err) => return text_error(err.to_string()),
        };
        let variants: Vec<CompiledVariantProbe> = prepared
            .probes
            .iter()
            .map(|p| CompiledVariantProbe {
                id: p.id.clone(),
                buildable: p.buildable,
                reason: p.reason.clone(),
                build_command: p.build_command.clone(),
                run_command: p.run_command.clone(),
            })
            .collect();
        let result = CompiledPlanResult {
            digest: prepared.plan.digest.clone(),
            plan,
            variants: variants.clone(),
            next_step: prepared.next_step.clone(),
        };
        let mut md = format!(
            "# Compiled plan\n\n**digest:** `{}`\n\n{}\n\n| Variant | Buildable | Reason |\n|---|---|---|\n",
            result.digest, result.next_step
        );
        for v in &variants {
            md.push_str(&format!("| {} | {} | {} |\n", v.id, v.buildable, v.reason));
        }
        to_tool_result(&result, md)
    }

    /// Measure an approved plan and promote a winner only if SPEED and SIZE
    /// both pass. Refuses an unapproved plan. No silent regressions.
    #[tool(
        name = "topos_compiled_apply",
        annotations(
            title = "Topos Compiled Apply",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    pub fn topos_compiled_apply(
        &self,
        Parameters(params): Parameters<CompiledApplyInput>,
    ) -> CallToolResult {
        let (plan_path, _) = match resolve_project_path(&params.plan_path) {
            Ok(pair) => pair,
            Err(err) => return text_error(err),
        };
        let text = match fs::read_to_string(&plan_path) {
            Ok(t) => t,
            Err(err) => return text_error(err.to_string()),
        };
        let approved = match ApprovedPlan::from_json(&text) {
            Ok(a) => a,
            Err(err) => return text_error(err.to_string()),
        };
        let config = load_topos_config(&plan_path);
        if let Some(err) = config.compiled_error {
            return text_error(err);
        }
        let project_root = config
            .root
            .clone()
            .or_else(|| plan_path.parent().map(PathBuf::from))
            .unwrap_or(plan_path.clone());
        let outcome = match apply_approved(&approved, &project_root, unix_now()) {
            Ok(o) => o,
            Err(err) => return text_error(err),
        };
        let candidates: Vec<CompiledCandidateRow> = outcome
            .report
            .candidates
            .iter()
            .map(|c| CompiledCandidateRow {
                id: c.id.clone(),
                variant: c.variant.clone(),
                speedup_pct: c.speedup_pct,
                noise: c.noise.clone(),
                speed_satisfied: c.speed_satisfied,
                size_satisfied: c.size_satisfied,
            })
            .collect();
        let promoted = outcome.promoted.as_ref().map(|p| p.display().to_string());
        let message = if promoted.is_some() {
            "promoted".to_string()
        } else {
            "NO CANDIDATE MET THRESHOLDS".to_string()
        };
        let result = CompiledApplyResult {
            run_id: outcome.report.run_id.clone(),
            promoted: promoted.clone(),
            message: message.clone(),
            candidates: candidates.clone(),
        };
        let mut md = format!("# Compiled apply\n\n**run:** `{}`\n\n", result.run_id);
        for c in &candidates {
            md.push_str(&format!(
                "- {} {:+.1}% {} speed={} size={}\n",
                c.variant, c.speedup_pct, c.noise, c.speed_satisfied, c.size_satisfied
            ));
        }
        md.push_str(&format!("\n**{message}**\n"));
        if promoted.is_some() {
            to_tool_result(&result, md)
        } else {
            let mut call = CallToolResult::error(vec![ContentBlock::text(md)]);
            call.structured_content = serde_json::to_value(&result).ok();
            call
        }
    }

    /// Restore the pre-apply baseline binary for a compiled run.
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
        let (project_root, _) = match resolve_project_path(&params.project_root) {
            Ok(pair) => pair,
            Err(err) => return text_error(err),
        };
        let store = match ArtifactStore::open(&project_root) {
            Ok(s) => s,
            Err(err) => return text_error(err.to_string()),
        };
        let outcome = match store.rollback_recorded(params.run_id.as_deref()) {
            Ok(r) => r,
            Err(err) => return text_error(err.to_string()),
        };
        let result = CompiledRollbackResult {
            bytes_restored: outcome.bytes_restored,
            verified: outcome.verified,
        };
        let md = format!(
            "# Compiled rollback\n\nrestored {} bytes, verified={}\n",
            result.bytes_restored, result.verified
        );
        if result.verified {
            to_tool_result(&result, md)
        } else {
            CallToolResult::error(vec![ContentBlock::text(
                "rollback read-back did not match stored baseline",
            )])
        }
    }
}

fn text_error(err: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(err.into())])
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

    #[test]
    fn compiled_approve_is_not_an_mcp_tool() {
        let names: Vec<String> = ToposServer::new()
            .list_tool_defs()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect();
        for name in [
            "topos_compiled_plan",
            "topos_compiled_apply",
            "topos_compiled_rollback",
        ] {
            assert!(
                names.iter().any(|n| n == name),
                "missing {name} in {names:?}"
            );
        }
        assert!(
            !names.iter().any(|n| n.contains("approve")),
            "approve leaked onto MCP: {names:?}"
        );
    }
}
