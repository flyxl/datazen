//! `datazen.sqlite`: the dashboard database holding the app's own state
//! (saved workflows, dashboards, widgets, and the rows widgets produced).
//!
//! These stay in one file rather than splitting per domain because the
//! referential rules cross the domains: a widget's foreign key points at a
//! workflow, and deleting a dashboard cascades to its widgets and their runs.
//! A test that saw only one table could not tell a missing row from a delete
//! that was supposed to take it.

use super::*;
use chrono::Utc;

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn sample_workflow(id: &str, visibility: WorkflowVisibility) -> WorkflowRecord {
    let ts = now();
    WorkflowRecord {
        id: id.into(),
        name: format!("WF {id}"),
        description: "desc".into(),
        visibility,
        definition_yaml: format!("id: {id}\nname: test\nsteps: []\n"),
        created_at: ts.clone(),
        updated_at: ts,
    }
}

fn sample_dashboard(id: &str) -> DashboardRecord {
    let ts = now();
    DashboardRecord {
        id: id.into(),
        name: format!("Dash {id}"),
        created_at: ts.clone(),
        updated_at: ts,
        layout_cols: 12,
        layout_row_height: 80,
        enabled: true,
        refresh_paused: false,
    }
}

fn sample_widget(id: &str, dashboard_id: &str, workflow_id: &str) -> WidgetRecord {
    let ts = now();
    WidgetRecord {
        id: id.into(),
        dashboard_id: dashboard_id.into(),
        title: format!("Widget {id}"),
        workflow_id: workflow_id.into(),
        view_mode: "chart".into(),
        chart_config_json: None,
        layout_x: 0,
        layout_y: 0,
        layout_w: 6,
        layout_h: 4,
        refresh_mode: "manual".into(),
        refresh_sec: None,
        alert_json: None,
        enabled: true,
        sort_order: 0,
        created_at: ts.clone(),
        updated_at: ts,
    }
}

#[test]
fn open_creates_schema_version() {
    let db = AppDb::open_in_memory().unwrap();
    let version: i32 = db
        .with_conn(|conn| {
            Ok(
                conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                    row.get(0)
                })?,
            )
        })
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
}

#[test]
fn workflow_crud_and_visibility_filter() {
    let db = AppDb::open_in_memory().unwrap();
    db.upsert_workflow(&sample_workflow("u1", WorkflowVisibility::User))
        .unwrap();
    db.upsert_workflow(&sample_workflow("h1", WorkflowVisibility::DashboardHidden))
        .unwrap();

    let users = db.list_workflows(Some(WorkflowVisibility::User)).unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].id, "u1");

    let all = db.list_workflows(None).unwrap();
    assert_eq!(all.len(), 2);

    let got = db.get_workflow("h1").unwrap();
    assert_eq!(got.visibility, WorkflowVisibility::DashboardHidden);
}

#[test]
fn dashboard_widget_cascade_and_refs() {
    let db = AppDb::open_in_memory().unwrap();
    db.upsert_workflow(&sample_workflow("wf1", WorkflowVisibility::User))
        .unwrap();
    db.upsert_dashboard(&sample_dashboard("d1")).unwrap();
    db.upsert_widget(&sample_widget("w1", "d1", "wf1")).unwrap();

    let refs = db.find_workflow_refs("wf1").unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].widget_title, "Widget w1");

    let err = db.delete_workflow("wf1").unwrap_err();
    assert!(matches!(err, AppDbError::WorkflowInUse(_)));

    db.delete_dashboard("d1").unwrap();
    assert!(db.list_widgets("d1").unwrap().is_empty());
    db.delete_workflow("wf1").unwrap();
}

#[test]
fn widget_fk_requires_workflow() {
    let db = AppDb::open_in_memory().unwrap();
    db.upsert_dashboard(&sample_dashboard("d1")).unwrap();
    let err = db
        .upsert_widget(&sample_widget("w1", "d1", "missing"))
        .unwrap_err();
    assert!(matches!(err, AppDbError::Sqlite(_)));
}

#[test]
fn refresh_interval_validation() {
    let db = AppDb::open_in_memory().unwrap();
    db.upsert_workflow(&sample_workflow("wf1", WorkflowVisibility::User))
        .unwrap();
    db.upsert_dashboard(&sample_dashboard("d1")).unwrap();
    let mut w = sample_widget("w1", "d1", "wf1");
    w.refresh_mode = "interval".into();
    w.refresh_sec = Some(10);
    let err = db.upsert_widget(&w).unwrap_err();
    assert!(matches!(err, AppDbError::Validation(_)));

    w.refresh_sec = Some(30);
    db.upsert_widget(&w).unwrap();
}

#[test]
fn write_run_caps_rows_and_updates_latest() {
    let db = AppDb::open_in_memory().unwrap();
    db.upsert_workflow(&sample_workflow("wf1", WorkflowVisibility::User))
        .unwrap();
    db.upsert_dashboard(&sample_dashboard("d1")).unwrap();
    db.upsert_widget(&sample_widget("w1", "d1", "wf1")).unwrap();

    let big_rows: Vec<Vec<i32>> = (0..600).map(|i| vec![i]).collect();
    let run = WidgetRunRecord {
        id: "r1".into(),
        dashboard_id: "d1".into(),
        widget_id: "w1".into(),
        workflow_id: "wf1".into(),
        started_at: now(),
        finished_at: now(),
        status: "ok".into(),
        error: None,
        row_count: 600,
        columns_json: r#"["n"]"#.into(),
        rows_json: serde_json::to_string(&big_rows).unwrap(),
        variables_json: None,
        alert_fired: None,
        alert_value: None,
    };
    db.write_run(run, 200, 30).unwrap();
    let got = db.get_run("r1").unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_str(&got.rows_json).unwrap();
    assert_eq!(rows.len(), MAX_RUN_ROWS);

    let latest: String = db
        .with_conn(|conn| {
            Ok(conn.query_row(
                "SELECT run_id FROM widget_latest_run WHERE widget_id = 'w1'",
                [],
                |row| row.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(latest, "r1");
}

#[test]
fn pause_dashboard_and_list_runs_order() {
    let db = AppDb::open_in_memory().unwrap();
    db.upsert_workflow(&sample_workflow("wf1", WorkflowVisibility::User))
        .unwrap();
    db.upsert_dashboard(&sample_dashboard("d1")).unwrap();
    db.upsert_widget(&sample_widget("w1", "d1", "wf1")).unwrap();
    db.set_dashboard_refresh_paused("d1", true).unwrap();
    assert!(db.get_dashboard("d1").unwrap().refresh_paused);

    for (id, offset_secs) in [("r1", 10i64), ("r2", 20i64)] {
        let started = (Utc::now() - chrono::Duration::seconds(30 - offset_secs)).to_rfc3339();
        db.write_run(
            WidgetRunRecord {
                id: id.into(),
                dashboard_id: "d1".into(),
                widget_id: "w1".into(),
                workflow_id: "wf1".into(),
                started_at: started.clone(),
                finished_at: started,
                status: "ok".into(),
                error: None,
                row_count: 0,
                columns_json: "[]".into(),
                rows_json: "[]".into(),
                variables_json: None,
                alert_fired: None,
                alert_value: None,
            },
            200,
            30,
        )
        .unwrap();
    }
    let list = db.list_run_index("w1", 10).unwrap();
    assert_eq!(list[0].id, "r2");
    assert_eq!(list[1].id, "r1");
}
