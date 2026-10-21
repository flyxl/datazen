//! Host bindings for the shared dashboard store.
//!
//! The desktop host keeps the rows in its SQLite store; the rules that
//! decide what a dashboard contains live in
//! `datazen_feature_core::dashboard::store`.

use crate::store::{AppDb, AppDbError, AppSettings};
use datazen_feature_core::dashboard::records::{DashboardRecord, WidgetRecord};
use datazen_feature_core::dashboard::store::{DashboardStore, DashboardStoreError};
use datazen_feature_core::dashboard::types::{DashboardWorkflowRef, MonitorSettings};

/// The monitor settings the desktop host persists in its settings file.
pub fn load_monitor_settings(settings: &AppSettings) -> MonitorSettings {
    settings.monitor.clone()
}

fn store_error(error: AppDbError) -> DashboardStoreError {
    match error {
        AppDbError::NotFound(id) => DashboardStoreError::NotFound(id),
        other => DashboardStoreError::Db(other.to_string()),
    }
}

impl DashboardStore for AppDb {
    fn list_dashboard_rows(&self) -> Result<Vec<DashboardRecord>, DashboardStoreError> {
        self.list_dashboards().map_err(store_error)
    }

    fn dashboard_row(&self, id: &str) -> Result<DashboardRecord, DashboardStoreError> {
        self.get_dashboard(id).map_err(store_error)
    }

    fn upsert_dashboard_row(&self, row: &DashboardRecord) -> Result<(), DashboardStoreError> {
        self.upsert_dashboard(row).map_err(store_error)
    }

    fn delete_dashboard_row(&self, id: &str) -> Result<(), DashboardStoreError> {
        self.delete_dashboard(id).map_err(store_error)
    }

    fn set_dashboard_refresh_paused(
        &self,
        id: &str,
        paused: bool,
    ) -> Result<(), DashboardStoreError> {
        self.set_dashboard_refresh_paused(id, paused)
            .map_err(store_error)
    }

    fn list_widget_rows(
        &self,
        dashboard_id: &str,
    ) -> Result<Vec<WidgetRecord>, DashboardStoreError> {
        self.list_widgets(dashboard_id).map_err(store_error)
    }

    fn widget_row(&self, id: &str) -> Result<WidgetRecord, DashboardStoreError> {
        self.get_widget(id).map_err(store_error)
    }

    fn upsert_widget_row(&self, row: &WidgetRecord) -> Result<(), DashboardStoreError> {
        self.upsert_widget(row).map_err(store_error)
    }

    fn delete_widget_row(&self, id: &str) -> Result<(), DashboardStoreError> {
        self.delete_widget(id).map_err(store_error)
    }

    fn find_workflow_refs(
        &self,
        workflow_id: &str,
    ) -> Result<Vec<DashboardWorkflowRef>, DashboardStoreError> {
        self.find_workflow_refs(workflow_id).map_err(store_error)
    }
}
