use super::store::{delete_dashboard, get_dashboard, list_dashboards, load_monitor_settings, save_dashboard};
use super::types::{
    AggregationType, ChartConfig, ChartSortBy, ChartType, Dashboard, DashboardLayout, DashboardWidget,
    WidgetLayout,
};
use crate::store::AppSettings;

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
            config_id: "conn-1".into(),
            sql: "SELECT 1 AS v".into(),
            chart_config: ChartConfig {
                chart_type: ChartType::Bar,
                x_axis: Some("v".into()),
                y_axes: vec!["v".into()],
                group_by: None,
                aggregation: AggregationType::None,
                sort_by: ChartSortBy::None,
                show_legend: true,
                show_grid: true,
                show_values: false,
                color_scheme: "default".into(),
            },
            layout: WidgetLayout {
                x: 0,
                y: 0,
                w: 6,
                h: 4,
            },
            refresh_sec: 60,
            alert: None,
            enabled: true,
        }],
        enabled: true,
    }
}

#[tokio::test]
async fn save_and_list_dashboard_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let dash = sample_dashboard();
    save_dashboard(dir.path(), dash.clone()).unwrap();
    let list = list_dashboards(dir.path()).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, dash.id);
    assert_eq!(list[0].widgets[0].sql, "SELECT 1 AS v");
}

#[test]
fn list_dashboards_creates_empty_file_when_missing() {
    let dir = tempfile::tempdir().unwrap();
    let list = list_dashboards(dir.path()).unwrap();
    assert!(list.is_empty());
    assert!(dir.path().join("dashboards.json").is_file());
    let content = std::fs::read_to_string(dir.path().join("dashboards.json")).unwrap();
    assert_eq!(content.trim(), "[]");
}

#[test]
fn get_dashboard_returns_one_by_id() {
    let dir = tempfile::tempdir().unwrap();
    let dash = sample_dashboard();
    save_dashboard(dir.path(), dash.clone()).unwrap();
    let loaded = get_dashboard(dir.path(), "dash-1").unwrap();
    assert_eq!(loaded.id, dash.id);
}

#[test]
fn delete_dashboard_removes_entry() {
    let dir = tempfile::tempdir().unwrap();
    let dash = sample_dashboard();
    save_dashboard(dir.path(), dash).unwrap();
    delete_dashboard(dir.path(), "dash-1").unwrap();
    assert!(list_dashboards(dir.path()).unwrap().is_empty());
}

#[test]
fn save_dashboard_clamps_refresh_sec() {
    let dir = tempfile::tempdir().unwrap();
    let mut dash = sample_dashboard();
    dash.widgets[0].refresh_sec = 5;
    save_dashboard(dir.path(), dash).unwrap();
    let loaded = get_dashboard(dir.path(), "dash-1").unwrap();
    assert_eq!(loaded.widgets[0].refresh_sec, 30);
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

#[test]
fn load_monitor_settings_defaults_when_monitor_missing() {
    let json = r#"{"theme":"dark","language":"en","limitSelectResults":false,"queryResultLimit":5000,"editorFontSize":13,"editorFontFamily":"JetBrains Mono","confirmOnDelete":true,"autoCommit":true,"defaultPageSize":50,"logLevel":"info","logPath":"","mcpServerEnabled":false,"mcpDisabledTools":[],"mcpPermissionMode":"safe_write","contextDir":"","checkForUpdatesOnStartup":false}"#;
    let settings: AppSettings = serde_json::from_str(json).unwrap();
    let monitor = load_monitor_settings(&settings);
    assert_eq!(monitor.max_concurrent_queries, 2);
}
