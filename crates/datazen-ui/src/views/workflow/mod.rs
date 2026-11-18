//! Ports of `packages/frontend/src/windows/workflow/*` (T019), one module
//! per React file:
//!
//! - `editor_action_bar.rs` — `WorkflowEditorActionBar.tsx` (the edit
//!   panel's shared bottom Cancel/Save bar).
//! - `execution_result_panel.rs` — `WorkflowExecutionResultPanel.tsx` (the
//!   run-result panel).
//! - `step_result_utils.rs` — the view models plus the minimal slice of
//!   `workflowStepResultUtils.tsx` / `lib/chart/fieldInference.ts` the panel
//!   consumes (the full helper port is T176).
//!
//! The workflows page (`views/dashboards_workflows.rs`) mounts both
//! components; `WorkflowPage.tsx` itself is a separate, later task.

pub(crate) mod editor_action_bar;
pub(crate) mod execution_result_panel;
pub(crate) mod step_result_utils;

pub(crate) use editor_action_bar::*;
pub(crate) use execution_result_panel::*;
pub(crate) use step_result_utils::*;
