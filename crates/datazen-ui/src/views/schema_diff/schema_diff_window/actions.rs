//! Port of `packages/frontend/src/windows/schema-diff/
//! SchemaDiffWindow.tsx` (T309) — the action half.
//!
//! The component's four async callbacks and its navigation cascade,
//! reduced to pure assemblies the host executes: every callback's body
//! is (1) a validation gate, (2) a request/payload assembly over the
//! window state, (3) a state-adoption rule for each outcome. The host
//! runs the IPC/clipboard halves and feeds the outcomes back.
//!
//! ## What maps where
//!
//! - `loadSourceTables` is [`begin_load_source_tables`] (the three
//!   clears + the loading flag) plus [`finish_load_source_tables`] (the
//!   picks adoption or the error + diagnostic + picks-clear, then the
//!   loading flag's end). The discovery walk itself is T351's
//!   [`discover_schema_diff_objects`] over a host trait.
//! - `runCompare` is [`begin_run_compare`] (the four clears + the
//!   `validateEndpoints` + the selected-empty gate) +
//!   [`compare_table_jobs`] (one unit per enabled pick: the target-only
//!   stub diff or the `compareTableSchemas` request — `sourceName ??
//!   name` and `targetName ?? name.split('.').at(-1) ?? name` verbatim)
//!   + [`finish_run_compare`].
//! - `buildPlan` is [`begin_build_plan`] (the error/deploy-result
//!   clears + the selection gate + the validation) +
//!   [`build_plan_params`] (the `tablePlanParams` assembly verbatim,
//!   plain vs unified by the object selections) +
//!   [`finish_build_plan`] (the plan, the envelope, the
//!   `useTransaction` adoption from the target dialect, the confirm
//!   clear). `handleApplyTypeOverrides` is `build_plan_params` with the
//!   explicit override list.
//! - `handleDeploy` is [`build_deploy_params`] (the payload assembly:
//!   `useTransaction: transactionEnabled`, the trimmed
//!   destructive-confirm only when destructive, `|| null` for the
//!   endpoint database/schema, the selected profile's `{id, revision:
//!   updatedAt}`, the envelope's `planId`/`selectionRevision`) +
//!   [`finish_deploy`] (the result adoption or the error; the
//!   stale-plan branch is [`deploy_outcome`]'s [`DeployOutcome::Stale`]
//!   — `migrationJob.reprepareOnStalePlan` + the plan/result clears).
//! - `goNext` is [`go_next_decision`]: the full gate cascade — the
//!   cross-dialect block, the selection-required gate, the
//!   objects-only skip straight to plan, the compare run, the plan
//!   step's plain advance, and the first-objects-visit discovery load.
//! - `goBack` is [`go_back_step`].
//! - `handleCopySummary` / `handleCopySql` are
//!   [`copy_summary_text`] / [`copy_sql_text`] (the clipboard payloads)
//!   with [`ClipboardOutcome`] adoption: the 2000ms feedback or the
//!   `schemaDiff.clipboardFailed` error.
//!
//! Known deltas: `ensureConnected` (the ref-counted session IPC) is the
//! host's; its failure surfaces as the callback's error branch, so the
//! gates treat it as an outcome, not a parameter.

use crate::commands::schema_diff::wire::{
    ColumnTypeOverrideVm, SchemaDiffDeployResultVm, SchemaDiffPlanVm,
    SchemaDiffPrepareEnvelopeVm,
};
use crate::components::schema::schema_diff_panel::{TableSchemaDiffVm, format_schema_diff_text};
use crate::views::schema_diff::schema_diff_object_identity::SchemaDiffObjectIdentityVm;
use crate::views::schema_diff::schema_diff_table_names::{
    SchemaDiffTablePickVm, TablePickOriginVm, enabled_source_table_names,
    enabled_target_only_table_names,
};
use crate::views::schema_diff::schema_diff_wizard_progress::{SCHEMA_DIFF_STEPS, SchemaDiffWizardStep};
use crate::views::schema_diff::use_schema_diff_clipboard_feedback::SchemaDiffFeedbackKind;
use crate::views::schema_diff::use_schema_diff_unified_objects::ObjectSide;

use super::state::{SavedSetupState, SchemaDiffWindowState};
use crate::commands::schema_diff::runner::{
    DeployProfileRefVm, ExecuteDeployParamsVm, PreparePlanParamsVm, PrepareUnifiedParamsVm,
    compare_table_schemas_request,
};
use crate::commands::schema_diff::{dialect_supports_transactional_ddl, export_plan_sql};

/// The plan actions' and errors' i18n keys (the React literals).
pub const KEY_TABLE_REQUIRED: &str = "schemaDiff.tableRequired";
pub const KEY_SELECTION_REQUIRED: &str = "schemaDiff.selectionRequired";
pub const KEY_CROSS_DIALECT_BLOCKED: &str = "schemaDiff.crossDialectObjectBlocked";
pub const KEY_CLIPBOARD_FAILED: &str = "schemaDiff.clipboardFailed";
pub const KEY_REPREPARE_ON_STALE_PLAN: &str = "migrationJob.reprepareOnStalePlan";

/// `value || undefined` for the schema strings: a blank (empty/whitespace)
/// reads as absent.
fn blank_to_none(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// The copy-summary join: `diffs.map(formatSchemaDiffText).join('\n\n')`.
pub fn copy_summary_text(diffs: &[TableSchemaDiffVm]) -> String {
    diffs.iter().map(format_schema_diff_text).collect::<Vec<_>>().join("\n\n")
}

/// The copy-SQL payload: `exportPlanSql(plan)`.
pub fn copy_sql_text(plan: &SchemaDiffPlanVm) -> String {
    export_plan_sql(plan)
}

/// One clipboard attempt's adoption: the feedback kind on success, the
/// clipboard-failed error on failure (React's `catch {}` arms).
pub enum ClipboardOutcome {
    /// `showClipboardFeedback(kind)`.
    Shown(SchemaDiffFeedbackKind),
    /// `setError(t('schemaDiff.clipboardFailed'))`.
    Failed,
}

// ---- loadSourceTables ---------------------------------------------------------

/// `loadSourceTables`'s begin: the validation gate first (a failed
/// validation returns without touching state), then the two clears and
/// the loading flag. Returns whether the discovery walk should start.
pub fn begin_load_source_tables(state: &mut SchemaDiffWindowState) -> bool {
    if state.endpoints.validate().is_err() {
        return false;
    }
    state.objects_loading = true;
    state.error = None;
    state.discovery_diagnostic.clear();
    true
}

/// `loadSourceTables`'s finish (the `finally` clears the loading flag on
/// every path): the picks land, or the error + diagnostic land and the
/// picks clear.
pub fn finish_load_source_tables(
    state: &mut SchemaDiffWindowState,
    outcome: Result<Vec<SchemaDiffTablePickVm>, (String, String)>,
) {
    match outcome {
        Ok(picks) => state.table_picks = picks,
        Err((message, diagnostic)) => {
            state.error = Some(message);
            state.discovery_diagnostic = diagnostic;
            state.table_picks.clear();
        }
    }
    state.objects_loading = false;
}

// ---- runCompare --------------------------------------------------------------

/// One compare unit: the target-only stub (all diff arrays empty) or
/// the `compareTableSchemas` request for a paired table.
#[derive(Debug)]
pub enum CompareJob {
    /// A `target-only` pick's stub diff — pushed without an IPC.
    TargetOnlyStub(TableSchemaDiffVm),
    /// `schemaDiffCommands.compareTableSchemas(...)`'s request.
    Compare {
        source_db_session_id: String,
        target_db_session_id: String,
        request: serde_json::Value,
    },
}

/// `runCompare`'s begin: the four artifact clears, then the validation
/// gate, then the selected-empty gate. Returns the compare units to
/// run (the selected picks in order) or the refusal's error key.
pub fn begin_run_compare(
    state: &mut SchemaDiffWindowState,
    source_db_session_id: String,
    target_db_session_id: String,
) -> Result<Vec<CompareJob>, &'static str> {
    state.error = None;
    state.diffs.clear();
    state.plan = None;
    state.plan_meta = None;
    state.deploy_result = None;
    state.endpoints.validate().map_err(|_| KEY_TABLE_REQUIRED)?;
    let selected: Vec<&SchemaDiffTablePickVm> =
        state.table_picks.iter().filter(|row| row.enabled).collect();
    if selected.is_empty() {
        return Err(KEY_TABLE_REQUIRED);
    }
    Ok(selected
        .into_iter()
        .map(|pick| {
            if pick.origin == TablePickOriginVm::TargetOnly {
                CompareJob::TargetOnlyStub(TableSchemaDiffVm {
                    table: pick.name.clone(),
                    target_only: true,
                    ..Default::default()
                })
            } else {
                let source_table = pick.source_name.clone().unwrap_or_else(|| pick.name.clone());
                let target_table = pick
                    .target_name
                    .clone()
                    .unwrap_or_else(|| pick.name.rsplit('.').next().unwrap_or("").to_owned());
                CompareJob::Compare {
                    source_db_session_id: source_db_session_id.clone(),
                    target_db_session_id: target_db_session_id.clone(),
                    request: compare_table_schemas_request(
                        &source_db_session_id,
                        &target_db_session_id,
                        &source_table,
                        &target_table,
                        blank_to_none(&state.endpoints.source().schema).as_deref(),
                        blank_to_none(&state.endpoints.target().schema).as_deref(),
                    ),
                }
            }
        })
        .collect())
}

/// `runCompare`'s finish: the diffs land in pick order and the first
/// selected name becomes the selected table (`selected[0]?.name`), or
/// the error lands.
pub fn finish_run_compare(
    state: &mut SchemaDiffWindowState,
    outcome: Result<Vec<TableSchemaDiffVm>, String>,
) {
    match outcome {
        Ok(diffs) => {
            state.selected_table = state
                .table_picks
                .iter()
                .find(|row| row.enabled)
                .map(|row| row.name.clone());
            state.diffs = diffs;
        }
        Err(message) => state.error = Some(message),
    }
    state.loading = false;
}

// ---- buildPlan ---------------------------------------------------------------

/// `buildPlan`'s begin: the error and deploy-result clears, then the
/// selection gate (no tables, no target-only tables and no objects of
/// either side → `schemaDiff.selectionRequired`), then the endpoint
/// validation. Returns whether the plan build should continue.
pub fn begin_build_plan(state: &mut SchemaDiffWindowState) -> Result<(), &'static str> {
    state.error = None;
    state.deploy_result = None;
    let source_tables = enabled_source_table_names(&state.table_picks);
    let target_only_tables = enabled_target_only_table_names(&state.table_picks);
    let source_objects = state.unified.selected_source_objects();
    let target_objects = state.unified.selected_target_objects();
    if source_tables.is_empty()
        && target_only_tables.is_empty()
        && source_objects.is_empty()
        && target_objects.is_empty()
    {
        return Err(KEY_SELECTION_REQUIRED);
    }
    state.endpoints.validate().map_err(|_| KEY_SELECTION_REQUIRED)?;
    Ok(())
}

/// The plan shape to build: the plain table plan or the unified plan
/// (any object selection rides the unified call, exactly React's
/// condition on the two object lists).
pub enum PlanShape {
    Plain(PreparePlanParamsVm),
    Unified(PrepareUnifiedParamsVm),
}

/// `buildPlan`'s `tablePlanParams` assembly, verbatim:
///
/// - `tableNames` = the enabled source tables;
/// - `targetTableNames` = the enabled non-target-only picks' target
///   names (`targetName ?? name.split('.').at(-1) ?? name`);
/// - `targetOnlyTableNames` rides ONLY when non-empty (React's
///   conditional spread in `preparePlan` — the unified shaper passes it
///   through unconditionally, so the caller's own rule is kept there);
/// - the schemas ride only when set (`|| undefined`);
/// - `typeOverrides` rides only when non-empty (`?? undefined` — an
///   empty override list is dropped, not sent as `[]`);
/// - `includeIndexes` always (the window's checkbox state).
///
/// `handleApplyTypeOverrides` is the `overrides` argument with the
/// window's current list.
pub fn build_plan_params(
    state: &SchemaDiffWindowState,
    source_db_session_id: &str,
    target_db_session_id: &str,
    overrides: &[ColumnTypeOverrideVm],
) -> PlanShape {
    let source_picks: Vec<&SchemaDiffTablePickVm> = state
        .table_picks
        .iter()
        .filter(|row| row.enabled && row.origin != TablePickOriginVm::TargetOnly)
        .collect();
    let table_names = enabled_source_table_names(&state.table_picks);
    let target_table_names: Vec<String> = source_picks
        .iter()
        .map(|row| {
            row.target_name.clone().unwrap_or_else(|| {
                row.name.rsplit('.').next().unwrap_or("").to_owned()
            })
        })
        .collect();
    let target_only_table_names = enabled_target_only_table_names(&state.table_picks);
    let source_objects = state.unified.selected_source_objects();
    let target_objects = state.unified.selected_target_objects();

    let source_schema = blank_to_none(&state.endpoints.source().schema);
    let target_schema = blank_to_none(&state.endpoints.target().schema);
    let type_overrides = (!overrides.is_empty()).then(|| overrides.to_vec());

    if source_objects.is_empty() && target_objects.is_empty() {
        PlanShape::Plain(PreparePlanParamsVm {
            source_db_session_id: source_db_session_id.to_owned(),
            target_db_session_id: target_db_session_id.to_owned(),
            table_names,
            target_table_names: Some(target_table_names),
            target_only_table_names: (!target_only_table_names.is_empty())
                .then_some(target_only_table_names),
            source_schema,
            target_schema,
            allow_destructive: state.allow_destructive,
            include_indexes: Some(state.include_indexes),
            type_overrides,
        })
    } else {
        PlanShape::Unified(PrepareUnifiedParamsVm {
            source_db_session_id: source_db_session_id.to_owned(),
            target_db_session_id: target_db_session_id.to_owned(),
            table_names,
            target_table_names: Some(target_table_names),
            target_only_table_names: Some(target_only_table_names),
            source_schema,
            target_schema,
            allow_destructive: state.allow_destructive,
            include_indexes: Some(state.include_indexes),
            type_overrides,
            source_objects,
            target_objects,
        })
    }
}

/// `buildPlan`'s finish: the plan and its envelope land, the
/// `useTransaction` checkbox adopts the target dialect's transactional
/// DDL support, and the confirm text clears. The error branch keeps
/// the previous artifacts (React only `setError`s).
pub fn finish_build_plan(
    state: &mut SchemaDiffWindowState,
    outcome: Result<SchemaDiffPrepareEnvelopeVm, String>,
) {
    match outcome {
        Ok(envelope) => {
            state.use_transaction =
                dialect_supports_transactional_ddl(&envelope.plan.target_dialect);
            state.plan = Some(envelope.plan.clone());
            state.plan_meta = Some(envelope);
            state.confirm_text.clear();
        }
        Err(message) => state.error = Some(message),
    }
    state.loading = false;
}

// ---- handleDeploy ------------------------------------------------------------

/// `executeDeploy`'s payload assembly, verbatim: the transaction flag
/// is `transactionEnabled`, the destructive confirm rides only when the
/// plan is destructive (trimmed), the endpoint database/schema ride as
/// `|| null` (`Some(None)` on the wire), the profile reference is the
/// selected saved profile's `{ id, revision: updatedAt }`, and the
/// plan id / selection revision come from the prepare envelope.
pub fn build_deploy_params(
    state: &SchemaDiffWindowState,
    target_db_session_id: &str,
) -> ExecuteDeployParamsVm {
    let plan = state.plan.clone();
    let confirm_destructive = plan
        .as_ref()
        .and_then(|plan| super::state::confirm_destructive_text(plan, &state.confirm_text));
    // `endpoints.targetDatabase || null` / `endpoints.targetSchema || null`:
    // the keys ride ALWAYS (null when absent) — `Some(None)` on the wire.
    let target_database = Some(blank_to_none(&state.endpoints.target().database));
    let target_schema = Some(blank_to_none(&state.endpoints.target().schema));
    ExecuteDeployParamsVm {
        target_db_session_id: target_db_session_id.to_owned(),
        plan,
        use_transaction: Some(state.transaction_enabled()),
        require_rollback: Some(state.require_rollback),
        confirm_destructive,
        job_id: None,
        target_database,
        target_schema,
        profile: state.selected_saved_profile().map(|profile| DeployProfileRefVm {
            id: profile.id.clone(),
            revision: profile.updated_at.clone(),
        }),
        plan_id: state.plan_meta.as_ref().map(|meta| meta.plan_id.clone()),
        selection_revision: state.plan_meta.as_ref().map(|meta| meta.selection_revision),
    }
}

/// `handleDeploy`'s outcomes: the receipt, the plain error, or the
/// stale-plan branch (`isStalePlanError` — the T376 classifier — which
/// re-arms the wizard by clearing the plan and the result).
pub enum DeployOutcome {
    Receipt(SchemaDiffDeployResultVm),
    Failed(String),
    /// `isStalePlanError(e)`: `migrationJob.reprepareOnStalePlan`, the
    /// plan clears, the deploy result clears.
    Stale,
}

/// `handleDeploy`'s finish.
pub fn finish_deploy(state: &mut SchemaDiffWindowState, outcome: DeployOutcome) {
    match outcome {
        DeployOutcome::Receipt(result) => state.deploy_result = Some(result),
        DeployOutcome::Failed(message) => state.error = Some(message),
        DeployOutcome::Stale => {
            state.error = Some(KEY_REPREPARE_ON_STALE_PLAN.to_owned());
            state.plan = None;
            state.deploy_result = None;
        }
    }
    state.loading = false;
}

// ---- goNext / goBack ---------------------------------------------------------

/// `goNext`'s full decision cascade. The host executes the decision:
/// running the compare (or the discovery load) resolves into either
/// the step advance or the stay.
#[derive(Clone, Debug, PartialEq)]
pub enum GoNextDecision {
    /// `SCHEMA_DIFF_STEPS[stepIndex + 1]` is absent: nothing to do.
    NoNextStep,
    /// The objects step's cross-dialect gate refused:
    /// `schemaDiff.crossDialectObjectBlocked`.
    CrossDialectBlocked,
    /// Nothing selected on the objects step:
    /// `schemaDiff.selectionRequired`.
    SelectionRequired,
    /// Objects selected but no tables: the compare step is skipped
    /// straight to plan (`setStep('plan')`).
    SkipToPlan,
    /// Tables are selected: `runCompare` runs first; the compare step
    /// lands only when it succeeds.
    RunCompareThenCompare,
    /// The plan step's plain advance to deploy.
    AdvanceToDeploy,
    /// The first visit to the objects step with no picks yet:
    /// `loadSourceTables` runs first; the step lands only when it
    /// succeeds.
    LoadTablesThenAdvance(SchemaDiffWizardStep),
    /// The plain `setStep(next)`.
    Advance(SchemaDiffWizardStep),
}

/// `goNext`, the gate cascade verbatim.
pub fn go_next_decision(state: &SchemaDiffWindowState) -> GoNextDecision {
    let step_index = state.step_index();
    let next = SCHEMA_DIFF_STEPS.get(step_index + 1).copied();

    if state.step == SchemaDiffWizardStep::Objects && next == Some(SchemaDiffWizardStep::Compare) {
        let source_objects = state.unified.selected_source_objects();
        let target_objects = state.unified.selected_target_objects();
        if state.endpoints.is_cross_dialect() && (!source_objects.is_empty() || !target_objects.is_empty())
        {
            return GoNextDecision::CrossDialectBlocked;
        }
        let selected_tables = state.selected_tables();
        if selected_tables.is_empty()
            && source_objects.is_empty()
            && target_objects.is_empty()
        {
            return GoNextDecision::SelectionRequired;
        }
        if selected_tables.is_empty() {
            return GoNextDecision::SkipToPlan;
        }
        return GoNextDecision::RunCompareThenCompare;
    }
    if state.step == SchemaDiffWizardStep::Plan && next == Some(SchemaDiffWizardStep::Deploy) {
        return GoNextDecision::AdvanceToDeploy;
    }
    if next == Some(SchemaDiffWizardStep::Objects) && state.table_picks.is_empty() {
        return GoNextDecision::LoadTablesThenAdvance(SchemaDiffWizardStep::Objects);
    }
    match next {
        Some(step) => GoNextDecision::Advance(step),
        None => GoNextDecision::NoNextStep,
    }
}

/// `goBack`: the previous step, or nothing on the first step.
pub fn go_back_step(state: &SchemaDiffWindowState) -> Option<SchemaDiffWizardStep> {
    let step_index = state.step_index();
    if step_index == 0 {
        return None;
    }
    SCHEMA_DIFF_STEPS.get(step_index - 1).copied()
}

// ---- the unified-object toggle ----------------------------------------------

/// `toggleUnifiedObject(side)(object)`.
pub fn toggle_unified_object(
    state: &mut SchemaDiffWindowState,
    side: ObjectSide,
    object: &SchemaDiffObjectIdentityVm,
) {
    state.unified.toggle(side, object);
}

/// The selection steps' clear-both action (`selectAll(false)` twice).
pub fn clear_object_selections(state: &mut SchemaDiffWindowState) {
    state.unified.select_all(ObjectSide::Source, false);
    state.unified.select_all(ObjectSide::Target, false);
}

// ---- the saved-setup quick state adoptions ----------------------------------

/// `setSelectedProfileId`.
pub fn set_selected_profile(setups: &mut SavedSetupState, id: impl Into<String>) {
    setups.selected_profile_id = id.into();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use crate::commands::schema_diff::wire::{
        DeployStatusVm, PlanStatementVm, StatementExecResultVm, StatementRiskVm,
    };
    use crate::views::schema_diff::schema_diff_endpoint_helpers::MigrationWindowPrefill;
    use crate::views::schema_diff::schema_diff_object_identity::SchemaDiffObjectKindVm;
    use crate::views::schema_diff::schema_diff_table_names::TablePickOriginVm;
    use crate::views::schema_diff::use_schema_diff_endpoints::{
        EndpointConnection, EndpointSide,
    };

    fn state() -> SchemaDiffWindowState {
        SchemaDiffWindowState::new(MigrationWindowPrefill::default())
    }

    fn ready_state() -> SchemaDiffWindowState {
        let mut state = state();
        state.endpoints.on_connections(vec![
            EndpointConnection {
                id: "c1".to_owned(),
                name: "A".to_owned(),
                database_type: "mysql".to_owned(),
                database: None,
                schema: None,
            },
            EndpointConnection {
                id: "c2".to_owned(),
                name: "B".to_owned(),
                database_type: "postgresql".to_owned(),
                database: None,
                schema: None,
            },
        ]);
        let _ = state.endpoints.select_side(EndpointSide::Source, "c1");
        let _ = state.endpoints.select_side(EndpointSide::Target, "c2");
        state.endpoints.select_database(EndpointSide::Source, "sdb");
        state.endpoints.select_database(EndpointSide::Target, "tdb");
        state
    }

    fn pick(name: &str, origin: TablePickOriginVm, enabled: bool) -> SchemaDiffTablePickVm {
        SchemaDiffTablePickVm {
            name: name.to_owned(),
            enabled,
            origin,
            source_name: None,
            target_name: None,
        }
    }

    fn object(name: &str) -> SchemaDiffObjectIdentityVm {
        SchemaDiffObjectIdentityVm {
            kind: SchemaDiffObjectKindVm::View,
            schema: None,
            name: name.to_owned(),
            signature: None,
            target_schema: None,
            target_name: None,
        }
    }

    #[test]
    fn the_copy_payloads_are_the_react_expressions() {
        let diff = TableSchemaDiffVm {
            table: "t".to_owned(),
            target_only: true,
            ..Default::default()
        };
        assert_eq!(copy_summary_text(&[diff.clone()]), format_schema_diff_text(&diff));
        let joined = copy_summary_text(&[diff.clone(), diff]);
        assert_eq!(joined.split("\n\n").count(), 2, "joined by a blank line");
        let plan = SchemaDiffPlanVm::default();
        assert_eq!(copy_sql_text(&plan), export_plan_sql(&plan));
    }

    #[test]
    fn load_source_tables_gates_on_validation_first() {
        let mut state = state();
        state.error = Some("old".to_owned());
        assert!(!begin_load_source_tables(&mut state), "no endpoints: refused");
        assert_eq!(state.error.as_deref(), Some("old"), "refusal touches nothing");
        assert!(!state.objects_loading);
        let mut state = ready_state();
        state.discovery_diagnostic = "diag".to_owned();
        assert!(begin_load_source_tables(&mut state));
        assert!(state.objects_loading);
        assert!(state.error.is_none());
        assert_eq!(state.discovery_diagnostic, "");
        // Finish: the happy path lands the picks; the failure path clears
        // them and carries both texts.
        finish_load_source_tables(&mut state, Ok(vec![pick("t", TablePickOriginVm::Both, true)]));
        assert_eq!(state.table_picks.len(), 1);
        assert!(!state.objects_loading);
        assert!(begin_load_source_tables(&mut state));
        finish_load_source_tables(
            &mut state,
            Err(("boom".to_owned(), "diag-2".to_owned())),
        );
        assert_eq!(state.error.as_deref(), Some("boom"));
        assert_eq!(state.discovery_diagnostic, "diag-2");
        assert!(state.table_picks.is_empty());
        assert!(!state.objects_loading);
    }

    #[test]
    fn run_compare_refuses_without_selection() {
        let mut state = ready_state();
        state.plan = Some(SchemaDiffPlanVm::default());
        state.deploy_result = Some(deploy_result());
        let refusal = begin_run_compare(&mut state, "s1".to_owned(), "t1".to_owned());
        assert_eq!(refusal.err(), Some(KEY_TABLE_REQUIRED));
        assert!(state.plan.is_none(), "the four clears ran first");
        assert!(state.deploy_result.is_none());
    }

    #[test]
    fn run_compare_assembles_both_unit_kinds() {
        let mut state = ready_state();
        state.table_picks = vec![
            pick("src.t", TablePickOriginVm::Both, true),
            pick("new_target_table", TablePickOriginVm::TargetOnly, true),
            pick("off", TablePickOriginVm::Both, false),
            pick("paired", TablePickOriginVm::Both, true),
        ];
        state.table_picks[3].source_name = Some("src.paired".to_owned());
        state.table_picks[3].target_name = Some("tgt.paired".to_owned());
        let jobs = begin_run_compare(&mut state, "s1".to_owned(), "t1".to_owned())
            .expect("valid + selected");
        assert_eq!(jobs.len(), 3, "the disabled pick is dropped");
        match &jobs[0] {
            CompareJob::Compare { request, .. } => {
                assert_eq!(request["sourceTableName"], "src.t", "sourceName ?? name");
                assert_eq!(request["targetTableName"], "t", "name.split('.').at(-1)");
                assert_eq!(request["sourceSchema"], serde_json::Value::Null);
            }
            other => panic!("expected a compare job, got {other:?}"),
        }
        match &jobs[1] {
            CompareJob::TargetOnlyStub(diff) => {
                assert_eq!(diff.table, "new_target_table");
                assert!(diff.target_only);
                assert!(diff.changed.is_empty());
            }
            other => panic!("expected a stub, got {other:?}"),
        }
        match &jobs[2] {
            CompareJob::Compare { request, .. } => {
                assert_eq!(request["sourceTableName"], "src.paired", "the explicit names win");
                assert_eq!(request["targetTableName"], "tgt.paired");
            }
            other => panic!("expected a compare job, got {other:?}"),
        }
    }

    #[test]
    fn run_compare_finish_selects_the_first_enabled_name() {
        let mut state = ready_state();
        state.table_picks = vec![
            pick("a", TablePickOriginVm::Both, true),
            pick("b", TablePickOriginVm::Both, true),
        ];
        finish_run_compare(&mut state, Ok(Vec::new()));
        assert_eq!(state.selected_table.as_deref(), Some("a"));
        assert!(state.diffs.is_empty());
        state.selected_table = None;
        finish_run_compare(&mut state, Err("boom".to_owned()));
        assert_eq!(state.error.as_deref(), Some("boom"));
        assert!(state.selected_table.is_none());
    }

    #[test]
    fn build_plan_refuses_an_empty_selection() {
        let mut state = ready_state();
        assert_eq!(begin_build_plan(&mut state).err(), Some(KEY_SELECTION_REQUIRED));
        assert!(state.error.is_none(), "the refusal is returned, not set");
        state.table_picks = vec![pick("t", TablePickOriginVm::Both, true)];
        assert!(begin_build_plan(&mut state).is_ok());
    }

    #[test]
    fn the_plain_plan_params_are_the_react_assembly() {
        let mut state = ready_state();
        state.table_picks = vec![
            pick("src.t", TablePickOriginVm::Both, true),
            pick("tgt_only", TablePickOriginVm::TargetOnly, true),
            pick("off", TablePickOriginVm::Both, false),
        ];
        state.table_picks[0].target_name = Some("tgt.t".to_owned());
        state.allow_destructive = true;
        state.include_indexes = false;
        state.set_type_override("t", "c", "BIGINT");
        let PlanShape::Plain(params) =
            build_plan_params(&state, "s1", "t1", &state.type_overrides)
        else {
            panic!("no objects: the plain shape");
        };
        assert_eq!(params.source_db_session_id, "s1");
        assert_eq!(params.target_db_session_id, "t1");
        assert_eq!(params.table_names, vec!["src.t"], "the enabled source names");
        assert_eq!(params.target_table_names.as_deref(), Some(&["tgt.t".to_owned()][..]));
        assert_eq!(
            params.target_only_table_names.as_deref(),
            Some(&["tgt_only".to_owned()][..]),
            "rides only when non-empty"
        );
        assert_eq!(params.source_schema, None, "no schema set");
        assert_eq!(params.target_schema, None);
        assert!(params.allow_destructive);
        assert_eq!(params.include_indexes, Some(false));
        assert_eq!(
            params.type_overrides.as_ref().map(|v| v.len()),
            Some(1),
            "non-empty overrides ride"
        );
    }

    #[test]
    fn the_unified_plan_shape_rides_any_object_selection() {
        let mut state = ready_state();
        state.table_picks = vec![pick("t", TablePickOriginVm::Both, true)];
        state.table_picks[0].target_name = Some("tgt.t".to_owned());
        let begin = state.unified.begin_load();
        state.unified.on_load_answer(begin, (vec![object("v1")], Vec::new()), (vec![], Vec::new()));
        state.unified.toggle(ObjectSide::Source, &object("v1"));
        let PlanShape::Unified(params) =
            build_plan_params(&state, "s1", "t1", &[])
        else {
            panic!("a source object is selected: the unified shape");
        };
        assert_eq!(params.source_objects.len(), 1);
        assert!(params.target_objects.is_empty());
        assert_eq!(params.type_overrides, None, "empty overrides drop");
        // The unified shaper passes targetOnlyTableNames through
        // unconditionally — an empty array stays an empty array.
        assert_eq!(params.target_only_table_names, Some(Vec::new()));
        // And the runner keeps it on the wire.
        let request = crate::commands::schema_diff::runner::prepare_unified_plan_request(&params);
        assert_eq!(request["targetOnlyTableNames"], json!([]));
    }

    #[test]
    fn the_plain_plan_drops_an_empty_target_only_list() {
        let mut state = ready_state();
        state.table_picks = vec![pick("t", TablePickOriginVm::Both, true)];
        let PlanShape::Plain(params) = build_plan_params(&state, "s1", "t1", &[]) else {
            panic!("plain");
        };
        assert_eq!(params.target_only_table_names, None, "the conditional spread");
        let request = crate::commands::schema_diff::runner::prepare_plan_request(&params);
        assert!(request.get("targetOnlyTableNames").is_none());
    }

    #[test]
    fn build_plan_finish_adopts_the_dialect_rule() {
        let mut state = ready_state();
        state.use_transaction = false;
        state.confirm_text = "typed".to_owned();
        let mut envelope = envelope_with_dialect("postgresql");
        envelope.selection_revision = 7;
        finish_build_plan(&mut state, Ok(envelope));
        assert!(state.use_transaction, "transactional dialect re-enables it");
        assert!(state.plan.is_some());
        assert_eq!(state.plan_meta.as_ref().map(|meta| meta.selection_revision), Some(7));
        assert_eq!(state.confirm_text, "", "the confirm text clears");

        finish_build_plan(&mut state, Ok(envelope_with_dialect("mysql")));
        assert!(!state.use_transaction, "mysql has no transactional DDL");
        assert!(state.error.is_none());

        finish_build_plan(&mut state, Err("boom".to_owned()));
        assert_eq!(state.error.as_deref(), Some("boom"));
        assert!(state.plan.is_some(), "the error branch keeps the plan");
    }

    #[test]
    fn the_deploy_payload_is_the_react_assembly() {
        let mut state = ready_state();
        state.endpoints.select_schema(EndpointSide::Target, "tschema");
        state.plan = Some(plan_with_statement());
        state.require_rollback = true;
        state.confirm_text = "  DEPLOY  ".to_owned();
        state.saved_setups.profiles = vec![profile("p1", "2026-01-01T00:00:00Z")];
        state.saved_setups.selected_profile_id = "p1".to_owned();
        state.plan_meta = Some(envelope_with_dialect("mysql"));
        state.plan_meta.as_mut().map(|meta| {
            meta.plan_id = "plan-9".to_owned();
            meta.selection_revision = 4;
        });

        let params = build_deploy_params(&state, "t1");
        assert_eq!(params.target_db_session_id, "t1");
        assert!(params.plan.is_some());
        assert_eq!(params.use_transaction, Some(true), "transactionEnabled");
        assert_eq!(params.require_rollback, Some(true));
        assert_eq!(
            params.confirm_destructive.as_deref(),
            Some("DEPLOY"),
            "destructive plan: the trimmed confirm rides"
        );
        assert_eq!(params.target_database, Some(Some("tdb".to_owned())), "targetDatabase || null rides the value");
        assert_eq!(params.target_schema, Some(Some("tschema".to_owned())));
        assert_eq!(
            params.profile.as_ref().map(|p| (p.id.as_str(), p.revision.as_str())),
            Some(("p1", "2026-01-01T00:00:00Z")),
            "the selected profile's id + updatedAt"
        );
        assert_eq!(params.plan_id.as_deref(), Some("plan-9"));
        assert_eq!(params.selection_revision, Some(4));

        // A safe plan drops the confirm text; no selected profile drops
        // the reference; no envelope drops the plan id.
        state.plan = Some(SchemaDiffPlanVm {
            statements: vec![PlanStatementVm {
                sql: "SELECT 1".to_owned(),
                risk: StatementRiskVm::Additive,
                rollback_sql: None,
                summary: String::new(),
                requires_transaction: None,
            }],
            ..Default::default()
        });
        state.saved_setups.selected_profile_id = String::new();
        state.plan_meta = None;
        let params = build_deploy_params(&state, "t1");
        assert_eq!(params.confirm_destructive, None);
        assert_eq!(params.profile, None);
        assert_eq!(params.plan_id, None);
        assert_eq!(params.selection_revision, None);
    }

    #[test]
    fn the_deploy_outcomes_adopt_verbatim() {
        let mut state = ready_state();
        state.plan = Some(plan_with_statement());
        finish_deploy(&mut state, DeployOutcome::Receipt(deploy_result()));
        assert!(state.deploy_result.is_some());
        assert!(state.plan.is_some());

        finish_deploy(&mut state, DeployOutcome::Failed("boom".to_owned()));
        assert_eq!(state.error.as_deref(), Some("boom"));

        state.plan = Some(plan_with_statement());
        state.deploy_result = Some(deploy_result());
        finish_deploy(&mut state, DeployOutcome::Stale);
        assert_eq!(state.error.as_deref(), Some(KEY_REPREPARE_ON_STALE_PLAN));
        assert!(state.plan.is_none(), "a stale plan re-arms the wizard");
        assert!(state.deploy_result.is_none());
        assert!(!state.loading);
    }

    #[test]
    fn go_next_is_the_full_cascade() {
        // Same-dialect endpoints (cross-dialect gets its own segment at
        // the end — the block gate precedes everything else).
        let mut state = state();
        state.endpoints.on_connections(vec![
            EndpointConnection {
                id: "c1".to_owned(),
                name: "A".to_owned(),
                database_type: "mysql".to_owned(),
                database: None,
                schema: None,
            },
            EndpointConnection {
                id: "c2".to_owned(),
                name: "B".to_owned(),
                database_type: "mysql".to_owned(),
                database: None,
                schema: None,
            },
        ]);
        let _ = state.endpoints.select_side(EndpointSide::Source, "c1");
        let _ = state.endpoints.select_side(EndpointSide::Target, "c2");
        state.endpoints.select_database(EndpointSide::Source, "sdb");
        state.endpoints.select_database(EndpointSide::Target, "tdb");

        // endpoints → objects: picks empty → the discovery load runs first.
        assert_eq!(
            go_next_decision(&state),
            GoNextDecision::LoadTablesThenAdvance(SchemaDiffWizardStep::Objects)
        );
        state.table_picks = vec![pick("t", TablePickOriginVm::Both, true)];
        assert_eq!(
            go_next_decision(&state),
            GoNextDecision::Advance(SchemaDiffWizardStep::Objects)
        );

        // objects → compare: the three gates, in order.
        state.step = SchemaDiffWizardStep::Objects;
        assert_eq!(go_next_decision(&state), GoNextDecision::RunCompareThenCompare);
        state.table_picks.clear();
        assert_eq!(go_next_decision(&state), GoNextDecision::SelectionRequired);
        // Objects only: skip straight to plan.
        let begin = state.unified.begin_load();
        state.unified.on_load_answer(begin, (vec![], Vec::new()), (vec![object("v1")], Vec::new()));
        state.unified.toggle(ObjectSide::Target, &object("v1"));
        assert_eq!(go_next_decision(&state), GoNextDecision::SkipToPlan);
        // ...and objects only with TABLES still compares.
        state.table_picks = vec![pick("t", TablePickOriginVm::Both, true)];
        assert_eq!(go_next_decision(&state), GoNextDecision::RunCompareThenCompare);

        // compare → plan, plan → deploy, deploy → nothing.
        state.step = SchemaDiffWizardStep::Compare;
        assert_eq!(go_next_decision(&state), GoNextDecision::Advance(SchemaDiffWizardStep::Plan));
        state.step = SchemaDiffWizardStep::Plan;
        assert_eq!(go_next_decision(&state), GoNextDecision::AdvanceToDeploy);
        state.step = SchemaDiffWizardStep::Deploy;
        assert_eq!(go_next_decision(&state), GoNextDecision::NoNextStep);

        // Cross-dialect with objects: blocked outright (the first gate).
        let mut cross = state;
        cross.endpoints.on_connections(vec![
            EndpointConnection {
                id: "c1".to_owned(),
                name: "A".to_owned(),
                database_type: "mysql".to_owned(),
                database: None,
                schema: None,
            },
            EndpointConnection {
                id: "c3".to_owned(),
                name: "C".to_owned(),
                database_type: "sqlite".to_owned(),
                database: None,
                schema: None,
            },
        ]);
        let _ = cross.endpoints.select_side(EndpointSide::Target, "c3");
        cross.endpoints.select_database(EndpointSide::Target, "tdb");
        assert!(cross.endpoints.is_cross_dialect());
        cross.step = SchemaDiffWizardStep::Objects;
        assert_eq!(go_next_decision(&cross), GoNextDecision::CrossDialectBlocked);
        // ...but cross-dialect with TABLES still compares.
        cross.table_picks = vec![pick("t", TablePickOriginVm::Both, true)];
        let begin = cross.unified.begin_load();
        cross.unified.on_load_answer(begin, (vec![], Vec::new()), (vec![], Vec::new()));
        assert_eq!(go_next_decision(&cross), GoNextDecision::RunCompareThenCompare);
    }

    #[test]
    fn go_back_walks_the_rail() {
        let mut state = state();
        assert_eq!(go_back_step(&state), None, "the first step has no back");
        state.step = SchemaDiffWizardStep::Plan;
        assert_eq!(go_back_step(&state), Some(SchemaDiffWizardStep::Compare));
        state.step = SchemaDiffWizardStep::Deploy;
        assert_eq!(go_back_step(&state), Some(SchemaDiffWizardStep::Plan));
    }

    #[test]
    fn the_object_toggles_forward_to_the_hook() {
        let mut state = state();
        let begin = state.unified.begin_load();
        state
            .unified
            .on_load_answer(begin, (vec![object("v1"), object("v2")], Vec::new()), (vec![], Vec::new()));
        toggle_unified_object(&mut state, ObjectSide::Source, &object("v1"));
        assert_eq!(state.unified.selected_source_keys().len(), 1);
        let begin = state.unified.begin_load();
        state.unified.on_load_answer(begin, (vec![], Vec::new()), (vec![object("v3")], Vec::new()));
        toggle_unified_object(&mut state, ObjectSide::Target, &object("v3"));
        clear_object_selections(&mut state);
        assert!(state.unified.selected_source_keys().is_empty());
        assert!(state.unified.selected_target_keys().is_empty());
    }

    // ---- helpers -------------------------------------------------------------

    fn plan_with_statement() -> SchemaDiffPlanVm {
        SchemaDiffPlanVm {
            target_dialect: "mysql".to_owned(),
            statements: vec![PlanStatementVm {
                sql: "DROP TABLE t".to_owned(),
                risk: StatementRiskVm::Destructive,
                rollback_sql: None,
                summary: String::new(),
                requires_transaction: None,
            }],
            ..Default::default()
        }
    }

    fn envelope_with_dialect(dialect: &str) -> SchemaDiffPrepareEnvelopeVm {
        SchemaDiffPrepareEnvelopeVm {
            plan: SchemaDiffPlanVm {
                target_dialect: dialect.to_owned(),
                ..Default::default()
            },
            plan_id: "p1".to_owned(),
            source_connection_id: None,
            target_connection_id: None,
            selection_revision: 1,
            plan_version: 1,
            handler_version: 1,
            checkpoint_version: 1,
            expires_at: String::new(),
            recovery_policy: String::new(),
        }
    }

    fn deploy_result() -> SchemaDiffDeployResultVm {
        SchemaDiffDeployResultVm {
            status: DeployStatusVm::Committed,
            executed_count: 0,
            statement_count: 0,
            errors: Vec::new(),
            statement_results: Vec::<StatementExecResultVm>::new(),
        }
    }

    fn profile(id: &str, updated_at: &str) -> crate::commands::schema_diff::wire::SchemaDiffProfileVm {
        crate::commands::schema_diff::wire::SchemaDiffProfileVm {
            version: 2,
            id: id.to_owned(),
            name: id.to_owned(),
            source_connection_id: String::new(),
            target_connection_id: String::new(),
            source_database: String::new(),
            target_database: String::new(),
            source_schema: None,
            target_schema: None,
            tables: Vec::new(),
            target_only_tables: None,
            source_objects: None,
            target_objects: None,
            allow_destructive: false,
            include_indexes: true,
            require_rollback: false,
            type_overrides: None,
            created_at: String::new(),
            updated_at: updated_at.to_owned(),
        }
    }
}
