//! The desktop store answers the shared dashboard store port.

use super::store::load_monitor_settings;
use crate::store::AppSettings;
use crate::store::{AppDb, WorkflowRecord, WorkflowVisibility};
use chrono::Utc;
use datazen_feature_core::dashboard::store::{
    delete_dashboard, get_dashboard, list_dashboards, save_dashboard, set_dashboard_refresh_paused,
};
use datazen_feature_core::dashboard::types::{
    AggregationType, ChartConfig, ChartSortBy, ChartType, Dashboard, DashboardLayout,
    DashboardWidget, RefreshMode, RefreshPolicy, ViewMode, WidgetLayout,
};
use datazen_platform_api::id::OrganizationId;
use datazen_platform_api::ports::scope::OrganizationScope;

/// 桌面自己的组织作用域。
fn local_scope() -> OrganizationScope {
    crate::platform::local_organization_scope()
}

/// 别的组织的作用域。
fn foreign_scope() -> OrganizationScope {
    OrganizationScope::new(OrganizationId::new("another-organization"))
}

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
                aggregation: AggregationType::None,
                sort_by: ChartSortBy::None,
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
    let scope = local_scope();
    let dash = sample_dashboard();
    save_dashboard(&scope, &*db, dash.clone()).unwrap();
    let list = list_dashboards(&scope, &*db).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, dash.id);
    assert_eq!(list[0].widgets[0].workflow_id, "wf-1");
}

#[test]
fn list_dashboards_empty_when_no_rows() {
    let db = AppDb::open_in_memory().unwrap();
    let list = list_dashboards(&local_scope(), &*db).unwrap();
    assert!(list.is_empty());
}

#[test]
fn get_dashboard_returns_one_by_id() {
    let db = open_db();
    let scope = local_scope();
    let dash = sample_dashboard();
    save_dashboard(&scope, &*db, dash.clone()).unwrap();
    let loaded = get_dashboard(&scope, &*db, "dash-1").unwrap();
    assert_eq!(loaded.id, dash.id);
}

#[test]
fn delete_dashboard_removes_entry() {
    let db = open_db();
    let scope = local_scope();
    let dash = sample_dashboard();
    save_dashboard(&scope, &*db, dash).unwrap();
    delete_dashboard(&scope, &*db, "dash-1").unwrap();
    assert!(list_dashboards(&scope, &*db).unwrap().is_empty());
}

#[test]
fn save_dashboard_clamps_interval_refresh_sec() {
    let db = open_db();
    let scope = local_scope();
    let mut dash = sample_dashboard();
    dash.widgets[0].refresh = RefreshPolicy {
        mode: RefreshMode::Interval,
        refresh_sec: Some(5),
    };
    save_dashboard(&scope, &*db, dash).unwrap();
    let loaded = get_dashboard(&scope, &*db, "dash-1").unwrap();
    assert_eq!(loaded.widgets[0].refresh.refresh_sec, Some(30));
}

#[test]
fn set_dashboard_refresh_paused_persists() {
    let db = open_db();
    let scope = local_scope();
    let dash = sample_dashboard();
    save_dashboard(&scope, &*db, dash).unwrap();
    set_dashboard_refresh_paused(&scope, &*db, "dash-1", true).unwrap();
    let loaded = get_dashboard(&scope, &*db, "dash-1").unwrap();
    assert!(loaded.refresh_paused);
}

#[test]
fn load_monitor_settings_uses_app_settings_nested_monitor() {
    use datazen_feature_core::dashboard::types::MonitorSettings;

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

// ─── 组织隔离（宿主的存储隔离） ───

/// 别的组织读不到本组织的仪表板（CM-05：按「不存在」处理，不是「无权限」）。
#[test]
fn a_dashboard_of_another_organization_is_not_found() {
    let db = open_db();
    let scope = local_scope();
    save_dashboard(&scope, &*db, sample_dashboard()).unwrap();

    assert!(list_dashboards(&foreign_scope(), &*db).unwrap().is_empty());
    assert!(matches!(
        get_dashboard(&foreign_scope(), &*db, "dash-1"),
        Err(datazen_feature_core::dashboard::store::DashboardStoreError::NotFound(_))
    ));
}

/// 别的组织读不到本组织的 widget（CM-05）。
#[test]
fn the_widgets_of_another_organization_are_not_found() {
    let db = open_db();
    let scope = local_scope();
    save_dashboard(&scope, &*db, sample_dashboard()).unwrap();

    assert!(
        datazen_feature_core::dashboard::store::DashboardStore::list_widget_rows(
            &*db,
            &foreign_scope(),
            "dash-1"
        )
        .unwrap()
        .is_empty()
    );
    assert!(matches!(
        datazen_feature_core::dashboard::store::DashboardStore::widget_row(
            &*db,
            &foreign_scope(),
            "w1"
        ),
        Err(datazen_feature_core::dashboard::store::DashboardStoreError::NotFound(_))
    ));
}

/// 别的组织既改不动也删不掉本组织的仪表板（CM-05）。
#[test]
fn a_dashboard_of_another_organization_cannot_be_changed_or_removed() {
    let db = open_db();
    let scope = local_scope();
    save_dashboard(&scope, &*db, sample_dashboard()).unwrap();
    let foreign = foreign_scope();

    assert!(matches!(
        set_dashboard_refresh_paused(&foreign, &*db, "dash-1", true),
        Err(datazen_feature_core::dashboard::store::DashboardStoreError::NotFound(_))
    ));
    assert!(matches!(
        delete_dashboard(&foreign, &*db, "dash-1"),
        Err(datazen_feature_core::dashboard::store::DashboardStoreError::NotFound(_))
    ));
    assert!(
        get_dashboard(&scope, &*db, "dash-1").is_ok(),
        "别的组织删除不得影响本组织的仪表板"
    );
}

/// 别的组织读不到本组织的工作流引用（CM-05）。
#[test]
fn the_workflow_refs_of_another_organization_are_not_found() {
    let db = open_db();
    let scope = local_scope();
    save_dashboard(&scope, &*db, sample_dashboard()).unwrap();

    let local =
        datazen_feature_core::dashboard::store::find_workflow_refs(&scope, &*db, "wf-1").unwrap();
    assert_eq!(local.len(), 1);

    let foreign =
        datazen_feature_core::dashboard::store::find_workflow_refs(&foreign_scope(), &*db, "wf-1")
            .unwrap();
    assert!(foreign.is_empty());
}
