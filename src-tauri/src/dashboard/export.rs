//! Single-file dashboard import/export (`.datazen-dashboard.json`).

use std::io;

use chrono::Utc;
use thiserror::Error;

use crate::store::{AppDb, AppDbError, WorkflowRecord, WorkflowVisibility as DbVisibility};
use datazen_feature_core::dashboard::repository::{
    assign_import_id, list_dashboards, save_dashboard, DashboardRepositoryError,
};
use datazen_feature_core::dashboard::types::Dashboard;
use datazen_feature_core::workflow::model::{WorkflowDefinition, WorkflowVisibility};
use datazen_modules::shared::scope::OrganizationScope;

#[derive(Debug, Error)]
pub enum DashboardExportError {
    #[error("validation: {0}")]
    Validation(String),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("store error: {0}")]
    Store(#[from] DashboardRepositoryError),

    #[error("database error: {0}")]
    Database(#[from] AppDbError),
}

fn to_db_visibility(v: WorkflowVisibility) -> DbVisibility {
    match v {
        WorkflowVisibility::User => DbVisibility::User,
        WorkflowVisibility::DashboardHidden => DbVisibility::DashboardHidden,
    }
}

fn persist_workflow(
    app_db: &AppDb,
    workflow: &WorkflowDefinition,
) -> Result<(), DashboardExportError> {
    let now = Utc::now().to_rfc3339();
    let existing = app_db.get_workflow(&workflow.id).ok();
    let created_at = existing
        .as_ref()
        .map(|r| r.created_at.clone())
        .unwrap_or_else(|| now.clone());

    let definition_yaml = serde_yaml::to_string(workflow)
        .map_err(|e| DashboardExportError::Validation(e.to_string()))?;

    let record = WorkflowRecord {
        id: workflow.id.clone(),
        name: workflow.name.clone(),
        description: workflow.description.clone(),
        visibility: to_db_visibility(workflow.visibility),
        definition_yaml,
        created_at,
        updated_at: now,
    };
    app_db.upsert_workflow(&record)?;
    Ok(())
}

fn record_to_definition(
    record: &WorkflowRecord,
) -> Result<WorkflowDefinition, DashboardExportError> {
    let mut def: WorkflowDefinition = serde_yaml::from_str(&record.definition_yaml)
        .map_err(|e| DashboardExportError::Validation(e.to_string()))?;
    def.id = record.id.clone();
    def.name = record.name.clone();
    def.description = record.description.clone();
    def.visibility = match record.visibility {
        DbVisibility::User => WorkflowVisibility::User,
        DbVisibility::DashboardHidden => WorkflowVisibility::DashboardHidden,
    };
    Ok(def)
}

fn collect_embedded_workflows(
    app_db: &AppDb,
    dashboard: &Dashboard,
) -> Result<Vec<WorkflowDefinition>, DashboardExportError> {
    datazen_feature_core::dashboard::archive::select_embedded_workflows(dashboard, |id| {
        let record = app_db.get_workflow(id)?;
        Ok(record_to_definition(&record)?)
    })
}

fn parse_dashboard_file_inner(
    bytes: &[u8],
) -> Result<(Dashboard, Vec<WorkflowDefinition>), DashboardExportError> {
    datazen_feature_core::dashboard::archive::parse_dashboard_file_inner(bytes)
        .map_err(|error| DashboardExportError::Validation(error.to_string()))
}

/// Parse and validate a single-file dashboard export. Strips accidental webhook fields.
#[cfg(test)]
pub fn parse_dashboard_file(bytes: &[u8]) -> Result<Dashboard, DashboardExportError> {
    parse_dashboard_file_inner(bytes).map(|(dashboard, _)| dashboard)
}

/// Serialize a dashboard to the single-file export format (no secrets / webhook URLs).
pub fn export_dashboard_json(
    app_db: &AppDb,
    dashboard: &Dashboard,
) -> Result<String, DashboardExportError> {
    let embedded_workflows = collect_embedded_workflows(app_db, dashboard)?;
    datazen_feature_core::dashboard::archive::serialize_dashboard_file(
        dashboard,
        embedded_workflows,
    )
    .map_err(|error| DashboardExportError::Validation(error.to_string()))
}

/// Import a dashboard file; assigns a new id when the id already exists locally.
///
/// `scope` 限定这次导入落到哪个组织：id 冲突只在**同一组织内**成立。
pub fn import_dashboard(
    scope: &OrganizationScope,
    app_db: &AppDb,
    bytes: &[u8],
) -> Result<Dashboard, DashboardExportError> {
    let (mut dashboard, embedded) = parse_dashboard_file_inner(bytes)?;

    for wf in &embedded {
        persist_workflow(app_db, wf)?;
    }

    let existing = list_dashboards(scope, app_db)?;
    assign_import_id(scope, &mut dashboard, &existing);

    save_dashboard(scope, app_db, dashboard.clone())?;
    Ok(dashboard)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{AppDb, WorkflowRecord, WorkflowVisibility};
    use chrono::Utc;
    use datazen_feature_core::dashboard::archive::FORMAT;
    use datazen_feature_core::dashboard::repository::{
        delete_dashboard, get_dashboard, list_dashboards, save_dashboard,
    };
    use datazen_feature_core::dashboard::types::{
        AlertChannel, AlertMetric, AlertMetricKind, AlertOperator, AlertRule, ChartConfig,
        ChartType, DashboardLayout, DashboardWidget, RefreshMode, RefreshPolicy, ViewMode,
        WidgetLayout,
    };

    fn open_db() -> std::sync::Arc<AppDb> {
        let db = AppDb::open_in_memory().unwrap();
        db.upsert_workflow(&WorkflowRecord {
            id: "wf-export".into(),
            name: "WF".into(),
            description: String::new(),
            visibility: WorkflowVisibility::User,
            definition_yaml: "id: wf-export\nname: WF\ndescription: ''\nsteps: []\n".into(),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        })
        .unwrap();
        db
    }

    fn sample_dashboard() -> Dashboard {
        Dashboard {
            id: "dash-export-1".into(),
            name: "Export Test".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            layout: DashboardLayout {
                cols: 12,
                row_height: 80,
            },
            widgets: vec![DashboardWidget {
                id: "w1".into(),
                title: "Widget".into(),
                workflow_id: "wf-export".into(),
                view_mode: ViewMode::Chart,
                chart_config: Some(ChartConfig {
                    chart_type: ChartType::Line,
                    x_axis: None,
                    y_axes: vec![],
                    group_by: None,
                    aggregation: datazen_feature_core::dashboard::types::AggregationType::None,
                    sort_by: datazen_feature_core::dashboard::types::ChartSortBy::None,
                    show_legend: true,
                    show_grid: true,
                    show_values: false,
                    color_scheme: "default".into(),
                }),
                layout: WidgetLayout {
                    x: 0,
                    y: 0,
                    w: 4,
                    h: 3,
                },
                refresh: RefreshPolicy {
                    mode: RefreshMode::Manual,
                    refresh_sec: None,
                },
                alert: Some(AlertRule {
                    metric: AlertMetric {
                        kind: AlertMetricKind::Column,
                        column: "v".into(),
                        agg: None,
                    },
                    op: AlertOperator::Gt,
                    threshold: 1.0,
                    cooldown_sec: 300,
                    channels: vec![AlertChannel::App, AlertChannel::Webhook],
                }),
                enabled: true,
            }],
            enabled: true,
            refresh_paused: false,
        }
    }

    #[test]
    fn import_dashboard_json_rejects_bad_format() {
        let err = parse_dashboard_file(br#"{"format":"nope"}"#).unwrap_err();
        assert!(err.to_string().contains("format"));
    }

    #[test]
    fn export_strips_webhook_channel_and_embeds_workflows() {
        let db = open_db();
        let dash = sample_dashboard();
        let json = export_dashboard_json(&db, &dash).unwrap();
        assert!(!json.contains("webhook"));
        assert!(json.contains("wf-export"));
        assert!(json.contains("embeddedWorkflows"));
        assert!(json.contains("\"version\": 2"));
        let parsed = parse_dashboard_file(json.as_bytes()).unwrap();
        assert_eq!(parsed.widgets[0].workflow_id, "wf-export");
        assert!(parsed.widgets[0]
            .alert
            .as_ref()
            .unwrap()
            .channels
            .iter()
            .all(|c| *c != AlertChannel::Webhook));
    }

    #[test]
    fn import_assigns_new_id_on_collision() {
        let db = open_db();
        let scope = crate::platform::local_organization_scope();
        let dash = sample_dashboard();
        save_dashboard(&scope, &*db, dash.clone()).unwrap();

        let json = export_dashboard_json(&db, &dash).unwrap();
        let imported = import_dashboard(&scope, &db, json.as_bytes()).unwrap();
        assert_ne!(imported.id, dash.id);
        assert_eq!(list_dashboards(&scope, &*db).unwrap().len(), 2);
    }

    #[test]
    fn import_roundtrip_persists_dashboard_and_workflows() {
        let db = open_db();
        let scope = crate::platform::local_organization_scope();
        let dash = sample_dashboard();
        let json = export_dashboard_json(&db, &dash).unwrap();
        let imported = import_dashboard(&scope, &db, json.as_bytes()).unwrap();
        let loaded = get_dashboard(&scope, &*db, &imported.id).unwrap();
        assert_eq!(loaded.widgets.len(), 1);
        assert!(db.get_workflow("wf-export").is_ok());
    }

    #[test]
    fn import_accepts_version_1_without_embedded() {
        let db = open_db();
        let dash = sample_dashboard();
        let v1 = serde_json::json!({
            "format": FORMAT,
            "version": 1,
            "dashboard": dash,
        });
        let imported = import_dashboard(
            &crate::platform::local_organization_scope(),
            &db,
            v1.to_string().as_bytes(),
        )
        .unwrap();
        assert_eq!(imported.widgets.len(), 1);
    }

    #[test]
    fn import_restores_hidden_workflow_from_embedded() {
        let db = AppDb::open_in_memory().unwrap();
        let hidden_id = "wf-hidden-export";
        db.upsert_workflow(&WorkflowRecord {
            id: hidden_id.into(),
            name: "Hidden".into(),
            description: String::new(),
            visibility: WorkflowVisibility::DashboardHidden,
            definition_yaml: format!(
                "id: {hidden_id}\nname: Hidden\ndescription: ''\nvisibility: dashboardHidden\nsteps:\n  - type: query\n    id: q1\n    sql: SELECT 1\n"
            ),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        })
        .unwrap();

        let mut dash = sample_dashboard();
        dash.widgets[0].workflow_id = hidden_id.into();
        let scope = crate::platform::local_organization_scope();
        save_dashboard(&scope, &*db, dash.clone()).unwrap();

        let json = export_dashboard_json(&db, &dash).unwrap();
        delete_dashboard(&scope, &*db, &dash.id).unwrap();
        db.delete_workflow(hidden_id).unwrap();

        let imported = import_dashboard(&scope, &db, json.as_bytes()).unwrap();
        assert_eq!(imported.widgets[0].workflow_id, hidden_id);
        let record = db.get_workflow(hidden_id).unwrap();
        assert_eq!(record.visibility, WorkflowVisibility::DashboardHidden);
    }
}
