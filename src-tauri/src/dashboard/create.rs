//! Create dashboard widgets from SQL or existing workflows.

use chrono::Utc;
use uuid::Uuid;

use super::store::{get_dashboard, save_dashboard, DashboardStoreError};
use super::types::{
    ChartConfig, Dashboard, DashboardWidget, RefreshMode, RefreshPolicy, ViewMode, WidgetLayout,
};
use crate::store::{AppDb, WorkflowRecord, WorkflowVisibility};
use crate::workflow::model::{
    WorkflowDefinition, WorkflowStep, WorkflowVisibility as WfVisibility,
};
use crate::workflow::WorkflowRegistry;

#[derive(Debug, thiserror::Error)]
pub enum CreateWidgetError {
    #[error("store error: {0}")]
    Store(#[from] DashboardStoreError),

    #[error("validation: {0}")]
    Validation(String),

    #[error("workflow error: {0}")]
    Workflow(String),
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWidgetResult {
    pub dashboard: Dashboard,
    pub widget: DashboardWidget,
}

fn default_chart_config() -> ChartConfig {
    ChartConfig::default()
}

fn next_widget_layout(dashboard: &Dashboard) -> WidgetLayout {
    let max_y = dashboard
        .widgets
        .iter()
        .map(|w| w.layout.y + w.layout.h)
        .max()
        .unwrap_or(0);
    WidgetLayout {
        x: 0,
        y: max_y,
        w: 6,
        h: 4,
    }
}

fn hidden_sql_workflow(
    workflow_id: &str,
    title: &str,
    config_id: &str,
    sql: &str,
) -> WorkflowDefinition {
    WorkflowDefinition {
        id: workflow_id.to_string(),
        name: title.to_string(),
        description: "Dashboard hidden query".into(),
        version: None,
        author: None,
        variables: vec![],
        connection: Some(config_id.to_string()),
        steps: vec![WorkflowStep::Query {
            id: "q1".into(),
            sql: sql.to_string(),
            connection: None,
            database: None,
            timeout_secs: Some(60),
            on_error: None,
        }],
        output: None,
        timeout_secs: Some(60),
        error_handling: None,
        schedule: None,
        visibility: WfVisibility::DashboardHidden,
    }
}

pub async fn create_widget_from_sql(
    app_db: &AppDb,
    registry: &WorkflowRegistry,
    dashboard_id: &str,
    config_id: &str,
    sql: &str,
    title: Option<String>,
    view_mode: ViewMode,
    chart_config: Option<ChartConfig>,
) -> Result<CreateWidgetResult, CreateWidgetError> {
    if config_id.trim().is_empty() {
        return Err(CreateWidgetError::Validation(
            "config_id is required".into(),
        ));
    }
    if sql.trim().is_empty() {
        return Err(CreateWidgetError::Validation("sql is required".into()));
    }

    let mut dashboard = get_dashboard(app_db, dashboard_id)?;
    let widget_title = title.unwrap_or_else(|| "SQL Widget".into());
    let workflow_id = format!("dash-wf-{}", Uuid::new_v4());
    let widget_id = Uuid::new_v4().to_string();

    let def = hidden_sql_workflow(&workflow_id, &widget_title, config_id, sql);
    registry
        .save_workflow(&def)
        .await
        .map_err(CreateWidgetError::Workflow)?;

    let widget = DashboardWidget {
        id: widget_id,
        title: widget_title,
        workflow_id,
        view_mode,
        chart_config: chart_config.or(Some(default_chart_config())),
        layout: next_widget_layout(&dashboard),
        refresh: RefreshPolicy {
            mode: RefreshMode::Manual,
            refresh_sec: None,
        },
        alert: None,
        enabled: true,
    };

    dashboard.widgets.push(widget.clone());
    save_dashboard(app_db, dashboard.clone())?;

    Ok(CreateWidgetResult { dashboard, widget })
}

/// Update SQL + connection on a dashboard-owned hidden workflow.
pub async fn update_hidden_workflow_sql(
    app_db: &AppDb,
    registry: &WorkflowRegistry,
    workflow_id: &str,
    config_id: &str,
    sql: &str,
) -> Result<(), CreateWidgetError> {
    if config_id.trim().is_empty() {
        return Err(CreateWidgetError::Validation(
            "config_id is required".into(),
        ));
    }
    if sql.trim().is_empty() {
        return Err(CreateWidgetError::Validation("sql is required".into()));
    }

    let record = app_db
        .get_workflow(workflow_id)
        .map_err(|e| CreateWidgetError::Validation(e.to_string()))?;
    if record.visibility != WorkflowVisibility::DashboardHidden {
        return Err(CreateWidgetError::Validation(
            "Workflow is not dashboard-hidden".into(),
        ));
    }

    let mut def = registry.get(workflow_id).await.ok_or_else(|| {
        CreateWidgetError::Workflow(format!("Workflow '{workflow_id}' not found"))
    })?;

    def.connection = Some(config_id.to_string());
    let mut updated = false;
    for step in &mut def.steps {
        if let WorkflowStep::Query { sql: step_sql, .. } = step {
            *step_sql = sql.to_string();
            updated = true;
            break;
        }
    }
    if !updated {
        return Err(CreateWidgetError::Validation(
            "Hidden workflow has no query step".into(),
        ));
    }

    registry
        .save_workflow(&def)
        .await
        .map_err(CreateWidgetError::Workflow)?;

    Ok(())
}

pub async fn create_widget_from_workflow(
    app_db: &AppDb,
    dashboard_id: &str,
    workflow_id: &str,
    title: Option<String>,
    view_mode: ViewMode,
    chart_config: Option<ChartConfig>,
) -> Result<CreateWidgetResult, CreateWidgetError> {
    if workflow_id.trim().is_empty() {
        return Err(CreateWidgetError::Validation(
            "workflow_id is required".into(),
        ));
    }

    let record = app_db
        .get_workflow(workflow_id)
        .map_err(|e| CreateWidgetError::Validation(e.to_string()))?;
    if record.visibility == WorkflowVisibility::DashboardHidden {
        return Err(CreateWidgetError::Validation(
            "Cannot bind a dashboard-hidden workflow directly; use SQL add instead".into(),
        ));
    }

    let mut dashboard = get_dashboard(app_db, dashboard_id)?;
    let widget_title = title.unwrap_or_else(|| record.name.clone());
    let widget_id = Uuid::new_v4().to_string();

    let widget = DashboardWidget {
        id: widget_id,
        title: widget_title,
        workflow_id: workflow_id.to_string(),
        view_mode,
        chart_config: chart_config.or(Some(default_chart_config())),
        layout: next_widget_layout(&dashboard),
        refresh: RefreshPolicy {
            mode: RefreshMode::Manual,
            refresh_sec: None,
        },
        alert: None,
        enabled: true,
    };

    dashboard.widgets.push(widget.clone());
    save_dashboard(app_db, dashboard.clone())?;

    Ok(CreateWidgetResult { dashboard, widget })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::store::save_dashboard;
    use crate::dashboard::types::DashboardLayout;
    use crate::store::AppDb;

    async fn setup() -> (std::sync::Arc<AppDb>, WorkflowRegistry, Dashboard) {
        let db = AppDb::open_in_memory().unwrap();
        let registry = WorkflowRegistry::new(db.clone(), std::path::PathBuf::from("/tmp"));
        let dash = Dashboard {
            id: "d1".into(),
            name: "Board".into(),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
            layout: DashboardLayout {
                cols: 12,
                row_height: 80,
            },
            widgets: vec![],
            enabled: true,
            refresh_paused: false,
        };
        save_dashboard(&db, dash.clone()).unwrap();
        (db, registry, dash)
    }

    #[tokio::test]
    async fn create_widget_from_sql_adds_hidden_workflow() {
        let (db, registry, dash) = setup().await;
        let result = create_widget_from_sql(
            &db,
            &registry,
            &dash.id,
            "cfg-1",
            "SELECT 1 AS v",
            Some("My SQL".into()),
            ViewMode::Chart,
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.widget.title, "My SQL");
        assert_eq!(result.dashboard.widgets.len(), 1);
        let wf = db.get_workflow(&result.widget.workflow_id).unwrap();
        assert_eq!(wf.visibility, WorkflowVisibility::DashboardHidden);
    }

    #[tokio::test]
    async fn update_hidden_workflow_sql_updates_query() {
        let (db, registry, dash) = setup().await;
        let created = create_widget_from_sql(
            &db,
            &registry,
            &dash.id,
            "cfg-1",
            "SELECT 1 AS v",
            Some("SQL".into()),
            ViewMode::Chart,
            None,
        )
        .await
        .unwrap();

        update_hidden_workflow_sql(
            &db,
            &registry,
            &created.widget.workflow_id,
            "cfg-2",
            "SELECT 2 AS v",
        )
        .await
        .unwrap();

        let wf = registry.get(&created.widget.workflow_id).await.unwrap();
        assert_eq!(wf.connection.as_deref(), Some("cfg-2"));
        match &wf.steps[0] {
            WorkflowStep::Query { sql, .. } => assert_eq!(sql, "SELECT 2 AS v"),
            other => panic!("expected query step, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn update_hidden_workflow_sql_rejects_user_workflow() {
        let (db, registry, _) = setup().await;
        db.upsert_workflow(&WorkflowRecord {
            id: "user-wf".into(),
            name: "User".into(),
            description: String::new(),
            visibility: WorkflowVisibility::User,
            definition_yaml:
                "id: user-wf\nname: User\nconnection: cfg\nsteps:\n  - type: query\n    id: q1\n    sql: SELECT 1\n"
                    .into(),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        })
        .unwrap();

        let err = update_hidden_workflow_sql(&db, &registry, "user-wf", "cfg", "SELECT 2")
            .await
            .unwrap_err();
        assert!(matches!(err, CreateWidgetError::Validation(_)));
    }

    #[tokio::test]
    async fn create_widget_from_workflow_rejects_hidden() {
        let (db, registry, dash) = setup().await;
        db.upsert_workflow(&WorkflowRecord {
            id: "hidden".into(),
            name: "Hidden".into(),
            description: String::new(),
            visibility: WorkflowVisibility::DashboardHidden,
            definition_yaml: "id: hidden\nname: Hidden\nsteps: []\n".into(),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        })
        .unwrap();

        let err = create_widget_from_workflow(&db, &dash.id, "hidden", None, ViewMode::Table, None)
            .await
            .unwrap_err();
        assert!(matches!(err, CreateWidgetError::Validation(_)));
    }
}
