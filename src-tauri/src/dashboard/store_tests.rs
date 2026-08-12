use super::store::{
    delete_dashboard, get_dashboard, list_dashboards, load_monitor_settings, save_dashboard,
    set_dashboard_refresh_paused,
};
use super::types::{
    ChartConfig, ChartType, Dashboard, DashboardLayout, DashboardWidget, RefreshMode,
    RefreshPolicy, ViewMode, WidgetLayout,
};
use crate::store::AppSettings;
use crate::store::{AppDb, WorkflowRecord, WorkflowVisibility};
use chrono::Utc;

fn seed_workflow(db: &AppDb, id: &str) {
    db.upsert_workflow(&WorkflowRecord {
        id: id.into(),
        name: "WF".into(),
        description: String::new(),
        visibility: WorkflowVisibility::User,
        definition_yaml: format!("id: {id}\nname: WF\nsteps: []\n"),
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    })
    .unwrap();
}

fn sample_dashboard() -> Dashboard {
    Dashboard {
        id: "dash-1".into(),
        name: "Test Dashboard".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
        layout: DashboardLayout {
            cols: 12,
            row_height: 80,
        },
        widgets: vec![DashboardWidget {
            id: "w1".into(),
            title: "Widget 1".into(),
            workflow_id: "wf-1".into(),
            view_mode: ViewMode::Chart,
            chart_config: Some(ChartConfig {
                chart_type: ChartType::Bar,
                x_axis: Some("v".into()),
                y_axes: vec!["v".into()],
                group_by: None,
                aggregation: super::types::AggregationType::None,
                sort_by: super::types::ChartSortBy::None,
                show_legend: true,
                show_grid: true,
                show_values: false,
                color_scheme: "default".into(),
            }),
            layout: WidgetLayout {
                x: 0,
                y: 0,
                w: 6,
                h: 4,
            },
            refresh: RefreshPolicy {
                mode: RefreshMode::Interval,
                refresh_sec: Some(60),
            },
            alert: None,
            enabled: true,
        }],
        enabled: true,
        refresh_paused: false,
    }
}

fn open_db() -> std::sync::Arc<AppDb> {
    let db = AppDb::open_in_memory().unwrap();
    seed_workflow(&db, "wf-1");
    db
}

#[test]
fn save_and_list_dashboard_roundtrip() {
    let db = open_db();
    let dash = sample_dashboard();
    save_dashboard(&db, dash.clone()).unwrap();
    let list = list_dashboards(&db).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, dash.id);
    assert_eq!(list[0].widgets[0].workflow_id, "wf-1");
}

#[test]
fn list_dashboards_empty_when_no_rows() {
    let db = AppDb::open_in_memory().unwrap();
    let list = list_dashboards(&db).unwrap();
    assert!(list.is_empty());
}

#[test]
fn get_dashboard_returns_one_by_id() {
    let db = open_db();
    let dash = sample_dashboard();
    save_dashboard(&db, dash.clone()).unwrap();
    let loaded = get_dashboard(&db, "dash-1").unwrap();
    assert_eq!(loaded.id, dash.id);
}

#[test]
fn delete_dashboard_removes_entry() {
    let db = open_db();
    let dash = sample_dashboard();
    save_dashboard(&db, dash).unwrap();
    delete_dashboard(&db, "dash-1").unwrap();
    assert!(list_dashboards(&db).unwrap().is_empty());
}

#[test]
fn save_dashboard_clamps_interval_refresh_sec() {
    let db = open_db();
    let mut dash = sample_dashboard();
    dash.widgets[0].refresh = RefreshPolicy {
        mode: RefreshMode::Interval,
        refresh_sec: Some(5),
    };
    save_dashboard(&db, dash).unwrap();
    let loaded = get_dashboard(&db, "dash-1").unwrap();
    assert_eq!(loaded.widgets[0].refresh.refresh_sec, Some(30));
}

#[test]
fn set_dashboard_refresh_paused_persists() {
    let db = open_db();
    let dash = sample_dashboard();
    save_dashboard(&db, dash).unwrap();
    set_dashboard_refresh_paused(&db, "dash-1", true).unwrap();
    let loaded = get_dashboard(&db, "dash-1").unwrap();
    assert!(loaded.refresh_paused);
}

#[test]
fn load_monitor_settings_uses_app_settings_nested_monitor() {
    use super::types::MonitorSettings;

    let settings = AppSettings {
        monitor: MonitorSettings {
            max_concurrent_queries: 4,
            ..MonitorSettings::default()
        },
        ..AppSettings::default()
    };
    let monitor = load_monitor_settings(&settings);
    assert_eq!(monitor.max_concurrent_queries, 4);
}
