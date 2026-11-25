//! The schema-diff wire vocabulary (`backend-client/types/schemaDiff.ts`)
//! and the domain client's tagged-enum folding (`schemaDiffClient.ts`).
//!
//! The Rust backend serializes plan requirements as tagged enums
//! (`{ backfill: {…} }` / `{ unsupported: { operation, reason } }`); the
//! React domain client folds them into the discriminated union
//! `PlanRequirement` on the way in and unfolds on the way out. That
//! folding is product behavior (the `users.meta` → table `users` /
//! column `meta` split), so it is ported here, once, and every later
//! schema-diff task calls these functions instead of re-deriving.
//!
//! The `*Vm` structs are the normalized shapes the UI reads (the host
//! converts; `datazen-ui` does not depend on the client crate). The
//! `*Wire` structs are the serde wire mirrors, camelCase, with
//! unknown-key tolerance (React only reads the keys it knows).

use serde::{Deserialize, Serialize};

use crate::views::data_transfer::data_transfer_attached_jobs::{
    JobCounter, JobProgress, JobState,
};

// ---- normalized vocabulary ---------------------------------------------------

/// React `StatementRisk`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatementRiskVm {
    Additive,
    Destructive,
    Rewrite,
}

impl StatementRiskVm {
    /// The wire spelling.
    pub fn wire_str(self) -> &'static str {
        match self {
            Self::Additive => "additive",
            Self::Destructive => "destructive",
            Self::Rewrite => "rewrite",
        }
    }

    /// The wire spelling's parse twin; unknown spellings are `None`, not a
    /// silent default (React could not have produced them).
    pub fn from_wire_str(value: &str) -> Option<Self> {
        match value {
            "additive" => Some(Self::Additive),
            "destructive" => Some(Self::Destructive),
            "rewrite" => Some(Self::Rewrite),
            _ => None,
        }
    }
}

/// React `PlanStatement`.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanStatementVm {
    pub sql: String,
    pub risk: StatementRiskVm,
    /// `rollbackSql: string | null`.
    pub rollback_sql: Option<String>,
    pub summary: String,
    /// `requiresTransaction?: boolean` — `undefined` is falsy in React's
    /// `.any(s => s.requiresTransaction)`, so `None` reads as false.
    pub requires_transaction: Option<bool>,
}

/// React `PlanRequirement.kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanRequirementKindVm {
    Backfill,
    Unsupported,
}

/// React `PlanRequirement` (the client-folded union).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanRequirementVm {
    pub kind: PlanRequirementKindVm,
    pub table: String,
    pub column: String,
    pub reason: String,
}

/// React `RollbackCompleteness`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RollbackCompletenessVm {
    pub complete: bool,
    pub missing: Vec<String>,
}

/// React `RollbackCompletenessCounts` — `{ complete, missing }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RollbackCompletenessCountsVm {
    pub complete: usize,
    pub missing: usize,
}

/// `rollbackCompletenessCounts` (`schemaDiffClient.ts`): complete is the
/// statement total minus the missing rollbacks.
pub fn rollback_completeness_counts(plan: &SchemaDiffPlanVm) -> RollbackCompletenessCountsVm {
    let total = plan.statements.len();
    let missing = plan.rollback_completeness.missing.len();
    RollbackCompletenessCountsVm { complete: total - missing, missing }
}

/// React `TypeSuggestion`.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeSuggestionVm {
    pub table: String,
    pub column: String,
    pub source_type: String,
    pub suggested_type: String,
    pub current_type: String,
    pub reason: String,
    pub is_key_or_indexed: bool,
}

/// React `ColumnTypeOverride`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnTypeOverrideVm {
    pub table: String,
    pub column: String,
    pub target_type: String,
}

/// React `SchemaDiffPlan`, the fields the schema-diff UI reads.
/// `expectedTargetSchemas` (stale-review rejection snapshots) is not
/// modelled — no UI consumer reads it (documented in the module root).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SchemaDiffPlanVm {
    pub plan_id: Option<String>,
    pub table: String,
    pub tables: Vec<String>,
    pub source_dialect: String,
    pub target_dialect: String,
    pub same_dialect: bool,
    pub statements: Vec<PlanStatementVm>,
    pub warnings: Vec<String>,
    /// `requirements?: PlanRequirement[]` — absent reads as empty.
    pub requirements: Vec<PlanRequirementVm>,
    pub rollback_completeness: RollbackCompletenessVm,
    pub type_suggestions: Option<Vec<TypeSuggestionVm>>,
}

/// React `SchemaDiffPrepareEnvelope`.
#[derive(Clone, Debug, PartialEq)]
pub struct SchemaDiffPrepareEnvelopeVm {
    pub plan: SchemaDiffPlanVm,
    pub plan_id: String,
    pub source_connection_id: Option<String>,
    pub target_connection_id: Option<String>,
    pub selection_revision: i64,
    pub plan_version: i64,
    pub handler_version: i64,
    pub checkpoint_version: i64,
    pub expires_at: String,
    pub recovery_policy: String,
}

/// React `StatementExecResult`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatementExecResultVm {
    pub index: usize,
    pub sql: String,
    pub ok: bool,
    pub error: Option<String>,
}

/// React `DeployStatus` (rendered as its wire string).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeployStatusVm {
    Committed,
    Unknown,
    RolledBack,
    Mixed,
    Failed,
    Cancelled,
}

impl DeployStatusVm {
    /// The wire string React interpolates into the status line.
    pub fn wire_str(self) -> &'static str {
        match self {
            DeployStatusVm::Committed => "committed",
            DeployStatusVm::Unknown => "unknown",
            DeployStatusVm::RolledBack => "rolled_back",
            DeployStatusVm::Mixed => "mixed",
            DeployStatusVm::Failed => "failed",
            DeployStatusVm::Cancelled => "cancelled",
        }
    }

    /// The wire spelling's parse twin.
    pub fn from_wire_str(value: &str) -> Option<Self> {
        match value {
            "committed" => Some(Self::Committed),
            "unknown" => Some(Self::Unknown),
            "rolled_back" => Some(Self::RolledBack),
            "mixed" => Some(Self::Mixed),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

/// React `SchemaDiffDeployResult`.
#[derive(Clone, Debug, PartialEq)]
pub struct SchemaDiffDeployResultVm {
    pub status: DeployStatusVm,
    pub executed_count: usize,
    pub statement_count: usize,
    pub errors: Vec<String>,
    pub statement_results: Vec<StatementExecResultVm>,
}

/// React `SchemaDiffJobAccepted` — `jobId`, the `schemaDiffPrepare` /
/// `schemaDiffApply` kind, the starting state and progress.
#[derive(Clone, Debug, PartialEq)]
pub struct SchemaDiffJobAcceptedVm {
    pub job_id: String,
    pub kind: String,
    pub state: JobState,
    pub progress: JobProgress,
}

/// The fields of `JobDetails.details.job` (`JobView`) the schema-diff UI
/// reads: identity, state, stage, cancellation, error, the P5 progress
/// buckets, the update stamp (the lifecycle's latest-first ordering) and
/// the pending-verification flag (the hydration's selection). The unread
/// projection (execution/artifact ids, effect outcome) is not modelled.
#[derive(Clone, Debug, PartialEq)]
pub struct SchemaDiffJobViewVm {
    pub job_id: String,
    pub kind: String,
    pub state: JobState,
    pub stage: Option<String>,
    pub cancel_requested: bool,
    pub error: Option<String>,
    pub progress: JobProgress,
    /// `updatedAt` — epoch milliseconds; the lifecycle sorts by it,
    /// latest first.
    pub updated_at: i64,
    /// Why the job is pending verification (e.g. `outcomeUnknown`), else
    /// null.
    pub pending_verification_reason: Option<String>,
}

/// React `JobRecoveryVerdict` (`jobs.ts`): the recovery receipt's verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobRecoveryVerdictVm {
    NotExecuted,
    PendingVerification,
    ResumeAfterVerify,
    Reject,
    RequireManualReview,
}

impl JobRecoveryVerdictVm {
    /// The wire spelling.
    pub fn wire_str(self) -> &'static str {
        match self {
            Self::NotExecuted => "notExecuted",
            Self::PendingVerification => "pendingVerification",
            Self::ResumeAfterVerify => "resumeAfterVerify",
            Self::Reject => "reject",
            Self::RequireManualReview => "requireManualReview",
        }
    }

    /// The wire spelling's parse twin.
    pub fn from_wire_str(value: &str) -> Option<Self> {
        match value {
            "notExecuted" => Some(Self::NotExecuted),
            "pendingVerification" => Some(Self::PendingVerification),
            "resumeAfterVerify" => Some(Self::ResumeAfterVerify),
            "reject" => Some(Self::Reject),
            "requireManualReview" => Some(Self::RequireManualReview),
            _ => None,
        }
    }
}

/// React `JobRecoveryResult` — the safe recovery receipt; `reasonCode` is
/// a stable code, never handler text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobRecoveryResultVm {
    pub verdict: JobRecoveryVerdictVm,
    pub reason_code: Option<String>,
}

/// React `JobRecoveryTarget` — a stable target identity for an explicit
/// fresh-session recovery verifier (read for presence only).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobRecoveryTargetVm {
    pub connection_id: String,
    pub object_ids: Vec<String>,
}

/// React `SchemaDiffJobDetails` — the durable detail query's envelope.
#[derive(Clone, Debug, PartialEq)]
pub struct SchemaDiffJobDetailsVm {
    pub details: SchemaDiffJobViewVm,
    pub state_version: i64,
    pub prepared: Option<SchemaDiffPrepareEnvelopeVm>,
    pub plan_unavailable_after_restart: bool,
    pub deploy_result: Option<SchemaDiffDeployResultVm>,
    /// The recovery receipt, when one was derived.
    pub recovery: Option<JobRecoveryResultVm>,
    /// The explicit recovery verifier's targets (presence matters).
    pub recovery_targets: Vec<JobRecoveryTargetVm>,
}

impl SchemaDiffJobDetailsVm {
    /// React's poll-loop terminal test:
    /// `details.details.job.state !== 'queued' && … !== 'running'`.
    pub fn job_terminal(&self) -> bool {
        !matches!(self.details.state, JobState::Queued | JobState::Running)
    }
}

/// React `SchemaDiffProfile` — the persisted setup (version 1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDiffProfileVm {
    pub version: u32,
    pub id: String,
    pub name: String,
    pub source_connection_id: String,
    pub target_connection_id: String,
    pub source_database: String,
    pub target_database: String,
    // The optionals ride only when present (React's profile object passes
    // through verbatim — `JSON.stringify` drops the absent keys).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_schema: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_schema: Option<String>,
    pub tables: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_only_tables: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_objects: Option<Vec<crate::views::schema_diff::schema_diff_object_identity::SchemaDiffObjectIdentityVm>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_objects: Option<Vec<crate::views::schema_diff::schema_diff_object_identity::SchemaDiffObjectIdentityVm>>,
    pub allow_destructive: bool,
    pub include_indexes: bool,
    pub require_rollback: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_overrides: Option<Vec<ColumnTypeOverrideVm>>,
    pub created_at: String,
    pub updated_at: String,
}

// ---- the wire mirrors + folding ----------------------------------------------

/// `PlanRequirementWire`'s backfill body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct BackfillBody {
    table: String,
    column: String,
    reason: String,
}

/// `PlanRequirementWire`'s unsupported body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct UnsupportedBody {
    operation: String,
    reason: String,
}

/// React `PlanRequirementWire`: the tagged enum the Rust backend
/// serializes. `'backfill' in raw` picks the branch (serde's untagged
/// tries Backfill first, matching).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
enum PlanRequirementWire {
    Backfill { backfill: BackfillBody },
    Unsupported { unsupported: UnsupportedBody },
}

/// `normalizeRequirement`: backfill passes through; unsupported splits the
/// dotted operation at the FIRST dot (`dot > 0` — a leading dot stays part
/// of the table name), and an operation without a dot leaves the column
/// empty.
fn normalize_requirement(raw: PlanRequirementWire) -> PlanRequirementVm {
    match raw {
        PlanRequirementWire::Backfill { backfill } => PlanRequirementVm {
            kind: PlanRequirementKindVm::Backfill,
            table: backfill.table,
            column: backfill.column,
            reason: backfill.reason,
        },
        PlanRequirementWire::Unsupported { unsupported } => {
            let dot = unsupported.operation.find('.');
            match dot {
                Some(dot) if dot > 0 => PlanRequirementVm {
                    kind: PlanRequirementKindVm::Unsupported,
                    table: unsupported.operation[..dot].to_owned(),
                    column: unsupported.operation[dot + 1..].to_owned(),
                    reason: unsupported.reason,
                },
                _ => PlanRequirementVm {
                    kind: PlanRequirementKindVm::Unsupported,
                    table: unsupported.operation,
                    column: String::new(),
                    reason: unsupported.reason,
                },
            }
        }
    }
}

/// `denormalizeRequirement`: backfill passes through; unsupported re-joins
/// `table.column` when the column is non-empty, else the bare table.
fn denormalize_requirement(requirement: &PlanRequirementVm) -> PlanRequirementWire {
    match requirement.kind {
        PlanRequirementKindVm::Backfill => PlanRequirementWire::Backfill {
            backfill: BackfillBody {
                table: requirement.table.clone(),
                column: requirement.column.clone(),
                reason: requirement.reason.clone(),
            },
        },
        PlanRequirementKindVm::Unsupported => {
            let operation = if requirement.column.is_empty() {
                requirement.table.clone()
            } else {
                format!("{}.{}", requirement.table, requirement.column)
            };
            PlanRequirementWire::Unsupported {
                unsupported: UnsupportedBody { operation, reason: requirement.reason.clone() },
            }
        }
    }
}

/// React `PlanStatementWire` (the wire mirror of the statement).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanStatementWire {
    sql: String,
    risk: String,
    rollback_sql: Option<String>,
    summary: String,
    #[serde(default)]
    requires_transaction: Option<bool>,
}

impl TryFrom<&PlanStatementWire> for PlanStatementVm {
    type Error = String;

    fn try_from(wire: &PlanStatementWire) -> Result<Self, Self::Error> {
        Ok(Self {
            sql: wire.sql.clone(),
            risk: StatementRiskVm::from_wire_str(&wire.risk)
                .ok_or_else(|| format!("unknown statement risk: {}", wire.risk))?,
            rollback_sql: wire.rollback_sql.clone(),
            summary: wire.summary.clone(),
            requires_transaction: wire.requires_transaction,
        })
    }
}

impl From<&PlanStatementVm> for PlanStatementWire {
    fn from(vm: &PlanStatementVm) -> Self {
        Self {
            sql: vm.sql.clone(),
            risk: vm.risk.wire_str().to_owned(),
            rollback_sql: vm.rollback_sql.clone(),
            summary: vm.summary.clone(),
            requires_transaction: vm.requires_transaction,
        }
    }
}

/// React `TypeSuggestionWire`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TypeSuggestionWire {
    table: String,
    column: String,
    source_type: String,
    suggested_type: String,
    current_type: String,
    reason: String,
    is_key_or_indexed: bool,
}

/// React `RollbackCompleteness` on the wire.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RollbackCompletenessWire {
    complete: bool,
    missing: Vec<String>,
}

/// React `SchemaDiffPlanWire` — the plan with tagged requirements.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SchemaDiffPlanWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    plan_id: Option<String>,
    table: String,
    tables: Vec<String>,
    source_dialect: String,
    target_dialect: String,
    same_dialect: bool,
    statements: Vec<PlanStatementWire>,
    #[serde(default)]
    warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    requirements: Option<Vec<PlanRequirementWire>>,
    rollback_completeness: RollbackCompletenessWire,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    type_suggestions: Option<Vec<TypeSuggestionWire>>,
}

/// `normalizePlan`: fold tagged requirements (`?? []` for the absent case).
fn normalize_plan(wire: &SchemaDiffPlanWire) -> Result<SchemaDiffPlanVm, String> {
    Ok(SchemaDiffPlanVm {
        plan_id: wire.plan_id.clone(),
        table: wire.table.clone(),
        tables: wire.tables.clone(),
        source_dialect: wire.source_dialect.clone(),
        target_dialect: wire.target_dialect.clone(),
        same_dialect: wire.same_dialect,
        statements: wire
            .statements
            .iter()
            .map(PlanStatementVm::try_from)
            .collect::<Result<Vec<_>, _>>()?,
        warnings: wire.warnings.clone(),
        requirements: wire
            .requirements
            .as_ref()
            .map(|requirements| requirements.iter().cloned().map(normalize_requirement).collect())
            .unwrap_or_default(),
        rollback_completeness: RollbackCompletenessVm {
            complete: wire.rollback_completeness.complete,
            missing: wire.rollback_completeness.missing.clone(),
        },
        type_suggestions: wire.type_suggestions.as_ref().map(|suggestions| {
            suggestions
                .iter()
                .map(|s| TypeSuggestionVm {
                    table: s.table.clone(),
                    column: s.column.clone(),
                    source_type: s.source_type.clone(),
                    suggested_type: s.suggested_type.clone(),
                    current_type: s.current_type.clone(),
                    reason: s.reason.clone(),
                    is_key_or_indexed: s.is_key_or_indexed,
                })
                .collect()
        }),
    })
}

/// `denormalizePlan` as the wire mirror — the deploy request's plan body.
/// Requirements always ride the wire (`?? []`), even when empty.
fn plan_to_wire(vm: &SchemaDiffPlanVm) -> SchemaDiffPlanWire {
    SchemaDiffPlanWire {
        plan_id: vm.plan_id.clone(),
        table: vm.table.clone(),
        tables: vm.tables.clone(),
        source_dialect: vm.source_dialect.clone(),
        target_dialect: vm.target_dialect.clone(),
        same_dialect: vm.same_dialect,
        statements: vm.statements.iter().map(PlanStatementWire::from).collect(),
        warnings: vm.warnings.clone(),
        requirements: Some(
            vm.requirements.iter().map(denormalize_requirement).collect(),
        ),
        rollback_completeness: RollbackCompletenessWire {
            complete: vm.rollback_completeness.complete,
            missing: vm.rollback_completeness.missing.clone(),
        },
        type_suggestions: vm.type_suggestions.as_ref().map(|suggestions| {
            suggestions
                .iter()
                .map(|s| TypeSuggestionWire {
                    table: s.table.clone(),
                    column: s.column.clone(),
                    source_type: s.source_type.clone(),
                    suggested_type: s.suggested_type.clone(),
                    current_type: s.current_type.clone(),
                    reason: s.reason.clone(),
                    is_key_or_indexed: s.is_key_or_indexed,
                })
                .collect()
        }),
    }
}

/// `denormalizePlan` → the JSON value the deploy request embeds.
pub fn denormalize_plan_to_json(plan: &SchemaDiffPlanVm) -> serde_json::Value {
    match serde_json::to_value(plan_to_wire(plan)) {
        Ok(value) => value,
        Err(_) => serde_json::Value::Null,
    }
}

/// React `SchemaDiffPrepareEnvelopeWire` (tagged plan inside).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SchemaDiffPrepareEnvelopeWire {
    plan: SchemaDiffPlanWire,
    plan_id: String,
    #[serde(default)]
    source_connection_id: Option<String>,
    #[serde(default)]
    target_connection_id: Option<String>,
    selection_revision: i64,
    plan_version: i64,
    handler_version: i64,
    checkpoint_version: i64,
    expires_at: String,
    recovery_policy: String,
}

/// React `StatementExecResultWire`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatementExecResultWire {
    index: i64,
    sql: String,
    ok: bool,
    error: Option<String>,
}

/// React `SchemaDiffDeployResultWire`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SchemaDiffDeployResultWire {
    status: String,
    executed_count: i64,
    statement_count: i64,
    #[serde(default)]
    errors: Vec<String>,
    #[serde(default)]
    statement_results: Vec<StatementExecResultWire>,
}

/// A progress bucket on the wire: a JSON number or a decimal string.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
enum WireCounter {
    Number(f64),
    Text(String),
}

impl From<WireCounter> for JobCounter {
    fn from(wire: WireCounter) -> Self {
        match wire {
            WireCounter::Number(value) => JobCounter::Number(value),
            WireCounter::Text(value) => JobCounter::Text(value),
        }
    }
}

/// React `JobProgress` on the wire (absent buckets read as unreadable,
/// exactly as `readJobCounter(undefined)` reports `None`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobProgressWire {
    #[serde(default)]
    read: Option<WireCounter>,
    #[serde(default)]
    converted: Option<WireCounter>,
    #[serde(default)]
    attempted: Option<WireCounter>,
    #[serde(default)]
    committed: Option<WireCounter>,
    #[serde(default)]
    unknown: Option<WireCounter>,
}

impl From<JobProgressWire> for JobProgress {
    fn from(wire: JobProgressWire) -> Self {
        Self {
            read: wire.read.map(JobCounter::from),
            converted: wire.converted.map(JobCounter::from),
            attempted: wire.attempted.map(JobCounter::from),
            committed: wire.committed.map(JobCounter::from),
            unknown: wire.unknown.map(JobCounter::from),
        }
    }
}

/// React `SchemaDiffJobAcceptedWire`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SchemaDiffJobAcceptedWire {
    job_id: String,
    kind: String,
    state: String,
    #[serde(default)]
    progress: Option<JobProgressWire>,
}

impl TryFrom<SchemaDiffJobAcceptedWire> for SchemaDiffJobAcceptedVm {
    type Error = String;

    fn try_from(wire: SchemaDiffJobAcceptedWire) -> Result<Self, Self::Error> {
        Ok(Self {
            job_id: wire.job_id,
            kind: wire.kind,
            state: JobState::from_str(&wire.state)
                .ok_or_else(|| format!("unknown job state: {}", wire.state))?,
            progress: wire.progress.map(JobProgress::from).unwrap_or_default(),
        })
    }
}

/// The `JobView` fields the schema-diff UI reads, on the wire.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SchemaDiffJobViewWire {
    job_id: String,
    kind: String,
    state: String,
    #[serde(default)]
    stage: Option<String>,
    #[serde(default)]
    cancel_requested: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    progress: Option<JobProgressWire>,
    /// Epoch milliseconds.
    #[serde(default)]
    updated_at: i64,
    #[serde(default)]
    pending_verification_reason: Option<String>,
}

/// React `JobRecoveryResultWire`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobRecoveryResultWire {
    verdict: String,
    #[serde(default)]
    reason_code: Option<String>,
}

/// React `JobRecoveryTargetWire`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobRecoveryTargetWire {
    connection_id: String,
    #[serde(default)]
    object_ids: Vec<String>,
}

/// React `JobDetailsWire` (the `details` field).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SchemaDiffJobDetailsInnerWire {
    job: SchemaDiffJobViewWire,
    state_version: i64,
    #[serde(default)]
    recovery: Option<JobRecoveryResultWire>,
    #[serde(default)]
    recovery_targets: Vec<JobRecoveryTargetWire>,
}

/// React `SchemaDiffJobDetailsWire` — the outer envelope with the tagged
/// plan inside `prepared`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SchemaDiffJobDetailsEnvelopeWire {
    details: SchemaDiffJobDetailsInnerWire,
    #[serde(default)]
    prepared: Option<SchemaDiffPrepareEnvelopeWire>,
    #[serde(default)]
    plan_unavailable_after_restart: bool,
    #[serde(default)]
    deploy_result: Option<SchemaDiffDeployResultWire>,
}

/// `normalizeSchemaDiffJobDetailsWire` — the domain client's
/// `getJobDetails` / `verifyRecovery` answer path: deserialize the wire
/// envelope, fold the tagged requirements, parse the enums. A shape the
/// React client could not have produced is an error, not a default.
pub fn normalize_job_details(
    value: serde_json::Value,
) -> Result<SchemaDiffJobDetailsVm, String> {
    let wire: SchemaDiffJobDetailsEnvelopeWire = serde_json::from_value(value)
        .map_err(|error| format!("schema-diff job details wire: {error}"))?;
    let job = wire.details.job;
    let deploy_result = wire
        .deploy_result
        .map(|result| {
            Ok::<SchemaDiffDeployResultVm, String>(SchemaDiffDeployResultVm {
                status: DeployStatusVm::from_wire_str(&result.status)
                    .ok_or_else(|| format!("unknown deploy status: {}", result.status))?,
                executed_count: result.executed_count.max(0) as usize,
                statement_count: result.statement_count.max(0) as usize,
                errors: result.errors,
                statement_results: result
                    .statement_results
                    .into_iter()
                    .map(|statement| StatementExecResultVm {
                        index: statement.index.max(0) as usize,
                        sql: statement.sql,
                        ok: statement.ok,
                        error: statement.error,
                    })
                    .collect(),
            })
        })
        .transpose()?;
    let prepared = wire
        .prepared
        .map(|envelope| {
            Ok::<SchemaDiffPrepareEnvelopeVm, String>(SchemaDiffPrepareEnvelopeVm {
                plan: normalize_plan(&envelope.plan)?,
                plan_id: envelope.plan_id,
                source_connection_id: envelope.source_connection_id,
                target_connection_id: envelope.target_connection_id,
                selection_revision: envelope.selection_revision,
                plan_version: envelope.plan_version,
                handler_version: envelope.handler_version,
                checkpoint_version: envelope.checkpoint_version,
                expires_at: envelope.expires_at,
                recovery_policy: envelope.recovery_policy,
            })
        })
        .transpose()?;
    let recovery = wire.details.recovery.map(|recovery| {
        Ok::<JobRecoveryResultVm, String>(JobRecoveryResultVm {
            verdict: JobRecoveryVerdictVm::from_wire_str(&recovery.verdict)
                .ok_or_else(|| format!("unknown recovery verdict: {}", recovery.verdict))?,
            reason_code: recovery.reason_code,
        })
    }).transpose()?;
    let recovery_targets = wire
        .details
        .recovery_targets
        .into_iter()
        .map(|target| JobRecoveryTargetVm {
            connection_id: target.connection_id,
            object_ids: target.object_ids,
        })
        .collect();
    Ok(SchemaDiffJobDetailsVm {
        details: SchemaDiffJobViewVm {
            job_id: job.job_id,
            kind: job.kind,
            state: JobState::from_str(&job.state)
                .ok_or_else(|| format!("unknown job state: {}", job.state))?,
            stage: job.stage,
            cancel_requested: job.cancel_requested,
            error: job.error,
            progress: job.progress.map(JobProgress::from).unwrap_or_default(),
            updated_at: job.updated_at,
            pending_verification_reason: job.pending_verification_reason,
        },
        state_version: wire.details.state_version,
        prepared,
        plan_unavailable_after_restart: wire.plan_unavailable_after_restart,
        deploy_result,
        recovery,
        recovery_targets,
    })
}

/// Parse a `listSchemaDiffJobs` answer (a `JobView[]`) into the view
/// projections the lifecycle hydrates from.
pub fn parse_job_views(value: serde_json::Value) -> Result<Vec<SchemaDiffJobViewVm>, String> {
    let wires: Vec<SchemaDiffJobViewWire> = serde_json::from_value(value)
        .map_err(|error| format!("schema-diff jobs list wire: {error}"))?;
    wires
        .into_iter()
        .map(|job| {
            Ok(SchemaDiffJobViewVm {
                job_id: job.job_id,
                kind: job.kind,
                state: JobState::from_str(&job.state)
                    .ok_or_else(|| format!("unknown job state: {}", job.state))?,
                stage: job.stage,
                cancel_requested: job.cancel_requested,
                error: job.error,
                progress: job.progress.map(JobProgress::from).unwrap_or_default(),
                updated_at: job.updated_at,
                pending_verification_reason: job.pending_verification_reason,
            })
        })
        .collect()
}

/// Deserialize one accepted receipt (`prepareSchemaDiffPlan` /
/// `prepareSchemaUnifiedPlan` / `executeSchemaDiffDeploy` answers).
pub fn parse_job_accepted(
    value: serde_json::Value,
) -> Result<SchemaDiffJobAcceptedVm, String> {
    let wire: SchemaDiffJobAcceptedWire = serde_json::from_value(value)
        .map_err(|error| format!("schema-diff job accepted wire: {error}"))?;
    SchemaDiffJobAcceptedVm::try_from(wire)
}
