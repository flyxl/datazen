//! Port of `packages/frontend/src/windows/workflow/workflowPanelGuards.ts` (T223; the
//! ledger's older numbering says T225).
//!
//! The React file owns the panel view models and their helpers: the
//! `WorkflowPanel` tagged union (`WorkflowRunPanel` / `HistoryDetailPanel` /
//! `EditPanel` / `AiCreatePanel`), the four `is*` type guards the page narrows
//! its active panel with, `panelExecutionResult` (the run/history panels'
//! result a tab badge reads), and `panelTabLabel` (the tab strip's label with
//! its labels bag and its clock reads).
//!
//! The Rust side already carries the union and the guards' answers in
//! `views/workflow/workflow_page.rs` (T145's port — the enum and its
//! `matches!` sites); this module is the file's own home: the guards as
//! functions (the parity surface), `panel_execution_result` as the one
//! definition (the enum method delegates), and `panel_tab_label` with the
//! labels bag the React signature declares (the page's state-coupled caller
//! builds the bag from `t()` and passes the edit draft's name — the draft
//! lives in the app-level slot T070's form contract set, not in the panel).
//!
//! ## Deliberate deviations
//!
//! * The type guards are Rust-redundant (the enum's `matches!` sites already
//!   narrow); they land as functions anyway — the file's own surface, and the
//!   tests' parity pins.
//! * `new Date(x).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit'
//! })` — the `locale_string` stance the page's clock already took
//! (`support::formatters::parse_zoned_instant` → local-zone `HH:MM`, anything
//! unparseable verbatim); the clock moved here with the label it serves.
//! * The edit panel's draft name — React reads `panel.draft.name`; the Rust
//!   edit panel keeps its draft in the app-level slot (T070's contract), so
//!   the name arrives as the `edit_draft_name` parameter the page passes.

use crate::support::formatters::parse_zoned_instant;
use crate::views::workflow::step_result_utils::WorkflowExecutionResult;
use crate::views::workflow::workflow_page::WorkflowPanel;

// ---- The labels bag (React's `labels` parameter) ------------------------------

/// React's `labels: { aiCreate, edit, create }` — the tab strip's fixed
/// spellings, injected the way the page passes its `t()` reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanelTabLabels {
    pub ai_create: String,
    pub edit: String,
    pub create: String,
}

// ---- The type guards (React 36-50) -----------------------------------------------

/// React's `isWorkflowRunPanel`.
pub fn is_workflow_run_panel(panel: &WorkflowPanel) -> bool {
    matches!(panel, WorkflowPanel::Run(_))
}

/// React's `isHistoryDetailPanel`.
pub fn is_history_detail_panel(panel: &WorkflowPanel) -> bool {
    matches!(panel, WorkflowPanel::History(_))
}

/// React's `isEditPanel`.
pub fn is_edit_panel(panel: &WorkflowPanel) -> bool {
    matches!(panel, WorkflowPanel::Edit(_))
}

/// React's `isAiCreatePanel`.
pub fn is_ai_create_panel(panel: &WorkflowPanel) -> bool {
    matches!(panel, WorkflowPanel::AiCreate { .. })
}

// ---- panelExecutionResult (React 52-58) ------------------------------------------

/// React's `panelExecutionResult` — the run panel's answer (null while the
/// run is in flight), the history record's result; edit and ai-create panels
/// carry none.
pub fn panel_execution_result(panel: &WorkflowPanel) -> Option<&WorkflowExecutionResult> {
    match panel {
        WorkflowPanel::Run(run) => run.result.as_ref(),
        WorkflowPanel::History(history) => Some(&history.result),
        WorkflowPanel::Edit(_) | WorkflowPanel::AiCreate { .. } => None,
    }
}

// ---- panelTabLabel (React 60-75) ---------------------------------------------------

/// The `HH:MM` the run/history tab labels read (React
/// `new Date(...).toLocaleTimeString([], {hour: '2-digit', minute: '2-digit'})`
/// — the `locale_string` stance: zoned input in the local zone; anything
/// unparseable stays verbatim).
pub fn tab_clock(iso: &str) -> String {
    match parse_zoned_instant(iso).and_then(chrono::DateTime::from_timestamp_millis) {
        Some(instant) => instant
            .with_timezone(&chrono::Local)
            .format("%H:%M")
            .to_string(),
        None => iso.to_owned(),
    }
}

/// React's `panelTabLabel(panel, labels)` — the ai-create label, the edit
/// tab's draft-name read (`panel.draft.name || labels.edit` when editing,
/// `labels.create` otherwise), the run/history names with their clock reads.
/// The edit draft's name arrives from the caller (the app-level draft slot).
///
/// React's ladder (`isAiCreatePanel` → `isEditPanel` → `isWorkflowRunPanel` →
/// `isHistoryDetailPanel` → `''`) narrows a disjoint union, so the branch
/// order is type-level — the match arms carry it.
pub fn panel_tab_label(
    panel: &WorkflowPanel,
    labels: &PanelTabLabels,
    edit_draft_name: &str,
) -> String {
    match panel {
        WorkflowPanel::AiCreate { .. } => labels.ai_create.clone(),
        // React 65-66: the editing tab reads the draft's name, falling back
        // to the edit label; the create tab is the create label.
        WorkflowPanel::Edit(edit) => {
            if edit.editing_id.is_some() {
                if edit_draft_name.is_empty() {
                    labels.edit.clone()
                } else {
                    edit_draft_name.to_owned()
                }
            } else {
                labels.create.clone()
            }
        }
        WorkflowPanel::Run(run) => {
            format!("{} {}", run.workflow_name, tab_clock(&run.started_at))
        }
        WorkflowPanel::History(history) => {
            format!("{} {}", history.workflow_name, tab_clock(&history.created_at))
        }
    }
}
