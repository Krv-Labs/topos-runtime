//! Human approval consumes the plan it signs.
//!
//! `ApprovedPlan::approve(plan, by, note, now)` takes ownership of the plan,
//! so an approval can never be paired with a different plan.
//! `approved_at` comes from an injected `now: f64` (unix seconds), matching
//! `snapshots::write_snapshot`, so tests do not sleep. There is no `--yes`
//! flag and no `require_human_approval: bool` to flip.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::plan::{OptimizationPlan, PlanError};

#[derive(Debug, Clone, PartialEq)]
pub struct ApprovedPlan {
    plan: OptimizationPlan,
    approved_by: String,
    note: Option<String>,
    approved_at: f64,
}

impl ApprovedPlan {
    pub fn approve(
        plan: OptimizationPlan,
        by: impl Into<String>,
        note: Option<String>,
        now: f64,
    ) -> Result<Self, ApprovalError> {
        if !plan.verify_digest() {
            return Err(ApprovalError::DigestMismatch);
        }
        let by = by.into();
        if by.trim().is_empty() {
            return Err(ApprovalError::MissingIdentity);
        }
        Ok(Self {
            plan,
            approved_by: by,
            note,
            approved_at: now,
        })
    }

    pub fn plan(&self) -> &OptimizationPlan {
        &self.plan
    }

    pub fn approved_by(&self) -> &str {
        &self.approved_by
    }

    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    pub fn approved_at(&self) -> f64 {
        self.approved_at
    }

    pub fn to_record(&self) -> ApprovedPlanRecord {
        ApprovedPlanRecord {
            plan: self.plan.clone(),
            approved_by: self.approved_by.clone(),
            note: self.note.clone(),
            approved_at: self.approved_at,
        }
    }

    pub fn from_record(record: ApprovedPlanRecord) -> Result<Self, ApprovalError> {
        Self::approve(
            record.plan,
            record.approved_by,
            record.note,
            record.approved_at,
        )
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(&self.to_record()).map_err(|e| e.to_string())
    }

    pub fn from_json(text: &str) -> Result<Self, ApprovalError> {
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| ApprovalError::Json(e.to_string()))?;
        if value.get("approved_by").is_none() {
            return Err(ApprovalError::NotApproved);
        }
        let record: ApprovedPlanRecord =
            serde_json::from_value(value).map_err(|e| ApprovalError::Json(e.to_string()))?;
        Self::from_record(record)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApprovedPlanRecord {
    pub plan: OptimizationPlan,
    pub approved_by: String,
    pub note: Option<String>,
    pub approved_at: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalError {
    DigestMismatch,
    MissingIdentity,
    NotApproved,
    Json(String),
    Plan(PlanError),
}

impl fmt::Display for ApprovalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApprovalError::DigestMismatch => {
                write!(f, "plan digest does not match canonical contents")
            }
            ApprovalError::MissingIdentity => {
                write!(f, "approval requires --by <identity>")
            }
            ApprovalError::NotApproved => {
                write!(
                    f,
                    "plan is not approved; run `topos compiled approve --by <identity>`"
                )
            }
            ApprovalError::Json(err) => write!(f, "approval JSON: {err}"),
            ApprovalError::Plan(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for ApprovalError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optimization::variant::FlagVariant;
    use std::path::PathBuf;

    fn plan() -> OptimizationPlan {
        OptimizationPlan::new(crate::optimization::plan::PlanSpec {
            source: Some(PathBuf::from("a.c")),
            build_command: None,
            run_command: vec!["{output}".into()],
            variants: vec![FlagVariant::O2],
            min_speedup_pct: 5.0,
            max_size_increase_pct: 10.0,
            max_rss_increase_pct: 10.0,
            warmup_runs: 2,
            measured_runs: 10,
            timeout_ms: 60_000,
        })
        .unwrap()
    }

    #[test]
    fn approval_timestamp_reflects_the_injected_clock() {
        let a = ApprovedPlan::approve(plan(), "reviewer", None, 1.0).unwrap();
        let b = ApprovedPlan::approve(plan(), "reviewer", None, 100_000.0).unwrap();
        assert_ne!(a.approved_at(), b.approved_at());
        assert_eq!(a.approved_at(), 1.0);
        assert_eq!(b.approved_at(), 100_000.0);
        let literal = "2026-08-20T19:00:00Z";
        assert_ne!(format!("{}", a.approved_at()), literal);
        assert_ne!(format!("{}", b.approved_at()), literal);
    }

    #[test]
    fn empty_identity_is_rejected() {
        let err = ApprovedPlan::approve(plan(), "  ", None, 1.0).unwrap_err();
        assert!(matches!(err, ApprovalError::MissingIdentity));
    }

    #[test]
    fn a_bare_plan_is_not_an_approval() {
        let json = serde_json::to_string(&plan()).unwrap();
        let err = ApprovedPlan::from_json(&json).unwrap_err();
        assert!(matches!(err, ApprovalError::NotApproved));
    }
}
