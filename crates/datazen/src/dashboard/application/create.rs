//! Portable dashboard widget/workflow defaults.
use crate::dashboard::domain::types::{
    ChartConfig, Dashboard, DashboardWidget, RefreshMode, RefreshPolicy, ViewMode, WidgetLayout,
};
use crate::shared::scope::OrganizationScope;
use crate::workflow::domain::model::{
    WorkflowDefinition, WorkflowStep, WorkflowVisibility as WfVisibility,
};
use uuid::Uuid;
pub fn default_chart_config() -> ChartConfig {
    ChartConfig::default()
}

pub fn next_widget_layout(dashboard: &Dashboard) -> WidgetLayout {
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

pub fn hidden_sql_workflow(
    workflow_id: &str,
    title: &str,
    connection_id: &str,
    sql: &str,
) -> WorkflowDefinition {
    WorkflowDefinition {
        database: None,
        id: workflow_id.to_string(),
        name: title.to_string(),
        description: "Dashboard hidden query".into(),
        version: None,
        blocks: Vec::new(),
        author: None,
        variables: vec![],
        connection: Some(connection_id.to_string()),
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

#[derive(Debug, thiserror::Error)]
pub enum CreateWidgetError {
    #[error("store error: {0}")]
    Store(String),

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

/// 仪表板 widget 创建需要的宿主能力。
///
/// 每个方法都带一个 [`OrganizationScope`]：widget 与它引用的 workflow 都属于
/// 某个组织，宿主据此拒绝别的组织的数据（CM-05）。
#[async_trait::async_trait]
pub trait WidgetCreationHost: Send + Sync {
    fn dashboard(&self, scope: &OrganizationScope, id: &str) -> Result<Dashboard, String>;
    fn save_dashboard(&self, scope: &OrganizationScope, dashboard: Dashboard)
        -> Result<(), String>;
    fn workflow_summary(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<(String, WfVisibility), String>;
    async fn workflow(&self, scope: &OrganizationScope, id: &str) -> Option<WorkflowDefinition>;
    async fn save_workflow(
        &self,
        scope: &OrganizationScope,
        workflow: &WorkflowDefinition,
    ) -> Result<(), String>;
}

// 组织作用域是边界参数，不参与「这组入参讲的是同一件事」的语义；
// 参数个数因此超过 clippy 的阈值——显式标注，不靠改写语义来凑数。
#[allow(clippy::too_many_arguments)]
pub async fn create_widget_from_sql(
    scope: &OrganizationScope,
    host: &dyn WidgetCreationHost,
    dashboard_id: &str,
    connection_id: &str,
    sql: &str,
    title: Option<String>,
    view_mode: ViewMode,
    chart_config: Option<ChartConfig>,
) -> Result<CreateWidgetResult, CreateWidgetError> {
    if connection_id.trim().is_empty() {
        return Err(CreateWidgetError::Validation(
            "connectionId is required".into(),
        ));
    }
    if sql.trim().is_empty() {
        return Err(CreateWidgetError::Validation("sql is required".into()));
    }

    let mut dashboard = host
        .dashboard(scope, dashboard_id)
        .map_err(CreateWidgetError::Store)?;
    let widget_title = title.unwrap_or_else(|| "SQL Widget".into());
    let workflow_id = format!("dash-wf-{}", Uuid::new_v4());
    let widget_id = Uuid::new_v4().to_string();

    let def = hidden_sql_workflow(&workflow_id, &widget_title, connection_id, sql);
    host.save_workflow(scope, &def)
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
    host.save_dashboard(scope, dashboard.clone())
        .map_err(CreateWidgetError::Store)?;

    Ok(CreateWidgetResult { dashboard, widget })
}

/// Update SQL + connection on a dashboard-owned hidden workflow.
pub async fn update_hidden_workflow_sql(
    scope: &OrganizationScope,
    host: &dyn WidgetCreationHost,
    workflow_id: &str,
    connection_id: &str,
    sql: &str,
) -> Result<(), CreateWidgetError> {
    if connection_id.trim().is_empty() {
        return Err(CreateWidgetError::Validation(
            "connectionId is required".into(),
        ));
    }
    if sql.trim().is_empty() {
        return Err(CreateWidgetError::Validation("sql is required".into()));
    }

    let record = host
        .workflow_summary(scope, workflow_id)
        .map_err(|e| CreateWidgetError::Validation(e.to_string()))?;
    if record.1 != WfVisibility::DashboardHidden {
        return Err(CreateWidgetError::Validation(
            "Workflow is not dashboard-hidden".into(),
        ));
    }

    let mut def = host.workflow(scope, workflow_id).await.ok_or_else(|| {
        CreateWidgetError::Workflow(format!("Workflow '{workflow_id}' not found"))
    })?;

    def.connection = Some(connection_id.to_string());
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

    host.save_workflow(scope, &def)
        .await
        .map_err(CreateWidgetError::Workflow)?;

    Ok(())
}

pub async fn create_widget_from_workflow(
    scope: &OrganizationScope,
    host: &dyn WidgetCreationHost,
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

    let record = host
        .workflow_summary(scope, workflow_id)
        .map_err(|e| CreateWidgetError::Validation(e.to_string()))?;
    if record.1 == WfVisibility::DashboardHidden {
        return Err(CreateWidgetError::Validation(
            "Cannot bind a dashboard-hidden workflow directly; use SQL add instead".into(),
        ));
    }

    let mut dashboard = host
        .dashboard(scope, dashboard_id)
        .map_err(CreateWidgetError::Store)?;
    let widget_title = title.unwrap_or_else(|| record.0.clone());
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
    host.save_dashboard(scope, dashboard.clone())
        .map_err(CreateWidgetError::Store)?;

    Ok(CreateWidgetResult { dashboard, widget })
}
