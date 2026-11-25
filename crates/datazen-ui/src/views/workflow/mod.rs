//! Ports of `packages/frontend/src/windows/workflow/*` (T019/T070), one
//! module per React file:
//!
//! - `editor_action_bar.rs` — `WorkflowEditorActionBar.tsx` (the edit
//!   panel's shared bottom Cancel/Save bar).
//! - `execution_result_panel.rs` — `WorkflowExecutionResultPanel.tsx` (the
//!   run-result panel).
//! - `history_section.rs` — `WorkflowHistorySection.tsx` (the two
//!   run-history list views: the page's sidebar list and the inline tab
//!   with its detail branch).
//! - `step_result_utils.rs` — `workflowStepResultUtils.tsx` (the status
//!   icon ladder, the column extraction and the step → statement-result
//!   conversion) plus the run view models and the chartability slice the
//!   panel renders from.
//! - `workflow_form.rs` — `WorkflowForm.tsx` (the visual workflow editor
//!   form: the draft model, field/variable/step editors, the command-input
//!   schema editor and the two footer variants).

//! - `workflow_draft_convert.rs` — `workflowDraftConvert.ts` (T223) — the
//!   definition ↔ draft conversion pair (the edit panel's entry draft, the
//!   save path's definition).
//! - `workflow_panel_guards.rs` — `workflowPanelGuards.ts` (T223) — the panel
//!   guards, `panelExecutionResult` and `panelTabLabel` with its labels bag
//!   and clock reads.
//! - `yaml_lint.rs` — `yamlLint.ts` (T244) — the parameter-reference pairing
//!   pass the yaml editor mounts as its lint.
//! - `workflow_page.rs` — `WorkflowPage.tsx` (T145; the ledger's older
//!   numbering says T147) — the page-level assembly: the toolbar, the
//!   workflows/history sidebar, the panel tab bar, the variables + execute
//!   row, the step detail view and the dialogs' ask state.
//!
//! The workflows page (`views/dashboards_workflows.rs`) routed to the lite
//! editor until this module's page port (T145) swapped the whole section.

pub mod editor_action_bar;
pub mod execution_result_panel;
// T113: the page port (T145) mounts the sidebar list; the AI panel's
// workflow tab (T334, segment-3) is the family's remaining caller.
pub mod history_section;
pub mod step_result_utils;
// T145: the workflow page port — the page-level assembly that mounts the
// form, the history list and the action bar as its production consumer (the
// section swap in `views/workspace.rs` routes here).
pub mod workflow_page;
// T070: the page's edit pane (T145) is the form's production consumer.
// T484: the richer editor draft's parking lot behind the legacy
// WorkflowPage callback.
#[allow(dead_code)]
pub mod draft_bridge;
#[allow(dead_code)]
pub mod workflow_form;
// T223: the definition ↔ draft conversion pair — the page's edit entry and
// save path call it through the full-definition host seam.
pub mod workflow_draft_convert;
// T223: the panel guards file's own surface — the guards, the result read
// and the tab label (the page's tab strip and the enum's method delegate).
pub mod workflow_panel_guards;
// T244: the yaml lint pass — the yaml editor (T200) mounts its gutter over
// these diagnostics.
pub mod yaml_lint;
// T265: the yaml syntax highlighting — the yaml editor (T200) mounts it
// over the document surface.
pub mod yaml_mode;
// T200: the yaml editor's chrome — the document surface with the yaml
// highlighting, the line-number and lint gutters (the page's edit pane is
// its production consumer).
pub mod workflow_yaml_editor;

// The family's re-exported view models (the page's own helpers ride through
// `workflow_page::*`; the page reaches the action bar, the result panel and
// the form through their module paths directly — the lite page's `super::*`
// consumers went with it).
#[allow(unused_imports)] // the family surface; the remaining callers are module-path imports
pub use editor_action_bar::*;
#[allow(unused_imports)] // the family surface; the remaining callers are module-path imports
pub use execution_result_panel::*;
#[allow(unused_imports)] // consumers: the AI panel (T334)
pub use history_section::*;
pub use step_result_utils::*;
// T145: the page's view models and pure helpers — the section arm in
// `views/workspace.rs` calls the page function through this re-export.
pub use workflow_page::*;
pub use workflow_form::*;
