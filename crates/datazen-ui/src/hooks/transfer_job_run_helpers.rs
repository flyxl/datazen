//! The carried half of `hooks/useTransferJobRun.ts` (T438, split):
//! the failure classification, the two read-model projections and
//! the lib predicates the hook consults.
//!
//! # What is carried, and for whom
//!
//! - `isBackendScopeRejection` / `isPipelineBudgetInterception` /
//!   `isPlanConsumedRejection` / `isMigrationJobInFlight` /
//!   `readJobCounter` live in `lib/migrationJobVerdict.ts` —
//!   **T561, segment 2**, not landed. They are carried here verbatim
//!   until T561 lands and absorbs them (the same convention
//!   `migration_job_hydration_helpers.rs` records for T559).
//! - `isStalePlanError` lives in `lib/migrationJobHydration.ts`
//!   (T376's own file); it is carried here for the same reason.
//! - `classifyTransferRunFailure` is the hook file's own export —
//!   this file's, permanently.
//!
//! Known deltas: the regexes are the `regex` crate's spelling of
//! the React ones (same patterns, same case-insensitivity); the
//! Counter parse accepts the JSON forms the wire can carry.

// No production consumer yet (the data-transfer window track wires
// the machine); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::commands::transfer_jobs::TransferApplyJobView;
use crate::hooks::migration_job_hydration_helpers::{EffectOutcome, MigrationJobViewVm};

/// `TransferRunFailureKind`: how a command refusal must be phrased.
/// Each arm is fail-closed on purpose: backend-scope and
/// pipeline-budget are *refusals*, not warnings, and degrading them
/// into a soft failure would hide a rule the backend enforced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransferRunFailureKind {
    /// Backend scope: endpoints could not prove they are the local
    /// desktop backend.
    BackendScope,
    /// The 8 MiB pipeline budget refused the write on purpose.
    PipelineBudget,
    /// This planId was already spent by an earlier apply Job.
    PlanConsumed,
    /// Plan expired / source drifted / permission changed —
    /// re-prepare.
    StalePlan,
    Other,
}

/// `TransferRunFailure`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TransferRunFailure {
    pub kind: TransferRunFailureKind,
    pub message: String,
}

/// The command error the classifier reads: the backend's own `code`
/// when it sent one (the `ApiError` shape) and its message.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TransferCommandError {
    pub code: Option<String>,
    pub message: String,
}

/// `STALE_PLAN_ERROR_CODES` (`lib/migrationJobHydration.ts`).
pub(crate) const STALE_PLAN_ERROR_CODES: [&str; 3] =
    ["PlanStale", "SourceChanged", "PermissionDenied"];

/// `isStalePlanError`: the code membership (an `ApiError`'s code, an
/// `Error`'s `.code`, or the message's own spelling of one).
pub(crate) fn is_stale_plan_error(error: &TransferCommandError) -> bool {
    if let Some(code) = &error.code {
        if STALE_PLAN_ERROR_CODES.contains(&code.as_str()) {
            return true;
        }
    }
    STALE_PLAN_ERROR_CODES.iter().any(|c| error.message.contains(c))
}

/// `isPipelineBudgetInterception`.
pub(crate) fn is_pipeline_budget_interception(message: &str) -> bool {
    regex::Regex::new(r"(?i)pipeline\s*budget|pipelinebudget|budget\s*exceed|8\s*MiB|8388608")
        .expect("the budget pattern compiles")
        .is_match(message)
}

/// `isPlanConsumedRejection`: planId single-consumption.
pub(crate) fn is_plan_consumed_rejection(message: &str) -> bool {
    regex::Regex::new(r"(?i)already\s*consumed|PlanAlreadyConsumed|re-review the migration to mint a new plan")
        .expect("the consumed pattern compiles")
        .is_match(message)
}

/// `isBackendScopeRejection`.
pub(crate) fn is_backend_scope_rejection(message: &str) -> bool {
    regex::Regex::new(r"(?i)backend\s*scope|local-desktop-backend|scope")
        .expect("the scope pattern compiles")
        .is_match(message)
}

/// `isMigrationJobInFlight`.
pub(crate) fn is_migration_job_in_flight(state: &str) -> bool {
    state == "queued" || state == "running"
}

/// `isActiveJobState` (the hook file's own).
pub(crate) fn is_active_job_state(state: &str) -> bool {
    is_migration_job_in_flight(state)
}

/// `readJobCounter`, over the JSON forms the wire can carry
/// (`number | string | null | undefined`): a finite `>= 0` number
/// truncates; a digits-only string parses; anything else is `None`.
pub(crate) fn read_job_counter(value: &Value) -> Option<u64> {
    if let Some(number) = value.as_f64() {
        return if number.is_finite() && number >= 0.0 {
            Some(number.trunc() as u64)
        } else {
            None
        };
    }
    if let Some(text) = value.as_str() {
        let trimmed = text.trim();
        if trimmed.is_empty() || !trimmed.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        return trimmed.parse::<u64>().ok();
    }
    None
}

/// `classifyTransferRunFailure` — the hook file's own export: the
/// four specific refusals in their order, then `other`.
pub(crate) fn classify_transfer_run_failure(
    error: &TransferCommandError,
) -> TransferRunFailure {
    let message = error.message.clone();
    if is_backend_scope_rejection(&message) {
        return TransferRunFailure { kind: TransferRunFailureKind::BackendScope, message };
    }
    if is_pipeline_budget_interception(&message) {
        return TransferRunFailure { kind: TransferRunFailureKind::PipelineBudget, message };
    }
    if is_plan_consumed_rejection(&message) {
        return TransferRunFailure { kind: TransferRunFailureKind::PlanConsumed, message };
    }
    if is_stale_plan_error(error) {
        return TransferRunFailure { kind: TransferRunFailureKind::StalePlan, message };
    }
    TransferRunFailure { kind: TransferRunFailureKind::Other, message }
}

/// `isAmbiguousApplySubmissionError`: the apply may have been
/// accepted even though its receipt was lost (the code triple or the
/// transport phrasing).
pub(crate) fn is_ambiguous_apply_submission_error(error: &TransferCommandError) -> bool {
    if let Some(code) = &error.code {
        if matches!(code.as_str(), "OutcomeUnknown" | "ServiceUnavailable" | "Timeout") {
            return true;
        }
    }
    regex::Regex::new(
        r"(?i)timed?\s*out|timeout|transport|connection\s+(?:closed|lost)|ack(?:nowledg(?:e)?ment)?\s+lost|outcome\s+unknown",
    )
    .expect("the ambiguity pattern compiles")
    .is_match(&error.message)
}

/// The `EffectOutcome` wire spelling (`notStarted` / `completed` /
/// `rolledBack` / `partiallyApplied` / `unknown`).
pub(crate) fn effect_outcome_as_str(outcome: Option<EffectOutcome>) -> Option<&'static str> {
    match outcome {
        Some(EffectOutcome::NotStarted) => Some("notStarted"),
        Some(EffectOutcome::Completed) => Some("completed"),
        Some(EffectOutcome::RolledBack) => Some("rolledBack"),
        Some(EffectOutcome::PartiallyApplied) => Some("partiallyApplied"),
        Some(EffectOutcome::Unknown) => Some("unknown"),
        None => None,
    }
}

/// The `JobState` wire spelling.
pub(crate) fn job_state_as_str(state: &crate::hooks::migration_job_hydration_helpers::JobState) -> &'static str {
    use crate::hooks::migration_job_hydration_helpers::JobState;
    match state {
        JobState::Queued => "queued",
        JobState::Running => "running",
        JobState::Succeeded => "succeeded",
        JobState::Failed => "failed",
        JobState::Cancelled => "cancelled",
    }
}

/// `JobRecoveryResult` (backend-client `types/jobs.ts`) — the fields
/// the terminal details read projects.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobRecoveryResultVm {
    pub verdict: Option<String>,
    pub resume_through: Option<u64>,
    pub reason_code: Option<String>,
}

/// `JobDetails` (backend-client `types/jobs.ts`) — the durable
/// terminal report `get_transfer_job_details` answers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobDetailsVm {
    pub job: MigrationJobViewVm,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_revision: Option<u64>,
    #[serde(default)]
    pub commit_boundaries: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<JobRecoveryResultVm>,
}

/// `projectApplyJobView` — merge the generic Job read model into the
/// transfer admission receipt. A terminal summary is not the durable
/// report: the effect outcome stays unknown until the details query
/// returns boundaries and the exact recovery disposition.
pub(crate) fn project_apply_job_view(
    current: &TransferApplyJobView,
    latest: &MigrationJobViewVm,
) -> TransferApplyJobView {
    let active = is_active_job_state(job_state_as_str(&latest.state));
    let mut next = current.clone();
    next.state = job_state_as_str(&latest.state).to_owned();
    next.effect_outcome = if active {
        effect_outcome_as_str(latest.effect_outcome)
            .map(str::to_owned)
            .unwrap_or_else(|| current.effect_outcome.clone())
    } else {
        "unknown".to_owned()
    };
    next.progress = serde_json::to_value(&latest.progress).unwrap_or_default();
    next.artifact_ids = latest.artifact_ids.clone();
    next.cancelled = latest.state
        == crate::hooks::migration_job_hydration_helpers::JobState::Cancelled;
    next.partial = matches!(
        latest.effect_outcome,
        Some(EffectOutcome::PartiallyApplied) | Some(EffectOutcome::Unknown)
    );
    next
}

/// `projectApplyJobDetails` — merge the durable terminal read model
/// into the live apply receipt.
pub(crate) fn project_apply_job_details(
    current: &TransferApplyJobView,
    details: &JobDetailsVm,
) -> TransferApplyJobView {
    let job = &details.job;
    let mut next = current.clone();
    next.job_id = job.job_id.clone();
    next.kind = job.kind.clone();
    next.state = job_state_as_str(&job.state).to_owned();
    next.effect_outcome = effect_outcome_as_str(job.effect_outcome)
        .unwrap_or("unknown")
        .to_owned();
    next.progress = serde_json::to_value(&job.progress).unwrap_or_default();
    next.plan_id = details.plan_id.clone().unwrap_or_else(|| current.plan_id.clone());
    next.plan_digest = details
        .plan_digest
        .clone()
        .unwrap_or_else(|| current.plan_digest.clone());
    next.selection_revision = details
        .selection_revision
        .unwrap_or(current.selection_revision);
    next.commit_boundaries = details.commit_boundaries.clone();
    next.artifact_ids = job.artifact_ids.clone();
    next.cancelled = job.state
        == crate::hooks::migration_job_hydration_helpers::JobState::Cancelled;
    next.partial = matches!(
        job.effect_outcome,
        Some(EffectOutcome::PartiallyApplied) | Some(EffectOutcome::Unknown)
    );
    next.error = job.error.clone();
    next.recovery_verdict = details.recovery.as_ref().and_then(|r| r.verdict.clone());
    next.recovery_resume_through =
        details.recovery.as_ref().and_then(|r| r.resume_through);
    next.recovery_reason = details.recovery.as_ref().and_then(|r| r.reason_code.clone());
    next
}
