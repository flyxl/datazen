//! Host bindings for the shared dashboard store.
//!
//! The desktop host keeps the rows in its SQLite store; the rules that
//! decide what a dashboard contains live in
//! `datazen_feature_core::dashboard::repository`.
//!
//! ## 组织作用域
//!
//! 桌面是**单租户**：一份 app data 就是一个组织。实现只承认**自己那一个组织**
//! ——别的组织的作用域一律按「不存在」处理（空列表 / NotFound），绝不返回
//! 「无权限」类错误（CM-05），也绝不把本组织的数据交出去。
//!
//! SQLite **不加** `organization_id` 列：桌面没有第二个组织，schema 不需要变。

use crate::platform::scope::is_local_scope;
use crate::store::{AppDb, AppDbError, AppSettings};
use datazen_feature_core::dashboard::records::{DashboardRecord, WidgetRecord};
use datazen_feature_core::dashboard::repository::{DashboardRepository, DashboardRepositoryError};
use datazen_feature_core::dashboard::types::{DashboardWorkflowRef, MonitorSettings};
use datazen_platform_api::ports::scope::OrganizationScope;

/// The monitor settings the desktop host persists in its settings file.
pub fn load_monitor_settings(settings: &AppSettings) -> MonitorSettings {
    settings.monitor.clone()
}

fn store_error(error: AppDbError) -> DashboardRepositoryError {
    match error {
        AppDbError::NotFound(id) => DashboardRepositoryError::NotFound(id),
        other => DashboardRepositoryError::Db(other.to_string()),
    }
}

impl DashboardRepository for AppDb {
    fn list_dashboard_rows(
        &self,
        scope: &OrganizationScope,
    ) -> Result<Vec<DashboardRecord>, DashboardRepositoryError> {
        if !is_local_scope(scope) {
            return Ok(Vec::new());
        }
        self.list_dashboards().map_err(store_error)
    }

    fn dashboard_row(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<DashboardRecord, DashboardRepositoryError> {
        if !is_local_scope(scope) {
            return Err(DashboardRepositoryError::NotFound(id.to_string()));
        }
        self.get_dashboard(id).map_err(store_error)
    }

    fn upsert_dashboard_row(
        &self,
        scope: &OrganizationScope,
        row: &DashboardRecord,
    ) -> Result<(), DashboardRepositoryError> {
        if !is_local_scope(scope) {
            return Err(DashboardRepositoryError::NotFound(row.id.clone()));
        }
        self.upsert_dashboard(row).map_err(store_error)
    }

    fn delete_dashboard_row(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<(), DashboardRepositoryError> {
        if !is_local_scope(scope) {
            return Err(DashboardRepositoryError::NotFound(id.to_string()));
        }
        self.delete_dashboard(id).map_err(store_error)
    }

    fn set_dashboard_refresh_paused(
        &self,
        scope: &OrganizationScope,
        id: &str,
        paused: bool,
    ) -> Result<(), DashboardRepositoryError> {
        if !is_local_scope(scope) {
            return Err(DashboardRepositoryError::NotFound(id.to_string()));
        }
        self.set_dashboard_refresh_paused(id, paused)
            .map_err(store_error)
    }

    fn list_widget_rows(
        &self,
        scope: &OrganizationScope,
        dashboard_id: &str,
    ) -> Result<Vec<WidgetRecord>, DashboardRepositoryError> {
        if !is_local_scope(scope) {
            return Ok(Vec::new());
        }
        self.list_widgets(dashboard_id).map_err(store_error)
    }

    fn widget_row(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<WidgetRecord, DashboardRepositoryError> {
        if !is_local_scope(scope) {
            return Err(DashboardRepositoryError::NotFound(id.to_string()));
        }
        self.get_widget(id).map_err(store_error)
    }

    fn upsert_widget_row(
        &self,
        scope: &OrganizationScope,
        row: &WidgetRecord,
    ) -> Result<(), DashboardRepositoryError> {
        if !is_local_scope(scope) {
            return Err(DashboardRepositoryError::NotFound(row.id.clone()));
        }
        self.upsert_widget(row).map_err(store_error)
    }

    fn delete_widget_row(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<(), DashboardRepositoryError> {
        if !is_local_scope(scope) {
            return Err(DashboardRepositoryError::NotFound(id.to_string()));
        }
        self.delete_widget(id).map_err(store_error)
    }

    fn find_workflow_refs(
        &self,
        scope: &OrganizationScope,
        workflow_id: &str,
    ) -> Result<Vec<DashboardWorkflowRef>, DashboardRepositoryError> {
        if !is_local_scope(scope) {
            return Ok(Vec::new());
        }
        self.find_workflow_refs(workflow_id).map_err(store_error)
    }
}
