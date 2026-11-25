//! T486: the per-mode view-state snapshots — save/restore per mode, the
//! dashboard-only clear, and the full clear.

use std::collections::BTreeMap;

use datazen_ui::views::dashboard::chart_widget_tile::TileViewMode;
use datazen_ui::views::dashboard::dashboard_panel::{RefreshModeVm, RefreshPolicyVm, WidgetLayoutVm};
use datazen_ui::views::dashboard::widget_editor_drawer::WidgetDraftVm;
use datazen_ui::views::{WorkflowPanel, WorkflowSideTab, WorkflowListItem};
use datazen_ui::workspace_panel_state_store::{
    DashboardWorkspaceSnapshot, EditorHiddenSql, ExtensionFilter,
    ExtensionWorkspaceSnapshot, WorkflowWorkspaceSnapshot, WorkspacePanelStateStore,
};

fn workflow_snapshot(active_panel: Option<&str>) -> WorkflowWorkspaceSnapshot {
    let mut variables = BTreeMap::new();
    variables.insert("env".to_string(), "staging".to_string());
    WorkflowWorkspaceSnapshot {
        panels: vec![
            WorkflowPanel::AiCreate { id: "ai-1".to_string() },
            WorkflowPanel::AiCreate { id: "ai-2".to_string() },
        ],
        active_panel_id: active_panel.map(str::to_string),
        active_step_index: Some(3),
        side_tab: WorkflowSideTab::History,
        variables,
    }
}

fn widget(title: &str) -> WidgetDraftVm {
    WidgetDraftVm {
        id: "w1".to_string(),
        title: title.to_string(),
        workflow_id: "wf-1".to_string(),
        view_mode: TileViewMode::Chart,
        chart_config: None,
        layout: WidgetLayoutVm { x: 0, y: 0, w: 6, h: 4 },
        refresh: RefreshPolicyVm { mode: RefreshModeVm::Manual, refresh_sec: None },
        enabled: true,
        alert: None,
    }
}

fn dashboard_snapshot() -> DashboardWorkspaceSnapshot {
    DashboardWorkspaceSnapshot {
        active_dashboard_id: "dash-7".to_string(),
        editor_open: true,
        editing_widget: Some(widget("Editing")),
        is_new_widget: false,
        editor_hidden_sql: Some(EditorHiddenSql {
            connection_id: "c1".to_string(),
            sql: "SELECT 1".to_string(),
        }),
        user_workflows: vec![WorkflowListItem {
            id: "wf-1".to_string(),
            name: "Nightly ETL".to_string(),
            description: None,
            variables: Vec::new(),
            scheduled: false,
        }],
        history_open: false,
        history_widget: None,
        renaming: true,
        name_draft: "Boards 2".to_string(),
    }
}

#[test]
fn a_fresh_store_has_no_snapshots() {
    let store = WorkspacePanelStateStore::default();
    assert!(store.workflow.is_none());
    assert!(store.dashboard.is_none());
    assert!(store.extension.is_none());
}

#[test]
fn workflow_snapshots_round_trip() {
    let mut store = WorkspacePanelStateStore::default();
    store.save_workflow_snapshot(workflow_snapshot(Some("ai-2")));
    let restored = store.workflow.expect("the snapshot is stored");
    assert_eq!(restored.active_panel_id.as_deref(), Some("ai-2"));
    assert_eq!(restored.active_step_index, Some(3));
    assert_eq!(restored.side_tab, WorkflowSideTab::History);
    assert_eq!(restored.variables.get("env").map(String::as_str), Some("staging"));
    assert_eq!(restored.panels.len(), 2);
}

#[test]
fn dashboard_snapshots_round_trip_every_field() {
    let mut store = WorkspacePanelStateStore::default();
    store.save_dashboard_snapshot(dashboard_snapshot());
    let restored = store.dashboard.expect("the snapshot is stored");
    assert_eq!(restored.active_dashboard_id, "dash-7");
    assert!(restored.editor_open);
    assert_eq!(restored.editing_widget.as_ref().map(|w| w.title.as_str()), Some("Editing"));
    assert!(!restored.is_new_widget);
    assert_eq!(
        restored.editor_hidden_sql.as_ref().map(|sql| sql.sql.as_str()),
        Some("SELECT 1")
    );
    assert_eq!(restored.user_workflows.len(), 1);
    assert_eq!(restored.user_workflows[0].name, "Nightly ETL");
    assert!(!restored.history_open);
    assert!(restored.history_widget.is_none());
    assert!(restored.renaming);
    assert_eq!(restored.name_draft, "Boards 2");
}

#[test]
fn extension_snapshots_round_trip() {
    let mut store = WorkspacePanelStateStore::default();
    store.save_extension_snapshot(ExtensionWorkspaceSnapshot {
        search: "theme".to_string(),
        filter: ExtensionFilter::Theme,
    });
    let restored = store.extension.expect("the snapshot is stored");
    assert_eq!(restored.search, "theme");
    assert_eq!(restored.filter, ExtensionFilter::Theme);
}

#[test]
fn each_mode_saves_and_restores_independently() {
    let mut store = WorkspacePanelStateStore::default();
    store.save_workflow_snapshot(workflow_snapshot(Some("ai-1")));
    store.save_extension_snapshot(ExtensionWorkspaceSnapshot {
        search: String::new(),
        filter: ExtensionFilter::All,
    });
    store.save_dashboard_snapshot(dashboard_snapshot());

    // Re-saving the workflow slot replaces it, not the others.
    store.save_workflow_snapshot(workflow_snapshot(None));
    assert_eq!(store.workflow.unwrap().active_panel_id, None);
    assert!(store.dashboard.is_some());
    assert!(store.extension.is_some());
}

#[test]
fn clear_dashboard_snapshot_drops_only_the_dashboard() {
    let mut store = WorkspacePanelStateStore::default();
    store.save_workflow_snapshot(workflow_snapshot(None));
    store.save_dashboard_snapshot(dashboard_snapshot());
    store.save_extension_snapshot(ExtensionWorkspaceSnapshot {
        search: "q".to_string(),
        filter: ExtensionFilter::Workspace,
    });
    store.clear_dashboard_snapshot();
    assert!(store.dashboard.is_none(), "an explicit open shows the requested board");
    assert!(store.workflow.is_some());
    assert!(store.extension.is_some());
}

#[test]
fn clear_workspace_panel_snapshots_drops_all_three() {
    let mut store = WorkspacePanelStateStore::default();
    store.save_workflow_snapshot(workflow_snapshot(None));
    store.save_dashboard_snapshot(dashboard_snapshot());
    store.save_extension_snapshot(ExtensionWorkspaceSnapshot {
        search: "q".to_string(),
        filter: ExtensionFilter::All,
    });
    store.clear_workspace_panel_snapshots();
    assert!(store.workflow.is_none());
    assert!(store.dashboard.is_none());
    assert!(store.extension.is_none());
}
