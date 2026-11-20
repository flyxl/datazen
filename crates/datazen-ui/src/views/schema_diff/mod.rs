//! The `windows/schema-diff` subtree ports (the deploy panel is its first;
//! the wizard window's step panels land with their own tasks).

pub(crate) mod schema_diff_deploy_panel;
// T323: the limitations dialog's key list (schemaDiffLimitationKeys.ts).
pub(crate) mod schema_diff_limitation_keys;
// T363: the unified-plan object identity key (schemaDiffObjectIdentity.ts).
pub(crate) mod schema_diff_object_identity;
// T351: the discovery run's diagnostics, host seam and table-pick pairing
// (schemaDiffObjectDiscovery.ts; the schemaDiffTableNames.ts merge rides
// along as its seam module until T374 ports the full file).
pub(crate) mod schema_diff_object_discovery;
// T149: the wizard's object-selection step (SchemaDiffObjectsStep.tsx).
pub(crate) mod schema_diff_objects_step;
// T178: the reviewed plan's content panel (SchemaDiffPlanPanel.tsx; the
// requirements / type-suggestions / rollback-status sections are submodules).
pub(crate) mod schema_diff_plan_panel;
// T269: the wizard's table list rail (SchemaDiffTableListPanel.tsx).
pub(crate) mod schema_diff_table_list_panel;
// T204: the right review panel (SchemaDiffRightPanel.tsx: tabs + plan /
// deploy bodies + the generate-plan empty state).
pub(crate) mod schema_diff_right_panel;
// T229: the two saved-setup dialogs (import config + profile naming).
pub(crate) mod schema_diff_saved_setup_dialogs;
// T250: the wizard's two selection steps (endpoints bar + objects
// step over the unified picker seam).
pub(crate) mod schema_diff_selection_steps;
// T290: the unified non-table object picker (two side lists grouped by
// kind, the cross-dialect lock, the retry/clear escapes).
pub(crate) mod schema_diff_unified_objects_picker;
// T075: the wizard's per-step execution surfaces
// (SchemaDiffExecutionPanels.tsx: compare row, plan step, deploy step).
pub(crate) mod schema_diff_execution_panels;
// T391: the wizard's job-tracking hook (useSchemaDiffJobLifecycle.ts:
// the tracked job, latest apply, cancel/verify outcomes, hydration).
pub(crate) mod use_schema_diff_job_lifecycle;
// T115: the wizard's job status strip (SchemaDiffJobStatusPanel.tsx).
pub(crate) mod schema_diff_job_status_panel;
// T372: the clipboard-feedback hook (useSchemaDiffClipboardFeedback.ts;
// the schemaDiffTableNames.ts full port rides in its own module).
pub(crate) mod use_schema_diff_clipboard_feedback;
pub(crate) mod schema_diff_table_names;
// T339: the wizard's step rail (SchemaDiffWizardProgress.tsx: the
// step type, the two step lists, the three-tone progress band) and
// the bottom navigation bar (SchemaDiffWindowNavigation.tsx: Back +
// the Deploy/Next branch).
pub(crate) mod schema_diff_wizard_progress;
pub(crate) mod schema_diff_window_navigation;
// T420: the unified (non-table) object catalogs (the six-kind walks,
// the opt-in selection over identity keys, the lifecycle evidence).
pub(crate) mod use_schema_diff_unified_objects;
