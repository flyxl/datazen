//! Per-mode view-state snapshots for the main-shell workspace panels,
//! ported from React
//! `packages/frontend/src/stores/workspacePanelStateStore.ts` (tasklist T486).
//!
//! The workspace modes (`workflow` / `dashboard` / `workspace` / `extension`)
//! render conditionally — only the active mode keeps its DOM tree mounted.
//! When the user switches away and back, each panel restores its previous
//! view state from here instead of starting over.
//!
//! Only recreatable view state lives here (open tabs, drafts, editor/dialog
//! UI). Server-backed data (workflow list, dashboard boards, wapp list)
//! stays in its own store / reloads on mount. Pure one-shot confirmations
//! and transient error banners are intentionally NOT snapshotted.
//!
//! ## What maps where
//!
//! | React | here |
//! |---|---|
//! | `useWorkspacePanelStateStore` (the hook) | the shell host's [`WorkspacePanelStateStore`] field |
//! | `saveWorkflowSnapshot(s)` / `saveDashboardSnapshot(s)` / `saveExtensionSnapshot(s)` | the three same-name methods |
//! | `clearDashboardSnapshot()` | the method — drops the dashboard slot so an explicit open request shows the requested board |
//! | `clearWorkspacePanelSnapshots()` | the method — drops all three |
//!
//! Type mirrors: `WorkflowPanel` / `WorkflowSideTab` / `WorkflowListItem`
//! are the workflow page's own mirrors (`views/workflow/workflow_page.rs`);
//! `DashboardWidget` is the widget-editor drawer's [`WidgetDraftVm`] — the
//! crate's field-for-field mirror of the display-side widget. `activeStepIndex`
//! is a step position, so it mirrors as `Option<usize>`; `variables` keeps
//! insertion-transparent comparison through `BTreeMap`.

use std::collections::BTreeMap;

use crate::views::dashboard::widget_editor_drawer::WidgetDraftVm;
use crate::views::{WorkflowListItem, WorkflowPanel, WorkflowSideTab};

/// React `WorkflowWorkspaceSnapshot`: the workflow mode's panels and UI
/// positions.
#[derive(Clone, Debug)]
#[allow(dead_code)] // consumers: the workspace shell track (not landed yet)
pub struct WorkflowWorkspaceSnapshot {
    pub panels: Vec<WorkflowPanel>,
    /// The active panel's id (`wf-{n}` / `edit-{n}` / …), or none.
    pub active_panel_id: Option<String>,
    /// The active step of the open workflow, or none.
    pub active_step_index: Option<usize>,
    /// `'workflows' | 'history'`.
    pub side_tab: WorkflowSideTab,
    /// The panel's variable inputs (`Record<string, string>`).
    pub variables: BTreeMap<String, String>,
}

/// React `editorHiddenSql?: { connectionId: string; sql: string }`: the SQL
/// a widget edit started from, kept so the editor round-trips it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // consumers: the workspace shell track (not landed yet)
pub struct EditorHiddenSql {
    pub connection_id: String,
    pub sql: String,
}

/// React `DashboardWorkspaceSnapshot`: the dashboard mode's board, editor,
/// history and renaming positions.
#[derive(Clone, Debug)]
#[allow(dead_code)] // consumers: the workspace shell track (not landed yet)
pub struct DashboardWorkspaceSnapshot {
    pub active_dashboard_id: String,
    pub editor_open: bool,
    /// The widget being edited, or none.
    pub editing_widget: Option<WidgetDraftVm>,
    /// Whether the editor is creating a new widget (vs. editing one).
    pub is_new_widget: bool,
    /// The hidden-SQL seed of the editor round-trip.
    pub editor_hidden_sql: Option<EditorHiddenSql>,
    /// The workflows the widget pickers may choose from.
    pub user_workflows: Vec<WorkflowListItem>,
    pub history_open: bool,
    /// The widget the history drawer shows, or none.
    pub history_widget: Option<WidgetDraftVm>,
    /// Whether the board rename input is showing.
    pub renaming: bool,
    /// The rename input's draft.
    pub name_draft: String,
}

/// React `ExtensionFilter = 'all' | 'workspace' | 'theme'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // consumers: the workspace shell track (not landed yet)
pub enum ExtensionFilter {
    All,
    Workspace,
    Theme,
}

/// React `ExtensionWorkspaceSnapshot`: the extension mode's search and
/// filter.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // consumers: the workspace shell track (not landed yet)
pub struct ExtensionWorkspaceSnapshot {
    pub search: String,
    pub filter: ExtensionFilter,
}

/// The store (`useWorkspacePanelStateStore`'s state): one optional snapshot
/// per mode.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)] // consumers: the workspace shell track (not landed yet)
pub struct WorkspacePanelStateStore {
    pub workflow: Option<WorkflowWorkspaceSnapshot>,
    pub dashboard: Option<DashboardWorkspaceSnapshot>,
    pub extension: Option<ExtensionWorkspaceSnapshot>,
}

#[allow(dead_code)] // consumers: the workspace shell track (not landed yet)
impl WorkspacePanelStateStore {
    /// `saveWorkflowSnapshot(snapshot)`.
    pub fn save_workflow_snapshot(&mut self, snapshot: WorkflowWorkspaceSnapshot) {
        self.workflow = Some(snapshot);
    }

    /// `saveDashboardSnapshot(snapshot)`.
    pub fn save_dashboard_snapshot(&mut self, snapshot: DashboardWorkspaceSnapshot) {
        self.dashboard = Some(snapshot);
    }

    /// `saveExtensionSnapshot(snapshot)`.
    pub fn save_extension_snapshot(&mut self, snapshot: ExtensionWorkspaceSnapshot) {
        self.extension = Some(snapshot);
    }

    /// `clearDashboardSnapshot()`: drops the dashboard snapshot so an
    /// explicit open request shows the requested board. The other modes
    /// keep theirs.
    pub fn clear_dashboard_snapshot(&mut self) {
        self.dashboard = None;
    }

    /// `clearWorkspacePanelSnapshots()`: drops all three.
    pub fn clear_workspace_panel_snapshots(&mut self) {
        self.workflow = None;
        self.dashboard = None;
        self.extension = None;
    }
}
