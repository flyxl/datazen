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
// T075: the wizard's per-step execution surfaces
// (SchemaDiffExecutionPanels.tsx: compare row, plan step, deploy step).
pub(crate) mod schema_diff_execution_panels;
// T391: the wizard's job-tracking hook (useSchemaDiffJobLifecycle.ts:
// the tracked job, latest apply, cancel/verify outcomes, hydration).
pub(crate) mod use_schema_diff_job_lifecycle;
pub(crate) mod schema_diff_table_names;
