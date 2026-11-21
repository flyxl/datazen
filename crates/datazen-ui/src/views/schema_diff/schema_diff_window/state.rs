//! Port of `packages/frontend/src/windows/schema-diff/
//! SchemaDiffWindow.tsx` (T309) — the state half.
//!
//! The wizard's orchestrating state: the five `useState` families React
//! declares in one component (the step, the table picks, the compare
//! artifacts, the plan-side inputs, the chrome flags) plus the five
//! composed hook states, and every render-time derivation and every
//! effect-shaped state transition that is pure bookkeeping.
//!
//! ## What maps where
//!
//! - The composed hooks are the ported hook states verbatim:
//!   [`UseSchemaDiffEndpoints`] (T384), [`UseSchemaDiffUnifiedObjects`]
//!   (T420), [`SchemaDiffJobLifecycle`] (T391),
//!   [`SchemaDiffClipboardFeedback`] (T372). The job-restore effect hook
//!   is the T402 pure function, fired by the host when job details
//!   arrive ([`SchemaDiffWindowState::adopt_restored_deploy_result`]).
//! - The saved-setup hook's *logic* is T411's pure functions; the window
//!   owns the hook's plain state — the profile list, the selected
//!   profile id, the two dialogs' open flags and texts, and the
//!   pending-profile / pending-config refs the preservation rule reads.
//! - `useResizable` (T404, not landed) is [`TABLE_LIST_WIDTH`]'s bounds
//!   rule in one place: initial 200, min 120, max 400, the
//!   `schema-diff.table-list` storage key. The width itself is plain
//!   state the host clamps.
//! - The endpoints-change reset effect is
//!   [`SchemaDiffWindowState::on_endpoints_changed`]: the
//!   `shouldPreserveEndpointChange` gate first (T411's rule, the
//!   snapshot built from the *previous* endpoint values), then the
//!   eight-field clear.
//! - The selected-table fallback effect is
//!   [`SchemaDiffWindowState::on_selected_tables_changed`].
//! - The limitations auto-open mount effect is
//!   [`SchemaDiffWindowState::limitations_auto_open`]: the window opens
//!   the dialog when the dismissal pref has not been set (the pref read
//!   is the host's).
//! - The plan auto-request effect is
//!   [`SchemaDiffWindowState::plan_auto_request_due`] /
//!   [`arm_plan_auto_request`] /
//!   [`SchemaDiffWindowState::sync_plan_auto_request`]: armed exactly
//!   once per plan-step visit, disarmed when the step is left.
//! - `localesReady` (the `useLocaleDomains(['sync'])` gate) is elided,
//!   the T018 ruling: the zh-CN table is compiled in, so the window
//!   never renders half-translated keys.
//!
//! Known deltas: React's `planAutoRequestedRef` is a plain bool (the
//! render loop cannot interleave); `error` is `Option<String>` where
//! React uses `''`/truthiness.

use std::collections::HashMap;

use super::super::schema_diff_wizard_progress::{SCHEMA_DIFF_STEPS, SchemaDiffWizardStep};
use crate::commands::schema_diff::wire::{
    ColumnTypeOverrideVm, PlanRequirementKindVm, PlanRequirementVm, SchemaDiffDeployResultVm,
    SchemaDiffPlanVm, SchemaDiffPrepareEnvelopeVm, SchemaDiffProfileVm,
};
use crate::components::schema::schema_diff_panel::TableSchemaDiffVm;
use crate::views::schema_diff::schema_diff_endpoint_helpers::MigrationWindowPrefill;
use crate::views::schema_diff::schema_diff_table_names::{
    SchemaDiffTablePickVm, enabled_table_names, table_diff_has_changes,
};
use crate::views::schema_diff::use_schema_diff_clipboard_feedback::SchemaDiffClipboardFeedback;
use crate::views::schema_diff::use_schema_diff_endpoints::UseSchemaDiffEndpoints;
use crate::views::schema_diff::use_schema_diff_job_lifecycle::SchemaDiffJobLifecycle;
use crate::views::schema_diff::use_schema_diff_saved_setups::{EndpointSnapshot, should_preserve_endpoint_change};
use crate::views::schema_diff::use_schema_diff_unified_objects::UseSchemaDiffUnifiedObjects;

/// `useResizable({ direction: 'horizontal', initialSize: 200, minSize: 120,
/// maxSize: 400, storageKey: 'schema-diff.table-list' })` — the table-list
/// rail's width rule in one place: (initial, min, max).
pub(crate) const TABLE_LIST_WIDTH: (f32, f32, f32) = (200.0, 120.0, 400.0);
/// The resizable's storage key.
pub(crate) const TABLE_LIST_WIDTH_KEY: &str = "schema-diff.table-list";

/// `planHasDestructive(plan) ? confirmText.trim() : undefined` — the
/// destructive-confirm text rides only when the plan is destructive.
pub(crate) fn confirm_destructive_text(
    plan: &SchemaDiffPlanVm,
    confirm_text: &str,
) -> Option<String> {
    if crate::commands::schema_diff::plan_has_destructive(plan) {
        Some(confirm_text.trim().to_owned())
    } else {
        None
    }
}

/// The window's saved-setup hook state (the `useSchemaDiffSavedSetups`
/// plain fields; the flows themselves are T411's pure functions).
#[derive(Clone, Debug, Default)]
pub(crate) struct SavedSetupState {
    /// `profiles` (the host delivers the list from `get_profiles_request`).
    pub(crate) profiles: Vec<SchemaDiffProfileVm>,
    /// `selectedProfileId` (`''` = the placeholder).
    pub(crate) selected_profile_id: String,
    /// `profileDialogOpen` / `profileName` / `profileError`.
    pub(crate) save_open: bool,
    pub(crate) save_name: String,
    pub(crate) profile_error: Option<String>,
    /// `importOpen` / `importText` / `importConfigError`.
    pub(crate) import_open: bool,
    pub(crate) import_text: String,
    pub(crate) import_error: Option<String>,
    /// The pending-profile preservation ref (`pendingProfileRef`) —
    /// `Some` from `handleLoadProfile` until its endpoints settle.
    pub(crate) pending_profile: Option<SchemaDiffProfileVm>,
    /// The pending-config preservation ref (`pendingConfigRef`).
    pub(crate) pending_config: Option<crate::views::schema_diff::use_schema_diff_saved_setups::SchemaDiffConfigEndpointIdentity>,
}

/// The wizard window's whole state (the React component's hooks,
/// composed).
pub(crate) struct SchemaDiffWindowState {
    /// `step` (`'endpoints'` to start).
    pub(crate) step: SchemaDiffWizardStep,
    /// `tablePicks` (the objects step's rows).
    pub(crate) table_picks: Vec<SchemaDiffTablePickVm>,
    /// `objectsLoading`.
    pub(crate) objects_loading: bool,
    /// `unifiedObjects` (the T420 hook state).
    pub(crate) unified: UseSchemaDiffUnifiedObjects,
    /// `diffs` (the compare step's artifacts).
    pub(crate) diffs: Vec<TableSchemaDiffVm>,
    /// `plan`.
    pub(crate) plan: Option<SchemaDiffPlanVm>,
    /// `planMeta` (the prepare envelope).
    pub(crate) plan_meta: Option<SchemaDiffPrepareEnvelopeVm>,
    /// `selectedTable`.
    pub(crate) selected_table: Option<String>,
    /// `allowDestructive` (`false`).
    pub(crate) allow_destructive: bool,
    /// `includeIndexes` (`true`).
    pub(crate) include_indexes: bool,
    /// `requireRollback` (`false`).
    pub(crate) require_rollback: bool,
    /// `useTransaction` (`true`).
    pub(crate) use_transaction: bool,
    /// `confirmText` (`''`).
    pub(crate) confirm_text: String,
    /// `deployResult`.
    pub(crate) deploy_result: Option<SchemaDiffDeployResultVm>,
    /// `loading` (the compare/plan/deploy in-flight flag).
    pub(crate) loading: bool,
    /// `error` (`''` = `None`).
    pub(crate) error: Option<String>,
    /// `clipboardFeedback` / `showClipboardFeedback` (the T372 hook).
    pub(crate) clipboard: SchemaDiffClipboardFeedback,
    /// `limitationsOpen`.
    pub(crate) limitations_open: bool,
    /// `typeOverrides`.
    pub(crate) type_overrides: Vec<ColumnTypeOverrideVm>,
    /// `planAutoRequestedRef.current`.
    pub(crate) plan_auto_requested: bool,
    /// `tableListWidth` (the resizable's value; the host clamps to
    /// [`TABLE_LIST_WIDTH`]).
    pub(crate) table_list_width: f32,
    /// `discoveryDiagnostic` (the root's `data-discovery-failure`).
    pub(crate) discovery_diagnostic: String,
    /// The endpoints hook state (T384).
    pub(crate) endpoints: UseSchemaDiffEndpoints,
    /// The job-tracking hook state (T391).
    pub(crate) job_lifecycle: SchemaDiffJobLifecycle,
    /// The saved-setup hook's plain state.
    pub(crate) saved_setups: SavedSetupState,
}

impl Default for SchemaDiffWindowState {
    fn default() -> Self {
        Self::new(MigrationWindowPrefill::default())
    }
}

impl SchemaDiffWindowState {
    /// React's start: every `useState` at its initial value, the hooks
    /// at theirs, and the table-list rail at the resizable's initial
    /// width.
    pub(crate) fn new(prefill: MigrationWindowPrefill) -> Self {
        Self {
            step: SchemaDiffWizardStep::Endpoints,
            table_picks: Vec::new(),
            objects_loading: false,
            unified: UseSchemaDiffUnifiedObjects::new(),
            diffs: Vec::new(),
            plan: None,
            plan_meta: None,
            selected_table: None,
            allow_destructive: false,
            include_indexes: true,
            require_rollback: false,
            use_transaction: true,
            confirm_text: String::new(),
            deploy_result: None,
            loading: false,
            error: None,
            clipboard: SchemaDiffClipboardFeedback::default(),
            limitations_open: false,
            type_overrides: Vec::new(),
            plan_auto_requested: false,
            table_list_width: TABLE_LIST_WIDTH.0,
            discovery_diagnostic: String::new(),
            endpoints: UseSchemaDiffEndpoints::new(prefill),
            job_lifecycle: SchemaDiffJobLifecycle::default(),
            saved_setups: SavedSetupState::default(),
        }
    }

    // ---- the render-time derivations (React's useMemo bodies) -------------

    /// `selectedTables = enabledTableNames(tablePicks)`.
    pub(crate) fn selected_tables(&self) -> Vec<String> {
        enabled_table_names(&self.table_picks)
    }

    /// `tableHasDiff`: one `tableDiffHasChanges(diff)` per diff, by name.
    pub(crate) fn table_has_diff(&self) -> HashMap<String, bool> {
        self.diffs
            .iter()
            .map(|diff| (diff.table.clone(), table_diff_has_changes(diff)))
            .collect()
    }

    /// `selectedDiff = diffs.find(d => d.table === selectedTable) ?? null`.
    pub(crate) fn selected_diff(&self) -> Option<&TableSchemaDiffVm> {
        let selected = self.selected_table.as_deref()?;
        self.diffs.iter().find(|diff| diff.table == selected)
    }

    /// `targetLabel`: `` `${c.name} (${c.databaseType})` `` or the bare
    /// `targetId` when no connection is loaded.
    pub(crate) fn target_label(&self) -> String {
        match self.endpoints.target_conn() {
            Some(conn) => format!("{} ({})", conn.name, conn.database_type),
            None => self.endpoints.target().id.clone(),
        }
    }

    /// `stepIndex = SCHEMA_DIFF_STEPS.indexOf(step)`.
    pub(crate) fn step_index(&self) -> usize {
        self.step.index_in(&SCHEMA_DIFF_STEPS).unwrap_or_default()
    }

    /// `selectedSavedProfile = profiles.find(p => p.id === selectedProfileId)`.
    pub(crate) fn selected_saved_profile(&self) -> Option<&SchemaDiffProfileVm> {
        let id = self.saved_setups.selected_profile_id.as_str();
        if id.is_empty() {
            return None;
        }
        self.saved_setups.profiles.iter().find(|profile| profile.id == id)
    }

    /// `transactionRequired = plan?.statements.some(s =>
    /// s.requiresTransaction)` (`?? false`).
    pub(crate) fn transaction_required(&self) -> bool {
        self.plan.as_ref().is_some_and(|plan| {
            plan.statements.iter().any(|s| s.requires_transaction.unwrap_or(false))
        })
    }

    /// `transactionEnabled = transactionRequired || useTransaction` —
    /// the WINDOW's spelling (the deploy panel's own derivation differs;
    /// see `schema_diff_deploy_panel::deploy_readiness`).
    pub(crate) fn transaction_enabled(&self) -> bool {
        self.transaction_required() || self.use_transaction
    }

    /// `blockingRequirements`: only `Unsupported` blocks — `Backfill` is
    /// the plan being safe (a NOT NULL column added nullable), and
    /// counting it left the plan permanently undeployable.
    pub(crate) fn blocking_requirements(&self) -> Vec<&PlanRequirementVm> {
        self.plan
            .as_ref()
            .map(|plan| {
                plan.requirements
                    .iter()
                    .filter(|requirement| requirement.kind == PlanRequirementKindVm::Unsupported)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The window's `deployAllowed`, verbatim.
    pub(crate) fn deploy_allowed(&self) -> bool {
        let Some(plan) = self.plan.as_ref() else {
            return false;
        };
        if self.deploy_result.is_some() {
            return false;
        }
        if !self.blocking_requirements().is_empty() {
            return false;
        }
        let tx_supported =
            crate::commands::schema_diff::dialect_supports_transactional_ddl(&plan.target_dialect);
        let transaction_enabled = self.transaction_enabled();
        if self.transaction_required() && !(transaction_enabled && tx_supported) {
            return false;
        }
        if self.require_rollback && !(transaction_enabled && tx_supported) {
            return false;
        }
        crate::views::schema_diff::schema_diff_deploy_panel::can_run_deploy(
            crate::commands::schema_diff::plan_has_destructive(plan),
            &self.confirm_text,
            self.require_rollback,
            plan.rollback_completeness.complete,
            plan.statements.len(),
        )
    }

    /// The window's `canNext`, per step.
    pub(crate) fn can_next(&self) -> bool {
        match self.step {
            SchemaDiffWizardStep::Endpoints => {
                let source = self.endpoints.source();
                let target = self.endpoints.target();
                !source.id.is_empty()
                    && !target.id.is_empty()
                    && !source.database.is_empty()
                    && !target.database.is_empty()
                    && !self.endpoints.is_same_endpoint()
            }
            SchemaDiffWizardStep::Objects => !self.objects_loading,
            SchemaDiffWizardStep::Compare => !self.diffs.is_empty() && !self.loading,
            SchemaDiffWizardStep::Plan => self.plan.is_some() && !self.loading,
            SchemaDiffWizardStep::Deploy => false,
        }
    }

    // ---- the effect-shaped pure transitions ---------------------------------

    /// `clearEndpointError(message)`: the error clears only when it is
    /// exactly that message.
    pub(crate) fn clear_endpoint_error(&mut self, message: &str) {
        if self.error.as_deref() == Some(message) {
            self.error = None;
        }
    }

    /// The endpoints-change reset effect. `previous` is the endpoint
    /// snapshot from *before* the change (React's effect reads the deps
    /// diff; the host supplies the old values); the gate is T411's
    /// preservation rule over the saved-setup pending refs. Returns
    /// whether the reset was skipped (preserved).
    pub(crate) fn on_endpoints_changed(&mut self, previous: &EndpointSnapshot) -> bool {
        // The rule reads the pending values as immutable snapshots while
        // consuming the refs; clone the snapshots out, then move the
        // refs (the T411 test call shape).
        let pending_profile = self.saved_setups.pending_profile.clone();
        let pending_config = self.saved_setups.pending_config.clone();
        let mut profile_ref = self.saved_setups.pending_profile.take();
        let mut config_ref = self.saved_setups.pending_config.take();
        let preserved = should_preserve_endpoint_change(
            previous,
            pending_profile.as_ref(),
            pending_config.as_ref(),
            &mut profile_ref,
            &mut config_ref,
        );
        self.saved_setups.pending_profile = profile_ref;
        self.saved_setups.pending_config = config_ref;
        if preserved {
            return true;
        }
        self.table_picks.clear();
        self.unified.clear();
        self.diffs.clear();
        self.plan = None;
        self.plan_meta = None;
        self.deploy_result = None;
        self.selected_table = None;
        self.type_overrides.clear();
        self.plan_auto_requested = false;
        false
    }

    /// The selected-table fallback effect: nothing selected stays
    /// nothing; a still-listed selection survives; anything else falls
    /// back to the first selected table.
    pub(crate) fn on_selected_tables_changed(&mut self, selected_tables: &[String]) {
        if selected_tables.is_empty() {
            self.selected_table = None;
            return;
        }
        let keep = self
            .selected_table
            .as_deref()
            .is_some_and(|current| selected_tables.iter().any(|name| name == current));
        if !keep {
            self.selected_table = selected_tables.first().cloned();
        }
    }

    /// `toggleTable(name)`: one row's enabled flag flips.
    pub(crate) fn toggle_table(&mut self, name: &str) {
        for pick in &mut self.table_picks {
            if pick.name == name {
                pick.enabled = !pick.enabled;
            }
        }
    }

    /// `handleTypeOverrideChange(table, column, targetType)`: the prior
    /// override for that cell is replaced, never duplicated.
    pub(crate) fn set_type_override(&mut self, table: &str, column: &str, target_type: &str) {
        self.type_overrides
            .retain(|existing| !(existing.table == table && existing.column == column));
        self.type_overrides.push(ColumnTypeOverrideVm {
            table: table.to_owned(),
            column: column.to_owned(),
            target_type: target_type.to_owned(),
        });
    }

    /// The plan-step auto-request gate: armed exactly once per visit
    /// (`planAutoRequestedRef`), and only while the plan is absent.
    pub(crate) fn plan_auto_request_due(&self) -> bool {
        self.step == SchemaDiffWizardStep::Plan
            && self.plan.is_none()
            && !self.loading
            && !self.plan_auto_requested
    }

    /// The effect's arm half (`planAutoRequestedRef.current = true`).
    pub(crate) fn arm_plan_auto_request(&mut self) {
        self.plan_auto_requested = true;
    }

    /// The effect's disarm half: leaving the plan step resets the ref.
    pub(crate) fn sync_plan_auto_request(&mut self) {
        if self.step != SchemaDiffWizardStep::Plan {
            self.plan_auto_requested = false;
        }
    }

    /// The mount effect's gate: the limitations dialog opens when the
    /// dismissal pref has not been set (the pref read is the host's).
    pub(crate) fn limitations_auto_open(&mut self, dismissed: bool) {
        if !dismissed {
            self.limitations_open = true;
        }
    }

    /// The job-restore effect hook (T402): only a locally-submitted
    /// job's deploy result is adopted.
    pub(crate) fn adopt_restored_deploy_result(
        &mut self,
        restored: Option<SchemaDiffDeployResultVm>,
    ) {
        if let Some(result) = restored {
            self.deploy_result = Some(result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::schema_diff::wire::{
        DeployStatusVm, PlanStatementVm, StatementExecResultVm, StatementRiskVm,
    };
    use crate::views::schema_diff::use_schema_diff_endpoints::{
        EndpointConnection, EndpointSide,
    };

    fn state() -> SchemaDiffWindowState {
        SchemaDiffWindowState::new(MigrationWindowPrefill::default())
    }

    fn conn(id: &str, name: &str, db_type: &str) -> EndpointConnection {
        EndpointConnection {
            id: id.to_owned(),
            name: name.to_owned(),
            database_type: db_type.to_owned(),
            database: None,
            schema: None,
        }
    }

    #[test]
    fn the_initial_state_is_the_react_initials() {
        let state = state();
        assert_eq!(state.step, SchemaDiffWizardStep::Endpoints);
        assert!(state.table_picks.is_empty());
        assert!(!state.objects_loading);
        assert!(state.diffs.is_empty());
        assert!(state.plan.is_none());
        assert!(state.plan_meta.is_none());
        assert!(state.selected_table.is_none());
        assert!(!state.allow_destructive);
        assert!(state.include_indexes, "includeIndexes starts true");
        assert!(!state.require_rollback);
        assert!(state.use_transaction, "useTransaction starts true");
        assert_eq!(state.confirm_text, "");
        assert!(state.deploy_result.is_none());
        assert!(!state.loading);
        assert!(state.error.is_none());
        assert!(!state.limitations_open);
        assert!(state.type_overrides.is_empty());
        assert!(!state.plan_auto_requested);
        assert_eq!(state.table_list_width, 200.0);
        assert_eq!(state.discovery_diagnostic, "");
    }

    #[test]
    fn the_resizable_rule_is_the_react_literal() {
        assert_eq!(TABLE_LIST_WIDTH, (200.0, 120.0, 400.0));
        assert_eq!(TABLE_LIST_WIDTH_KEY, "schema-diff.table-list");
    }

    #[test]
    fn the_target_label_prefers_the_connection() {
        let mut state = state();
        state.endpoints.on_connections(vec![conn("c1", "Prod", "mysql")]);
        assert_eq!(state.endpoints.target().id, "", "no target selected yet");
        assert_eq!(state.target_label(), "");
        let _ = state.endpoints.select_side(EndpointSide::Target, "c1");
        assert_eq!(state.target_label(), "Prod (mysql)");
    }

    #[test]
    fn the_selected_table_fallback_follows_the_list() {
        let mut state = state();
        state.on_selected_tables_changed(&[]);
        assert!(state.selected_table.is_none());
        state.on_selected_tables_changed(&["a".to_owned(), "b".to_owned()]);
        assert_eq!(state.selected_table.as_deref(), Some("a"));
        state.selected_table = Some("b".to_owned());
        state.on_selected_tables_changed(&["a".to_owned(), "b".to_owned()]);
        assert_eq!(state.selected_table.as_deref(), Some("b"), "still listed: kept");
        state.on_selected_tables_changed(&["c".to_owned()]);
        assert_eq!(state.selected_table.as_deref(), Some("c"), "fell back to first");
    }

    #[test]
    fn a_type_override_replaces_its_own_cell_only() {
        let mut state = state();
        state.set_type_override("t", "c1", "INT");
        state.set_type_override("t", "c1", "BIGINT");
        state.set_type_override("t", "c2", "TEXT");
        state.set_type_override("u", "c1", "INT");
        assert_eq!(state.type_overrides.len(), 3);
        assert_eq!(state.type_overrides[0].target_type, "BIGINT", "t.c1 replaced");
    }

    #[test]
    fn clear_endpoint_error_only_clears_the_matching_message() {
        let mut state = state();
        state.error = Some("boom".to_owned());
        state.clear_endpoint_error("other");
        assert_eq!(state.error.as_deref(), Some("boom"));
        state.clear_endpoint_error("boom");
        assert!(state.error.is_none());
    }

    #[test]
    fn transaction_rules_follow_the_window_spelling() {
        let mut state = state();
        assert!(!state.transaction_required());
        assert!(state.transaction_enabled(), "useTransaction starts true");
        state.use_transaction = false;
        assert!(!state.transaction_enabled());
        state.plan = Some(plan_with_transaction_statement(true));
        assert!(state.transaction_required());
        assert!(state.transaction_enabled(), "required forces enabled");
    }

    #[test]
    fn only_unsupported_requirements_block() {
        let mut state = state();
        state.plan = Some(SchemaDiffPlanVm {
            statements: vec![PlanStatementVm {
                sql: "ALTER".to_owned(),
                risk: StatementRiskVm::Additive,
                rollback_sql: None,
                summary: String::new(),
                requires_transaction: None,
            }],
            requirements: vec![
                PlanRequirementVm {
                    kind: PlanRequirementKindVm::Backfill,
                    table: "t".to_owned(),
                    column: "c".to_owned(),
                    reason: String::new(),
                },
                PlanRequirementVm {
                    kind: PlanRequirementKindVm::Unsupported,
                    table: "t".to_owned(),
                    column: "c".to_owned(),
                    reason: String::new(),
                },
            ],
            ..plan_default()
        });
        assert_eq!(state.blocking_requirements().len(), 1, "Backfill is not a blocker");
        assert!(
            !state.deploy_allowed(),
            "an Unsupported requirement blocks the deploy (Backfill would not)"
        );
    }

    #[test]
    fn can_next_per_step_is_the_switch_verbatim() {
        let mut state = state();
        // endpoints: everything empty → false.
        assert!(!state.can_next());
        state
            .endpoints
            .on_connections(vec![conn("c1", "A", "mysql"), conn("c2", "B", "postgresql")]);
        let _ = state.endpoints.select_side(EndpointSide::Source, "c1");
        let _ = state.endpoints.select_side(EndpointSide::Target, "c2");
        assert!(!state.can_next(), "databases still empty");
        state.endpoints.select_database(EndpointSide::Source, "db1");
        state.endpoints.select_database(EndpointSide::Target, "db2");
        assert!(state.can_next(), "two distinct endpoints with databases");
        // Same connection AND database on both sides is rejected
        // (`isSameEndpoint` compares id+database+schema).
        let _ = state.endpoints.select_side(EndpointSide::Target, "c1");
        state.endpoints.select_database(EndpointSide::Target, "db1");
        assert!(!state.can_next());

        // objects: only the loading flag.
        state.step = SchemaDiffWizardStep::Objects;
        assert!(state.can_next());
        state.objects_loading = true;
        assert!(!state.can_next());
        state.objects_loading = false;

        // compare: needs diffs and not loading.
        state.step = SchemaDiffWizardStep::Compare;
        assert!(!state.can_next());
        state.diffs = vec![target_only_diff("t")];
        assert!(state.can_next());
        state.loading = true;
        assert!(!state.can_next());
        state.loading = false;

        // plan: needs a plan and not loading.
        state.step = SchemaDiffWizardStep::Plan;
        assert!(!state.can_next());
        state.plan = Some(plan_default());
        assert!(state.can_next());

        // deploy: no next.
        state.step = SchemaDiffWizardStep::Deploy;
        assert!(!state.can_next());
    }

    #[test]
    fn deploy_allowed_needs_a_live_plan() {
        let mut state = state();
        assert!(!state.deploy_allowed(), "no plan");
        state.plan = Some(plan_with_statements(1));
        assert!(state.deploy_allowed(), "one safe statement deploys");
        state.deploy_result = Some(deploy_result());
        assert!(!state.deploy_allowed(), "a result blocks re-deploy");
        state.deploy_result = None;
        assert!(state.deploy_allowed());
        state.plan = Some(plan_with_statements(0));
        assert!(!state.deploy_allowed(), "statement_count 0 fails canRunDeploy");
    }

    #[test]
    fn rollback_required_needs_a_complete_rollback() {
        let mut state = state();
        // A transactional dialect: `requireRollback` can be satisfied.
        let mut pg = plan_with_statements(2);
        pg.target_dialect = "postgresql".to_owned();
        state.plan = Some(pg);
        state.require_rollback = true;
        assert!(
            !state.deploy_allowed(),
            "rollback incomplete (missing rollbacks) blocks"
        );
        let mut complete = plan_with_statements(2);
        complete.target_dialect = "postgresql".to_owned();
        complete.rollback_completeness = crate::commands::schema_diff::wire::RollbackCompletenessVm {
            complete: true,
            missing: Vec::new(),
        };
        state.plan = Some(complete);
        assert!(state.deploy_allowed());
        // A non-transactional dialect can never satisfy requireRollback
        // (`!requireRollback || (txEnabled && txSupported)`).
        let mut mysql = plan_with_statements(2);
        mysql.target_dialect = "mysql".to_owned();
        mysql.rollback_completeness = crate::commands::schema_diff::wire::RollbackCompletenessVm {
            complete: true,
            missing: Vec::new(),
        };
        state.plan = Some(mysql);
        assert!(!state.deploy_allowed());
    }

    #[test]
    fn the_destructive_confirm_text_rides_only_for_destructive_plans() {
        let mut state = state();
        state.confirm_text = " DEPLOY ".to_owned();
        let safe = plan_with_statements(1);
        assert!(confirm_destructive_text(&safe, &state.confirm_text).is_none());
        let mut destructive = plan_with_statements(1);
        destructive.statements[0].risk = StatementRiskVm::Destructive;
        assert!(crate::commands::schema_diff::plan_has_destructive(&destructive));
        assert_eq!(
            confirm_destructive_text(&destructive, &state.confirm_text).as_deref(),
            Some("DEPLOY"),
            "trimmed"
        );
    }

    #[test]
    fn the_plan_auto_request_arms_once_per_visit() {
        let mut state = state();
        assert!(!state.plan_auto_request_due(), "not on the plan step");
        state.step = SchemaDiffWizardStep::Plan;
        assert!(state.plan_auto_request_due());
        state.arm_plan_auto_request();
        assert!(!state.plan_auto_request_due(), "armed already");
        state.plan = Some(plan_default());
        state.plan_auto_requested = false;
        assert!(!state.plan_auto_request_due(), "a plan answers it");
        state.plan = None;
        state.loading = true;
        assert!(!state.plan_auto_request_due(), "a build is in flight");
        state.loading = false;
        assert!(state.plan_auto_request_due());
        state.step = SchemaDiffWizardStep::Compare;
        state.sync_plan_auto_request();
        assert!(!state.plan_auto_requested, "leaving the step disarms");
    }

    #[test]
    fn the_limitations_dialog_auto_opens_until_dismissed() {
        let mut state = state();
        state.limitations_auto_open(false);
        assert!(state.limitations_open);
        let mut dismissed = SchemaDiffWindowState::default();
        dismissed.limitations_auto_open(true);
        assert!(!dismissed.limitations_open);
    }

    #[test]
    fn the_endpoints_reset_clears_the_eight_and_honours_preservation() {
        let mut state = state();
        state.table_picks = vec![pick("t")];
        state.diffs = vec![target_only_diff("t")];
        state.plan = Some(plan_default());
        state.plan_meta = Some(envelope());
        state.deploy_result = Some(deploy_result());
        state.selected_table = Some("t".to_owned());
        state.type_overrides = vec![ColumnTypeOverrideVm::default()];
        state.plan_auto_requested = true;

        let snapshot = EndpointSnapshot {
            source_id: String::new(),
            target_id: String::new(),
            source_database: String::new(),
            target_database: String::new(),
            source_schema: String::new(),
            target_schema: String::new(),
        };
        assert!(!state.on_endpoints_changed(&snapshot), "nothing pending: reset");
        assert!(state.table_picks.is_empty());
        assert!(state.diffs.is_empty());
        assert!(state.plan.is_none());
        assert!(state.plan_meta.is_none());
        assert!(state.deploy_result.is_none());
        assert!(state.selected_table.is_none());
        assert!(state.type_overrides.is_empty());
        assert!(!state.plan_auto_requested);

        // A pending profile whose endpoints now match preserves once —
        // T411's rule — and consumes the ref.
        let mut preserved = SchemaDiffWindowState::default();
        preserved.saved_setups.pending_profile = Some(SchemaDiffProfileVm {
            source_connection_id: "s".to_owned(),
            target_connection_id: "t".to_owned(),
            source_database: "sdb".to_owned(),
            target_database: "tdb".to_owned(),
            ..profile_default()
        });
        let matching = EndpointSnapshot {
            source_id: "s".to_owned(),
            target_id: "t".to_owned(),
            source_database: "sdb".to_owned(),
            target_database: "tdb".to_owned(),
            source_schema: String::new(),
            target_schema: String::new(),
        };
        preserved.table_picks = vec![pick("keep")];
        assert!(preserved.on_endpoints_changed(&matching), "preserved");
        assert_eq!(preserved.table_picks.len(), 1, "the picks survived");
        assert!(preserved.saved_setups.pending_profile.is_none(), "ref consumed");
        // The next change (ref gone) resets.
        assert!(!preserved.on_endpoints_changed(&matching));
        assert!(preserved.table_picks.is_empty());
    }

    // ---- helpers -------------------------------------------------------------

    fn plan_default() -> SchemaDiffPlanVm {
        SchemaDiffPlanVm {
            target_dialect: "mysql".to_owned(),
            ..Default::default()
        }
    }

    fn plan_with_statements(count: usize) -> SchemaDiffPlanVm {
        SchemaDiffPlanVm {
            target_dialect: "mysql".to_owned(),
            statements: (0..count)
                .map(|_| PlanStatementVm {
                    sql: "SELECT 1".to_owned(),
                    risk: StatementRiskVm::Additive,
                    rollback_sql: None,
                    summary: String::new(),
                    requires_transaction: None,
                })
                .collect(),
            ..Default::default()
        }
    }

    fn plan_with_transaction_statement(required: bool) -> SchemaDiffPlanVm {
        SchemaDiffPlanVm {
            target_dialect: "mysql".to_owned(),
            statements: vec![PlanStatementVm {
                sql: "ALTER".to_owned(),
                risk: StatementRiskVm::Additive,
                rollback_sql: None,
                summary: String::new(),
                requires_transaction: Some(required),
            }],
            ..Default::default()
        }
    }

    fn target_only_diff(table: &str) -> TableSchemaDiffVm {
        TableSchemaDiffVm {
            table: table.to_owned(),
            target_only: true,
            ..Default::default()
        }
    }

    fn pick(name: &str) -> SchemaDiffTablePickVm {
        SchemaDiffTablePickVm {
            name: name.to_owned(),
            enabled: true,
            origin: crate::views::schema_diff::schema_diff_table_names::TablePickOriginVm::Both,
            source_name: None,
            target_name: None,
        }
    }

    fn envelope() -> SchemaDiffPrepareEnvelopeVm {
        SchemaDiffPrepareEnvelopeVm {
            plan: plan_default(),
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

    fn profile_default() -> SchemaDiffProfileVm {
        SchemaDiffProfileVm {
            version: 2,
            id: "p".to_owned(),
            name: "P".to_owned(),
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
            updated_at: String::new(),
        }
    }
}
