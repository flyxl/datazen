//! Desktop repository bindings for portable dashboard creation use cases.
use super::types::{ChartConfig, Dashboard, ViewMode};
use crate::store::{AppDb, WorkflowVisibility};
use crate::workflow::{
    model::{WorkflowDefinition, WorkflowVisibility as WfVisibility},
    WorkflowRegistry,
};
pub use datazen_feature_core::dashboard::create::{CreateWidgetError, CreateWidgetResult};
struct DesktopCreationHost<'a> {
    app_db: &'a AppDb,
    registry: Option<&'a WorkflowRegistry>,
}
#[async_trait::async_trait]
impl datazen_feature_core::dashboard::create::WidgetCreationHost for DesktopCreationHost<'_> {
    fn dashboard(&self, id: &str) -> Result<Dashboard, String> {
        super::store::get_dashboard(self.app_db, id).map_err(|error| error.to_string())
    }
    fn save_dashboard(&self, dashboard: Dashboard) -> Result<(), String> {
        super::store::save_dashboard(self.app_db, dashboard)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    fn workflow_summary(&self, id: &str) -> Result<(String, WfVisibility), String> {
        let record = self
            .app_db
            .get_workflow(id)
            .map_err(|error| error.to_string())?;
        Ok((
            record.name,
            match record.visibility {
                WorkflowVisibility::User => WfVisibility::User,
                WorkflowVisibility::DashboardHidden => WfVisibility::DashboardHidden,
            },
        ))
    }
    async fn workflow(&self, id: &str) -> Option<WorkflowDefinition> {
        match self.registry {
            Some(registry) => registry.get(id).await,
            None => None,
        }
    }
    async fn save_workflow(&self, workflow: &WorkflowDefinition) -> Result<(), String> {
        self.registry
            .ok_or("Workflow repository unavailable")?
            .save_workflow(workflow)
            .await
            .map(|_| ())
    }
}
pub async fn create_widget_from_sql(
    app_db: &AppDb,
    registry: &WorkflowRegistry,
    dashboard_id: &str,
    connection_id: &str,
    sql: &str,
    title: Option<String>,
    view_mode: ViewMode,
    chart_config: Option<ChartConfig>,
) -> Result<CreateWidgetResult, CreateWidgetError> {
    datazen_feature_core::dashboard::create::create_widget_from_sql(
        &DesktopCreationHost {
            app_db,
            registry: Some(registry),
        },
        dashboard_id,
        connection_id,
        sql,
        title,
        view_mode,
        chart_config,
    )
    .await
}
pub async fn update_hidden_workflow_sql(
    app_db: &AppDb,
    registry: &WorkflowRegistry,
    workflow_id: &str,
    connection_id: &str,
    sql: &str,
) -> Result<(), CreateWidgetError> {
    datazen_feature_core::dashboard::create::update_hidden_workflow_sql(
        &DesktopCreationHost {
            app_db,
            registry: Some(registry),
        },
        workflow_id,
        connection_id,
        sql,
    )
    .await
}
pub async fn create_widget_from_workflow(
    app_db: &AppDb,
    dashboard_id: &str,
    workflow_id: &str,
    title: Option<String>,
    view_mode: ViewMode,
    chart_config: Option<ChartConfig>,
) -> Result<CreateWidgetResult, CreateWidgetError> {
    datazen_feature_core::dashboard::create::create_widget_from_workflow(
        &DesktopCreationHost {
            app_db,
            registry: None,
        },
        dashboard_id,
        workflow_id,
        title,
        view_mode,
        chart_config,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::store::save_dashboard;
    use crate::dashboard::types::DashboardLayout;
    use crate::store::{AppDb, WorkflowRecord};
    use crate::workflow::WorkflowStep;
    use chrono::Utc;

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
        let (db, _registry, dash) = setup().await;
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
