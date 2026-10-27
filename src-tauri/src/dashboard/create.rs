//! Desktop repository bindings for portable dashboard creation use cases.
use crate::store::{AppDb, WorkflowVisibility};
use crate::workflow::WorkflowRegistry;
pub use datazen::dashboard::create::{CreateWidgetError, CreateWidgetResult};
use datazen::dashboard::types::{ChartConfig, Dashboard, ViewMode};
use datazen::shared::scope::OrganizationScope;
use datazen::workflow::model::{WorkflowDefinition, WorkflowVisibility as WfVisibility};
struct DesktopCreationHost<'a> {
    app_db: &'a AppDb,
    registry: Option<&'a WorkflowRegistry>,
}
#[async_trait::async_trait]
impl datazen::dashboard::create::WidgetCreationHost for DesktopCreationHost<'_> {
    fn dashboard(&self, scope: &OrganizationScope, id: &str) -> Result<Dashboard, String> {
        datazen::dashboard::repository::get_dashboard(scope, self.app_db, id)
            .map_err(|error| error.to_string())
    }
    fn save_dashboard(
        &self,
        scope: &OrganizationScope,
        dashboard: Dashboard,
    ) -> Result<(), String> {
        datazen::dashboard::repository::save_dashboard(scope, self.app_db, dashboard)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    fn workflow_summary(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<(String, WfVisibility), String> {
        // 桌面是单租户：别的组织的工作流对本组织**不存在**（CM-05）。
        if !crate::platform::scope::is_local_scope(scope) {
            return Err(format!("Workflow '{id}' not found"));
        }
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
    async fn workflow(&self, scope: &OrganizationScope, id: &str) -> Option<WorkflowDefinition> {
        match self.registry {
            Some(registry) => registry.get(scope, id).await,
            None => None,
        }
    }
    async fn save_workflow(
        &self,
        scope: &OrganizationScope,
        workflow: &WorkflowDefinition,
    ) -> Result<(), String> {
        self.registry
            .ok_or("Workflow repository unavailable")?
            .save_workflow(scope, workflow)
            .await
            .map(|_| ())
    }
}
// 组织作用域是边界参数，不参与「这组入参讲的是同一件事」的语义。
#[allow(clippy::too_many_arguments)]
pub async fn create_widget_from_sql(
    scope: &OrganizationScope,
    app_db: &AppDb,
    registry: &WorkflowRegistry,
    dashboard_id: &str,
    connection_id: &str,
    sql: &str,
    title: Option<String>,
    view_mode: ViewMode,
    chart_config: Option<ChartConfig>,
) -> Result<CreateWidgetResult, CreateWidgetError> {
    datazen::dashboard::create::create_widget_from_sql(
        scope,
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
    scope: &OrganizationScope,
    app_db: &AppDb,
    registry: &WorkflowRegistry,
    workflow_id: &str,
    connection_id: &str,
    sql: &str,
) -> Result<(), CreateWidgetError> {
    datazen::dashboard::create::update_hidden_workflow_sql(
        scope,
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
    scope: &OrganizationScope,
    app_db: &AppDb,
    dashboard_id: &str,
    workflow_id: &str,
    title: Option<String>,
    view_mode: ViewMode,
    chart_config: Option<ChartConfig>,
) -> Result<CreateWidgetResult, CreateWidgetError> {
    datazen::dashboard::create::create_widget_from_workflow(
        scope,
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
    use crate::store::{AppDb, WorkflowRecord};
    use crate::workflow::registry::AppDbWorkflowRepository;
    use chrono::Utc;
    use datazen::dashboard::repository::save_dashboard;
    use datazen::dashboard::types::DashboardLayout;
    use datazen::shared::id::OrganizationId;
    use datazen::shared::scope::OrganizationScope;
    use datazen::workflow::model::WorkflowStep;
    use std::sync::Arc;

    /// 别的组织的作用域。
    fn foreign_scope() -> OrganizationScope {
        OrganizationScope::new(OrganizationId::new("another-organization"))
    }

    async fn setup() -> (std::sync::Arc<AppDb>, WorkflowRegistry, Dashboard) {
        let db = AppDb::open_in_memory().unwrap();
        let registry = WorkflowRegistry::new(
            Arc::new(AppDbWorkflowRepository::new(db.clone())),
            std::path::PathBuf::from("/tmp"),
        );
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
        save_dashboard(
            &crate::platform::local_organization_scope(),
            &*db,
            dash.clone(),
        )
        .unwrap();
        (db, registry, dash)
    }

    #[tokio::test]
    async fn create_widget_from_sql_adds_hidden_workflow() {
        let (db, registry, dash) = setup().await;
        let result = create_widget_from_sql(
            &crate::platform::local_organization_scope(),
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
            &crate::platform::local_organization_scope(),
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
            &crate::platform::local_organization_scope(),
            &db,
            &registry,
            &created.widget.workflow_id,
            "cfg-2",
            "SELECT 2 AS v",
        )
        .await
        .unwrap();

        let wf = registry
            .get(
                &crate::platform::local_organization_scope(),
                &created.widget.workflow_id,
            )
            .await
            .unwrap();
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

        let err = update_hidden_workflow_sql(
            &crate::platform::local_organization_scope(),
            &db,
            &registry,
            "user-wf",
            "cfg",
            "SELECT 2",
        )
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

        let err = create_widget_from_workflow(
            &crate::platform::local_organization_scope(),
            &db,
            &dash.id,
            "hidden",
            None,
            ViewMode::Table,
            None,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CreateWidgetError::Validation(_)));
    }

    // ─── 组织隔离（WidgetCreationHost 是读/写已存数据的端口） ───

    /// 别的组织建不了 widget：读不到本组织的仪表板 ⇒ 失败，且**没有**写入。
    ///
    /// `WidgetCreationHost` 直接读写已存的仪表板与 workflow，不带作用域就是越权旁路。
    #[tokio::test]
    async fn a_widget_cannot_be_created_for_another_organization() {
        let (db, registry, dash) = setup().await;
        let foreign = foreign_scope();

        let err = create_widget_from_sql(
            &foreign,
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
        .unwrap_err();
        assert!(
            matches!(err, CreateWidgetError::Store(_)),
            "读不到别的组织的仪表板应当按「不存在」失败，实际：{err:?}"
        );

        // 本组织的数据一个字节都没被动过。
        let local = crate::platform::local_organization_scope();
        let loaded = datazen::dashboard::repository::get_dashboard(&local, &*db, &dash.id)
            .expect("本组织的仪表板应当仍然读得到");
        assert!(
            loaded.widgets.is_empty(),
            "别的组织不得往本组织的仪表板里塞 widget"
        );
    }

    /// 别的组织绑不了本组织的 workflow（读路径按「不存在」处理）。
    #[tokio::test]
    async fn a_widget_cannot_be_bound_to_a_workflow_of_another_organization() {
        let (db, _registry, dash) = setup().await;
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

        let err = create_widget_from_workflow(
            &foreign_scope(),
            &db,
            &dash.id,
            "user-wf",
            None,
            ViewMode::Table,
            None,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, CreateWidgetError::Validation(_)),
            "别的组织的 workflow 应当按「不存在」处理，实际：{err:?}"
        );
        assert!(err.to_string().contains("not found"));
    }

    /// 别的组织改不动本组织 dashboard 自带的隐藏 workflow（写路径）。
    #[tokio::test]
    async fn a_hidden_workflow_of_another_organization_cannot_be_updated() {
        let (db, registry, dash) = setup().await;
        let created = create_widget_from_sql(
            &crate::platform::local_organization_scope(),
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

        let err = update_hidden_workflow_sql(
            &foreign_scope(),
            &db,
            &registry,
            &created.widget.workflow_id,
            "cfg-2",
            "SELECT 2",
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, CreateWidgetError::Validation(_)),
            "别的组织的隐藏 workflow 应当按「不存在」处理，实际：{err:?}"
        );

        // 本组织的隐藏 workflow 内容没变。
        let local = crate::platform::local_organization_scope();
        let def = registry
            .get(&local, &created.widget.workflow_id)
            .await
            .unwrap();
        match &def.steps[0] {
            WorkflowStep::Query { sql, .. } => assert_eq!(sql, "SELECT 1 AS v"),
            other => panic!("expected query step, got {other:?}"),
        }
    }
}
