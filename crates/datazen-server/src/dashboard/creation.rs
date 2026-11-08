//! 组件创建用例的 server 宿主（W8 刀3d-2）。
//!
//! 桌面对照物：`datazen-desktop/src/dashboard/create.rs` 的
//! `DesktopCreationHost`。用例规则（SQL→隐藏工作流铸造、widget 行合成、
//! 隐藏 SQL 更新）单家在共享层 `dashboard::create`；本宿主供四端口：
//!
//! - `dashboard` / `save_dashboard`——v14 仪表板存储（共享层
//!   `repository` 的规则函数直架其上）；
//! - `workflow_summary`——定义行的 name/visibility 投影（服务端不查
//!   本地组织：组织键就是宿主配置的组织，scope 不匹配 = 「不存在」，
//!   CM-05）；
//! - `workflow` / `save_workflow`——共享 registry（读定义 / 写定义——
//!   `create_widget_from_sql` 铸造 `DashboardHidden` 工作流的地方）。

use std::sync::Arc;

use datazen::dashboard::types::Dashboard;
use datazen::shared::scope::OrganizationScope;
use datazen::workflow::model::{WorkflowDefinition, WorkflowVisibility as WfVisibility};
use datazen::workflow::registry::WorkflowRegistry;

use crate::storage::ServerDashboardStore;

/// server 形态的组件创建宿主。
pub struct ServerWidgetCreationHost {
    pub dashboards: ServerDashboardStore,
    pub registry: Arc<WorkflowRegistry>,
}

#[async_trait::async_trait]
impl datazen::dashboard::create::WidgetCreationHost for ServerWidgetCreationHost {
    fn dashboard(&self, scope: &OrganizationScope, id: &str) -> Result<Dashboard, String> {
        datazen::dashboard::repository::get_dashboard(scope, &self.dashboards, id)
            .map_err(|error| error.to_string())
    }

    fn save_dashboard(
        &self,
        scope: &OrganizationScope,
        dashboard: Dashboard,
    ) -> Result<(), String> {
        datazen::dashboard::repository::save_dashboard(scope, &self.dashboards, dashboard)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn workflow_summary(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<(String, WfVisibility), String> {
        // 定义行的列投影（存储列胜过文档——重命名后旧名字不回读）。
        let store = self.registry.store();
        store
            .get_workflow(scope, id)
            .map_err(|error| error.to_string())?
            .map(|record| (record.name, record.visibility))
            .ok_or_else(|| format!("Workflow '{id}' not found"))
    }

    async fn workflow(&self, scope: &OrganizationScope, id: &str) -> Option<WorkflowDefinition> {
        self.registry.get(scope, id).await
    }

    async fn save_workflow(
        &self,
        scope: &OrganizationScope,
        workflow: &WorkflowDefinition,
    ) -> Result<(), String> {
        self.registry.save(scope, workflow).await
    }
}
