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
pub(crate) enum StatementRiskVm {
    Additive,
    Destructive,
    Rewrite,
}

impl StatementRiskVm {
    /// The wire spelling.
    pub(crate) fn wire_str(self) -> &'static str {
        match self {
            Self::Additive => "additive",
            Self::Destructive => "destructive",
            Self::Rewrite => "rewrite",
        }
    }

    /// The wire spelling's parse twin; unknown spellings are `None`, not a
    /// silent default (React could not have produced them).
    pub(crate) fn from_wire_str(value: &str) -> Option<Self> {
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
pub(crate) struct PlanStatementVm {
    pub(crate) sql: String,
    pub(crate) risk: StatementRiskVm,
    /// `rollbackSql: string | null`.
    pub(crate) rollback_sql: Option<String>,
    pub(crate) summary: String,
    /// `requiresTransaction?: boolean` — `undefined` is falsy in React's
    /// `.any(s => s.requiresTransaction)`, so `None` reads as false.
    pub(crate) requires_transaction: Option<bool>,
}

/// React `PlanRequirement.kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlanRequirementKindVm {
    Backfill,
    Unsupported,
}

/// React `PlanRequirement` (the client-folded union).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlanRequirementVm {
    pub(crate) kind: PlanRequirementKindVm,
    pub(crate) table: String,
    pub(crate) column: String,
    pub(crate) reason: String,
}

/// React `RollbackCompleteness`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RollbackCompletenessVm {
    pub(crate) complete: bool,
    pub(crate) missing: Vec<String>,
}

/// React `RollbackCompletenessCounts` — `{ complete, missing }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RollbackCompletenessCountsVm {
    pub(crate) complete: usize,
    pub(crate) missing: usize,
}

/// `rollbackCompletenessCounts` (`schemaDiffClient.ts`): complete is the
/// statement total minus the missing rollbacks.
pub(crate) fn rollback_completeness_counts(plan: &SchemaDiffPlanVm) -> RollbackCompletenessCountsVm {
    let total = plan.statements.len();
    let missing = plan.rollback_completeness.missing.len();
    RollbackCompletenessCountsVm { complete: total - missing, missing }
}

/// React `TypeSuggestion`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TypeSuggestionVm {
    pub(crate) table: String,
    pub(crate) column: String,
    pub(crate) source_type: String,
    pub(crate) suggested_type: String,
    pub(crate) current_type: String,
    pub(crate) reason: String,
    pub(crate) is_key_or_indexed: bool,
}

/// React `ColumnTypeOverride`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ColumnTypeOverrideVm {
    pub(crate) table: String,
    pub(crate) column: String,
    pub(crate) target_type: String,
}

/// React `SchemaDiffPlan`, the fields the schema-diff UI reads.
/// `expectedTargetSchemas` (stale-review rejection snapshots) is not
/// modelled — no UI consumer reads it (documented in the module root).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SchemaDiffPlanVm {
    pub(crate) plan_id: Option<String>,
    pub(crate) table: String,
    pub(crate) tables: Vec<String>,
    pub(crate) source_dialect: String,
    pub(crate) target_dialect: String,
    pub(crate) same_dialect: bool,
    pub(crate) statements: Vec<PlanStatementVm>,
    pub(crate) warnings: Vec<String>,
    /// `requirements?: PlanRequirement[]` — absent reads as empty.
    pub(crate) requirements: Vec<PlanRequirementVm>,
    pub(crate) rollback_completeness: RollbackCompletenessVm,
    pub(crate) type_suggestions: Option<Vec<TypeSuggestionVm>>,
}

/// React `SchemaDiffPrepareEnvelope`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SchemaDiffPrepareEnvelopeVm {
    pub(crate) plan: SchemaDiffPlanVm,
    pub(crate) plan_id: String,
    pub(crate) source_connection_id: Option<String>,
    pub(crate) target_connection_id: Option<String>,
    pub(crate) selection_revision: i64,
    pub(crate) plan_version: i64,
    pub(crate) handler_version: i64,
    pub(crate) checkpoint_version: i64,
    pub(crate) expires_at: String,
    pub(crate) recovery_policy: String,
}

/// React `StatementExecResult`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StatementExecResultVm {
    pub(crate) index: usize,
    pub(crate) sql: String,
    pub(crate) ok: bool,
    pub(crate) error: Option<String>,
}

/// React `DeployStatus` (rendered as its wire string).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeployStatusVm {
    Committed,
    Unknown,
    RolledBack,
    Mixed,
    Failed,
    Cancelled,
}

impl DeployStatusVm {
    /// The wire string React interpolates into the status line.
    pub(crate) fn wire_str(self) -> &'static str {
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
    pub(crate) fn from_wire_str(value: &str) -> Option<Self> {
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
pub(crate) struct SchemaDiffDeployResultVm {
    pub(crate) status: DeployStatusVm,
    pub(crate) executed_count: usize,
    pub(crate) statement_count: usize,
    pub(crate) errors: Vec<String>,
    pub(crate) statement_results: Vec<StatementExecResultVm>,
}

/// React `SchemaDiffJobAccepted` — `jobId`, the `schemaDiffPrepare` /
/// `schemaDiffApply` kind, the starting state and progress.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SchemaDiffJobAcceptedVm {
    pub(crate) job_id: String,
    pub(crate) kind: String,
    pub(crate) state: JobState,
    pub(crate) progress: JobProgress,
}

/// The fields of `JobDetails.details.job` (`JobView`) the schema-diff UI
/// reads: identity, state, stage, cancellation, error and the P5 progress
/// buckets. The unread projection (execution/artifact ids, timestamps,
/// effect outcome) is not modelled.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SchemaDiffJobViewVm {
    pub(crate) job_id: String,
    pub(crate) kind: String,
    pub(crate) state: JobState,
    pub(crate) stage: Option<String>,
    pub(crate) cancel_requested: bool,
    pub(crate) error: Option<String>,
    pub(crate) progress: JobProgress,
}

/// React `SchemaDiffJobDetails` — the durable detail query's envelope.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SchemaDiffJobDetailsVm {
    pub(crate) details: SchemaDiffJobViewVm,
    pub(crate) state_version: i64,
    pub(crate) prepared: Option<SchemaDiffPrepareEnvelopeVm>,
    pub(crate) plan_unavailable_after_restart: bool,
    pub(crate) deploy_result: Option<SchemaDiffDeployResultVm>,
}

impl SchemaDiffJobDetailsVm {
    /// React's poll-loop terminal test:
    /// `details.details.job.state !== 'queued' && … !== 'running'`.
    pub(crate) fn job_terminal(&self) -> bool {
        !matches!(self.details.state, JobState::Queued | JobState::Running)
    }
}

/// React `SchemaDiffProfile` — the persisted setup (version 1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SchemaDiffProfileVm {
    pub(crate) version: u32,
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) source_connection_id: String,
    pub(crate) target_connection_id: String,
    pub(crate) source_database: String,
    pub(crate) target_database: String,
    // The optionals ride only when present (React's profile object passes
    // through verbatim — `JSON.stringify` drops the absent keys).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) source_schema: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) target_schema: Option<String>,
    pub(crate) tables: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) target_only_tables: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) source_objects: Option<Vec<crate::views::schema_diff::schema_diff_object_identity::SchemaDiffObjectIdentityVm>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) target_objects: Option<Vec<crate::views::schema_diff::schema_diff_object_identity::SchemaDiffObjectIdentityVm>>,
    pub(crate) allow_destructive: bool,
    pub(crate) include_indexes: bool,
    pub(crate) require_rollback: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) type_overrides: Option<Vec<ColumnTypeOverrideVm>>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
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
pub(crate) fn denormalize_plan_to_json(plan: &SchemaDiffPlanVm) -> serde_json::Value {
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
}

/// React `JobDetailsWire` (the `details` field).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SchemaDiffJobDetailsInnerWire {
    job: SchemaDiffJobViewWire,
    state_version: i64,
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
pub(crate) fn normalize_job_details(
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
        },
        state_version: wire.details.state_version,
        prepared,
        plan_unavailable_after_restart: wire.plan_unavailable_after_restart,
        deploy_result,
    })
}

/// Deserialize one accepted receipt (`prepareSchemaDiffPlan` /
/// `prepareSchemaUnifiedPlan` / `executeSchemaDiffDeploy` answers).
pub(crate) fn parse_job_accepted(
    value: serde_json::Value,
) -> Result<SchemaDiffJobAcceptedVm, String> {
    let wire: SchemaDiffJobAcceptedWire = serde_json::from_value(value)
        .map_err(|error| format!("schema-diff job accepted wire: {error}"))?;
    SchemaDiffJobAcceptedVm::try_from(wire)
}
