//! T486 audit (tester): independent checks of the per-mode view-state
//! snapshot store, from separate snapshot construction sites: interleaved
//! saves and clears, a snapshot replaced by a different one mid-session,
//! and the clear scope boundaries.

use std::collections::BTreeMap;

use crate::views::dashboard::chart_widget_tile::TileViewMode;
use crate::views::dashboard::dashboard_panel::{RefreshModeVm, RefreshPolicyVm, WidgetLayoutVm};
use crate::views::dashboard::widget_editor_drawer::WidgetDraftVm;
use crate::views::{WorkflowPanel, WorkflowSideTab};
use crate::workspace_panel_state_store::{
    DashboardWorkspaceSnapshot, EditorHiddenSql, ExtensionFilter,
    ExtensionWorkspaceSnapshot, WorkflowWorkspaceSnapshot, WorkspacePanelStateStore,
};

fn audit_workflow(panels: usize, active: Option<&str>) -> WorkflowWorkspaceSnapshot {
    WorkflowWorkspaceSnapshot {
        panels: (1..=panels)
            .map(|n| WorkflowPanel::AiCreate { id: format!("ai-{n}") })
            .collect(),
        active_panel_id: active.map(str::to_string),
        active_step_index: if panels == 0 { None } else { Some(panels) },
        side_tab: WorkflowSideTab::Workflows,
        variables: BTreeMap::new(),
    }
}

fn audit_widget(id: &str) -> WidgetDraftVm {
    WidgetDraftVm {
        id: id.to_string(),
        title: format!("Widget {id}"),
        workflow_id: "wf".to_string(),
        view_mode: TileViewMode::Table,
        chart_config: None,
        layout: WidgetLayoutVm { x: 1, y: 2, w: 3, h: 4 },
        refresh: RefreshPolicyVm { mode: RefreshModeVm::Manual, refresh_sec: None },
        enabled: false,
        alert: None,
    }
}

fn audit_dashboard(id: &str) -> DashboardWorkspaceSnapshot {
    DashboardWorkspaceSnapshot {
        active_dashboard_id: id.to_string(),
        editor_open: false,
        editing_widget: None,
        is_new_widget: true,
        editor_hidden_sql: None,
        user_workflows: Vec::new(),
        history_open: true,
        history_widget: Some(audit_widget(id)),
        renaming: false,
        name_draft: String::new(),
    }
}

#[test]
fn audit_an_interleaved_mode_switching_session() {
    // The user walks through all four modes, snapshotting on the way out.
    let mut store = WorkspacePanelStateStore::default();

    store.save_workflow_snapshot(audit_workflow(1, Some("ai-1")));
    store.save_dashboard_snapshot(audit_dashboard("d1"));
    store.save_extension_snapshot(ExtensionWorkspaceSnapshot {
        search: "sql".to_string(),
        filter: ExtensionFilter::Workspace,
    });

    // Back to workflow: the snapshot survived the dashboard/extension
    // saves; a NEW snapshot replaces it wholesale.
    assert_eq!(store.workflow.as_ref().unwrap().panels.len(), 1);
    store.save_workflow_snapshot(audit_workflow(2, Some("ai-2")));
    let workflow = store.workflow.as_ref().unwrap();
    assert_eq!(workflow.panels.len(), 2);
    assert_eq!(workflow.active_panel_id.as_deref(), Some("ai-2"));
    assert_eq!(workflow.active_step_index, Some(2));

    // The other two modes are untouched by the workflow re-save.
    assert_eq!(store.dashboard.as_ref().unwrap().active_dashboard_id, "d1");
    assert_eq!(store.extension.as_ref().unwrap().search, "sql");
}

#[test]
fn audit_clear_dashboard_is_scoped_and_repeatable() {
    let mut store = WorkspacePanelStateStore::default();
    store.save_dashboard_snapshot(audit_dashboard("d1"));
    store.clear_dashboard_snapshot();
    assert!(store.dashboard.is_none());
    // A second clear with nothing stored stays a quiet no-op.
    store.clear_dashboard_snapshot();
    assert!(store.dashboard.is_none());
    // And saving again after a clear works.
    store.save_dashboard_snapshot(audit_dashboard("d2"));
    assert_eq!(store.dashboard.as_ref().unwrap().active_dashboard_id, "d2");
}

#[test]
fn audit_the_hidden_sql_round_trips_with_the_widget() {
    let mut store = WorkspacePanelStateStore::default();
    let mut snapshot = audit_dashboard("d9");
    snapshot.editing_widget = Some(audit_widget("w9"));
    snapshot.editor_hidden_sql = Some(EditorHiddenSql {
        connection_id: "conn-9".to_string(),
        sql: "SELECT 9".to_string(),
    });
    store.save_dashboard_snapshot(snapshot);
    let restored = store.dashboard.as_ref().unwrap();
    assert_eq!(restored.editing_widget.as_ref().unwrap().id, "w9");
    assert_eq!(restored.editor_hidden_sql.as_ref().unwrap().connection_id, "conn-9");
    assert_eq!(restored.editor_hidden_sql.as_ref().unwrap().sql, "SELECT 9");
    assert_eq!(restored.history_widget.as_ref().unwrap().title, "Widget d9");
}

#[test]
fn audit_a_full_clear_then_a_fresh_session() {
    let mut store = WorkspacePanelStateStore::default();
    store.save_workflow_snapshot(audit_workflow(1, None));
    store.save_dashboard_snapshot(audit_dashboard("d1"));
    store.save_extension_snapshot(ExtensionWorkspaceSnapshot {
        search: "theme".to_string(),
        filter: ExtensionFilter::Theme,
    });
    store.clear_workspace_panel_snapshots();
    // A fresh session's first snapshots are the only state left.
    store.save_workflow_snapshot(audit_workflow(0, None));
    let workflow = store.workflow.as_ref().unwrap();
    assert_eq!(workflow.panels.len(), 0);
    assert_eq!(workflow.active_step_index, None);
    assert_eq!(workflow.side_tab, WorkflowSideTab::Workflows);
    assert!(store.dashboard.is_none());
    assert!(store.extension.is_none());
}
