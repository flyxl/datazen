//! The `windows/schema-diff` subtree ports (the deploy panel is its first;
//! the wizard window's step panels land with their own tasks).

pub mod schema_diff_deploy_panel;
// T323: the limitations dialog's key list (schemaDiffLimitationKeys.ts).
pub mod schema_diff_limitation_keys;
// T363: the unified-plan object identity key (schemaDiffObjectIdentity.ts).
pub mod schema_diff_object_identity;
// T351: the discovery run's diagnostics, host seam and table-pick pairing
// (schemaDiffObjectDiscovery.ts; the schemaDiffTableNames.ts merge rides
// along as its seam module until T374 ports the full file).
pub mod schema_diff_object_discovery;
// T149: the wizard's object-selection step (SchemaDiffObjectsStep.tsx).
pub mod schema_diff_objects_step;
// T178: the reviewed plan's content panel (SchemaDiffPlanPanel.tsx; the
// requirements / type-suggestions / rollback-status sections are submodules).
pub mod schema_diff_plan_panel;
// T269: the wizard's table list rail (SchemaDiffTableListPanel.tsx).
pub mod schema_diff_table_list_panel;
// T204: the right review panel (SchemaDiffRightPanel.tsx: tabs + plan /
// deploy bodies + the generate-plan empty state).
pub mod schema_diff_right_panel;
// T229: the two saved-setup dialogs (import config + profile naming).
pub mod schema_diff_saved_setup_dialogs;
// T250: the wizard's two selection steps (endpoints bar + objects
// step over the unified picker seam).
pub mod schema_diff_selection_steps;
// T290: the unified non-table object picker (two side lists grouped by
// kind, the cross-dialect lock, the retry/clear escapes).
pub mod schema_diff_unified_objects_picker;
// T075: the wizard's per-step execution surfaces
// (SchemaDiffExecutionPanels.tsx: compare row, plan step, deploy step).
pub mod schema_diff_execution_panels;
// T391: the wizard's job-tracking hook (useSchemaDiffJobLifecycle.ts:
// the tracked job, latest apply, cancel/verify outcomes, hydration).
pub mod use_schema_diff_job_lifecycle;
// T115: the wizard's job status strip (SchemaDiffJobStatusPanel.tsx).
pub mod schema_diff_job_status_panel;
// T372: the clipboard-feedback hook (useSchemaDiffClipboardFeedback.ts;
// the schemaDiffTableNames.ts full port rides in its own module).
pub mod use_schema_diff_clipboard_feedback;
pub mod schema_diff_table_names;
// T339: the wizard's step rail (SchemaDiffWizardProgress.tsx: the
// step type, the two step lists, the three-tone progress band) and
// the bottom navigation bar (SchemaDiffWindowNavigation.tsx: Back +
// the Deploy/Next branch).
pub mod schema_diff_wizard_progress;
pub mod schema_diff_window_navigation;
// T420: the unified (non-table) object catalogs (the six-kind walks,
// the opt-in selection over identity keys, the lifecycle evidence).
pub mod use_schema_diff_unified_objects;
// T402: the wizard's job-result restore (history stays separate;
// only a locally-submitted job's deploy result is adopted).
pub mod use_schema_diff_job_restore;
// T325: the window's job section (the status panel + the window
// error strip).
pub mod schema_diff_window_job_section;
// T384: the wizard's endpoint state (connections, the two sides, the
// discovery owner gates, the catalog reads, swap/validate).
pub mod use_schema_diff_endpoints;
/// `lib/endpointIdentity.ts` + `lib/migrationWindowPrefill.ts` +
/// data-sync `pickDefaultSchema` + the §13 error-code table, carried
/// for the endpoints port (T384) until their own ledger tasks land.
pub mod schema_diff_endpoint_helpers;
// T411: the saved setups (profile save/load/delete, the v2 config
// import/export, the endpoint-preservation rule).
pub mod use_schema_diff_saved_setups;
// T309: the wizard window itself (the orchestrator over every ported
// child; the state, action and view halves are the schema_diff_window
// submodule).
pub mod schema_diff_window;
