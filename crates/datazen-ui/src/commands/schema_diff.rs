//! Port of `packages/frontend/src/commands/schemaDiff.ts` (T377) plus the
//! schema-diff slice of `@datazen/backend-client` it re-exports
//! (`schemaDiffClient.ts`'s tagged-enum folding and
//! `rollbackCompletenessCounts`, `types/schemaDiff.ts`'s wire vocabulary).
//!
//! React's file is the UI-side orchestration shell over the schema-diff
//! domain client: the pure plan helpers, the clipboard config format, the
//! module-level job-observer set, and the submit→poll→resolve flow that
//! wraps `preparePlan` / `prepareUnifiedPlan` / `executeDeploy` around a
//! 400ms `getJobDetails` poll loop.
//!
//! ## What maps where
//!
//! - **Wire vocabulary** (`SchemaDiffPlan`, `PlanStatement`,
//!   `SchemaDiffDeployResult`, `SchemaDiffJobDetails`, …) lives in
//!   `@datazen/backend-client`; `datazen-ui` does not depend on the client
//!   crate, so the host converts into the `*Vm` projections in
//!   [`wire`]. [`runner`] carries the domain client's own logic (the
//!   `MethodMap` payloads and the `normalizeSchemaDiffJobDetailsWire`
//!   tagged-enum folding) so the folding rule is ported once, here.
//! - `planHasDestructive` / `dialectSupportsTransactionalDdl` /
//!   `exportPlanSql` / `rollbackCompletenessCounts` /
//!   `DESTRUCTIVE_CONFIRM_TOKEN` are 1:1 copies of the pure helpers (they
//!   first landed as the deploy panel's seam and now live here, their
//!   single source — `schema_diff_deploy_panel.rs` re-exports them).
//! - `SchemaDiffConfigJson` (the clipboard export format, v2) is the serde
//!   shape; the v1 rejection rule is its consumer's
//!   (`useSchemaDiffSavedSetups.ts`, T409's task) — only `version: 2` and
//!   the field set live here, exactly like the React interface.
//! - `subscribeSchemaDiffJobUpdates`' module-level `Set` becomes
//!   [`SchemaDiffJobBus`]: the host owns one instance, subscribe returns an
//!   id (React's unsubscribe closure), publish iterates in insertion order
//!   (JS `Set` iteration order).
//! - `userAssetToken()` (`issueSubmissionToken('saveUserAsset', null)`) and
//!   every `useBackendClient()` call become effects, not promises (the
//!   `session_cancel.rs` / `ai_store.rs` pattern): [`runner`]'s
//!   [`SchemaDiffJobRun`](runner::SchemaDiffJobRun) emits
//!   [`SchemaDiffEffect`](runner::SchemaDiffEffect) steps and the host runs
//!   the transport, delivering receipts through `on_token` / `on_accepted`
//!   / `on_details`. The 400ms poll wait becomes a deadline the host turns
//!   into `Tx::wake_at`.
//! - The single-shot wrappers (`getProfiles`, `saveProfile`,
//!   `deleteProfile`, `compareTableSchemas`, `getJobDetails`, `listJobs`,
//!   `cancelDeploy`, `verifyRecovery`) are payload shapers in [`runner`]:
//!   each returns the exact `MethodMap` JSON (key-for-key with React's
//!   `__tests__/schemaDiff.test.ts` wire assertions) plus
//!   [`with_submission_token`](runner::with_submission_token) for the
//!   token-carrying calls.
//!
//! ## Known deltas (documented, not silent)
//!
//! - JS `undefined`-valued keys are dropped by `JSON.stringify`; the
//!   shapers model the resulting wire (key absent), not the in-memory call
//!   args the React test sees — the Rust assertions compare the wire.
//! - `expectedTargetSchemas` (stale-SQLite-review rejection snapshots) is
//!   not modelled in `SchemaDiffPlanVm`: no landed or remaining
//!   schema-diff task reads it (the backend's check consumes it before the
//!   UI sees the plan). It rides the wire unknown-key-tolerant.
//! - `JobDetails`' unread projection fields (`commitBoundaries`,
//!   `recovery`, `domainResults`, `recoveryTargets`, fingerprints) are not
//!   modelled: no schema-diff consumer reads them.

pub(crate) mod runner;
pub(crate) mod wire;

pub(crate) use runner::{
    DeployProfileRefVm, ExecuteDeployParamsVm, PreparePlanParamsVm, PrepareUnifiedParamsVm,
    SchemaDiffEffect, SchemaDiffJobRun, SchemaDiffJobRunOutcome, SchemaDiffMethod,
    SchemaDiffRunKind, SchemaDiffSubmissionTokenVm, cancel_deploy_request,
    compare_table_schemas_request, delete_profile_request, execute_deploy_request,
    get_job_details_request, get_profiles_request, list_jobs_request, prepare_plan_request,
    prepare_unified_plan_request, save_profile_request, verify_recovery_request,
    with_submission_token,
};
pub(crate) use wire::{
    ColumnTypeOverrideVm, DeployStatusVm, PlanRequirementKindVm, PlanRequirementVm,
    PlanStatementVm, RollbackCompletenessCountsVm, RollbackCompletenessVm, SchemaDiffDeployResultVm,
    SchemaDiffJobAcceptedVm, SchemaDiffJobDetailsVm, SchemaDiffPlanVm, SchemaDiffProfileVm,
    SchemaDiffPrepareEnvelopeVm, StatementExecResultVm, StatementRiskVm, TypeSuggestionVm,
    denormalize_plan_to_json, normalize_job_details, rollback_completeness_counts,
};

use std::rc::Rc;

use serde::{Deserialize, Serialize};

use crate::views::schema_diff::schema_diff_object_identity::SchemaDiffObjectIdentityVm;

/// React `DESTRUCTIVE_CONFIRM_TOKEN` ('DEPLOY').
pub(crate) const DESTRUCTIVE_CONFIRM_TOKEN: &str = "DEPLOY";

/// React `planHasDestructive`: any statement whose risk is `destructive` or
/// `rewrite`.
pub(crate) fn plan_has_destructive(plan: &SchemaDiffPlanVm) -> bool {
    plan.statements.iter().any(|s| {
        matches!(s.risk, StatementRiskVm::Destructive | StatementRiskVm::Rewrite)
    })
}

/// React `dialectSupportsTransactionalDdl`: postgresql / postgres / sqlite
/// (case-insensitive).
pub(crate) fn dialect_supports_transactional_ddl(dialect: &str) -> bool {
    let d = dialect.to_lowercase();
    d == "postgresql" || d == "postgres" || d == "sqlite"
}

/// React `exportPlanSql`: the header comment block, then every statement
/// semicolon-terminated, joined by newlines —
/// `[...header, ...statements].join('\n')` with the header's trailing
/// empty string producing one blank line between header and statements.
pub(crate) fn export_plan_sql(plan: &SchemaDiffPlanVm) -> String {
    let mut lines: Vec<String> = vec![
        "-- Schema Diff Deploy plan".to_owned(),
        format!("-- tables: {}", plan.tables.join(", ")),
        format!("-- {} → {}", plan.source_dialect, plan.target_dialect),
        String::new(),
    ];
    lines.extend(plan.statements.iter().map(|s| format!("{};", s.sql)));
    lines.join("\n")
}

// ---- the job-observer bus (subscribeSchemaDiffJobUpdates) --------------------

/// One broadcast the module publishes: the accepted receipt when a prepare /
/// apply job is taken, then every `getJobDetails` poll answer.
/// React `SchemaDiffJobUpdate`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SchemaDiffJobUpdateVm {
    Accepted(SchemaDiffJobAcceptedVm),
    Details(SchemaDiffJobDetailsVm),
}

/// The unsubscribe handle React's `subscribeSchemaDiffJobUpdates` closure
/// owns. Dropping it does NOT unsubscribe (React's closure must be called
/// explicitly); the id is the key back into the bus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SchemaDiffJobSubscription {
    pub(crate) id: u64,
}

/// React's module-level `schemaDiffJobObservers` `Set`. The host owns one
/// instance for the window's lifetime; insertion order is preserved (JS
/// `Set` iteration order) so publishes reach observers in subscribe order.
#[derive(Default)]
pub(crate) struct SchemaDiffJobBus {
    observers: Vec<(u64, Rc<dyn Fn(&SchemaDiffJobUpdateVm)>)>,
    next_id: u64,
}

impl SchemaDiffJobBus {
    /// `subscribeSchemaDiffJobUpdates(observer)`: register, get the
    /// unsubscribe handle.
    pub(crate) fn subscribe(
        &mut self,
        observer: Rc<dyn Fn(&SchemaDiffJobUpdateVm)>,
    ) -> SchemaDiffJobSubscription {
        let id = self.next_id;
        self.next_id += 1;
        self.observers.push((id, observer));
        SchemaDiffJobSubscription { id }
    }

    /// The unsubscribe closure's `observers.delete(observer)`: `true` when
    /// the observer was registered.
    pub(crate) fn unsubscribe(&mut self, subscription: SchemaDiffJobSubscription) -> bool {
        let before = self.observers.len();
        self.observers.retain(|(id, _)| *id != subscription.id);
        self.observers.len() != before
    }

    /// `publishSchemaDiffJobUpdate`: every observer, in subscribe order.
    pub(crate) fn publish(&self, update: &SchemaDiffJobUpdateVm) {
        for (_, observer) in &self.observers {
            observer(update);
        }
    }

    /// How many observers are registered (the `Set`'s `size`).
    pub(crate) fn len(&self) -> usize {
        self.observers.len()
    }

    /// Whether no observer is registered.
    pub(crate) fn is_empty(&self) -> bool {
        self.observers.is_empty()
    }
}

// ---- the clipboard export format (SchemaDiffConfigJson) ----------------------

/// The clipboard export format's version. v2 renamed the connection keys to
/// `connectionId` (the `dbSessionId` terminology); v1 configs (with
/// `configId` keys) are rejected by the consumer — `useSchemaDiffSavedSetups.ts`
/// (T409) — not by this shape.
pub(crate) const SCHEMA_DIFF_CONFIG_JSON_VERSION: u32 = 2;

/// React `SchemaDiffConfigJson`: the clipboard export/import format of a
/// saved schema-diff setup. Runtime session ids and generated plans are
/// never part of it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SchemaDiffConfigJson {
    pub(crate) version: u32,
    pub(crate) source_connection_id: String,
    pub(crate) target_connection_id: String,
    pub(crate) tables: Vec<String>,
    /// Optional to keep v2 configs exported before target-only selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) target_only_tables: Option<Vec<String>>,
    /// Optional to keep v2 configs exported before unified object selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) source_objects: Option<Vec<SchemaDiffObjectIdentityVm>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) target_objects: Option<Vec<SchemaDiffObjectIdentityVm>>,
    pub(crate) allow_destructive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) include_indexes: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) require_rollback: Option<bool>,
}
