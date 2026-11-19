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
//! - `step_result_utils.rs` — the view models plus the minimal slice of
//!   `workflowStepResultUtils.tsx` / `lib/chart/fieldInference.ts` the panel
//!   consumes (the full helper port is T176).
//! - `workflow_form.rs` — `WorkflowForm.tsx` (the visual workflow editor
//!   form: the draft model, field/variable/step editors, the command-input
//!   schema editor and the two footer variants).
//!
//! The workflows page (`views/dashboards_workflows.rs`) mounts the first
//! two; the form's page mount is `WorkflowPage.tsx` (T147) — the lite
//! editor stays until that port swaps the whole edit pane.

pub(crate) mod editor_action_bar;
pub(crate) mod execution_result_panel;
// T113: the section's two list views wait for their pages — WorkflowPage
// (T147) and the AI panel's workflow tab (T334, segment-3) are the callers;
// drop this allow when the first lands.
#[allow(dead_code)]
pub(crate) mod history_section;
pub(crate) mod step_result_utils;
// T070: the form has no production mount yet — the WorkflowPage port (T147)
// is the first caller; drop this allow when it lands.
#[allow(dead_code)]
pub(crate) mod workflow_form;

pub(crate) use editor_action_bar::*;
pub(crate) use execution_result_panel::*;
#[allow(unused_imports)] // consumers: WorkflowPage (T147), AI panel (T334)
pub(crate) use history_section::*;
pub(crate) use step_result_utils::*;
pub(crate) use workflow_form::*;
