//! Workflow persistence backed by [`crate::store::AppDb`].
//!
//! The registry's rules — which workflows a user sees, how a definition is
//! encoded, what a saved workflow looks like — live in
//! `datazen_feature_core::workflow::registry`. This module only binds them to
//! the desktop store.

use crate::store::{AppDb, AppDbError, WorkflowRecord, WorkflowVisibility as DbVisibility};
use datazen_feature_core::workflow::model::{
    WorkflowDefinition, WorkflowListItem, WorkflowVisibility,
};
use datazen_feature_core::workflow::registry::{
    WorkflowDefinitionRecord, WorkflowDefinitionStore, WorkflowRegistry as CoreRegistry,
};
use std::path::PathBuf;
use std::sync::Arc;

/// The builtin workflows the desktop host ships.
const BUILTIN_DEFINITIONS: &[(&str, &str)] = &[
    (
        "hello-query",
        include_str!("../../resources/builtin-workflows/hello-query.yaml"),
    ),
    (
        "cross-db-sample",
        include_str!("../../resources/builtin-workflows/cross-db-sample.yaml"),
    ),
    (
        "ai-summarize",
        include_str!("../../resources/builtin-workflows/ai-summarize.yaml"),
    ),
];

/// The desktop rows the shared registry reads and writes.
struct AppDbWorkflowStore {
    app_db: Arc<AppDb>,
}

impl WorkflowDefinitionStore for AppDbWorkflowStore {
    fn get_workflow(&self, id: &str) -> Result<Option<WorkflowDefinitionRecord>, String> {
        match self.app_db.get_workflow(id) {
            Ok(record) => Ok(Some(record_to_definition_record(&record))),
            Err(AppDbError::NotFound(_)) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn list_workflows(
        &self,
        visibility: WorkflowVisibility,
    ) -> Result<Vec<WorkflowDefinitionRecord>, String> {
        self.app_db
            .list_workflows(Some(to_db_visibility(visibility)))
            .map(|rows| rows.iter().map(record_to_definition_record).collect())
            .map_err(|error| error.to_string())
    }

    fn upsert_workflow(&self, record: &WorkflowDefinitionRecord) -> Result<(), String> {
        self.app_db
            .upsert_workflow(&WorkflowRecord {
                id: record.id.clone(),
                name: record.name.clone(),
                description: record.description.clone(),
                visibility: to_db_visibility(record.visibility),
                definition_yaml: record.definition_yaml.clone(),
                created_at: record.created_at.clone(),
                updated_at: record.updated_at.clone(),
            })
            .map_err(|error| error.to_string())
    }

    fn delete_workflow(&self, id: &str) -> Result<(), String> {
        match self.app_db.delete_workflow(id) {
            Ok(()) => Ok(()),
            Err(AppDbError::NotFound(_)) => Ok(()),
            Err(AppDbError::WorkflowInUse(summary)) => Err(format!(
                "Workflow is still referenced by dashboards: {summary}"
            )),
            Err(error) => Err(error.to_string()),
        }
    }
}

fn to_db_visibility(visibility: WorkflowVisibility) -> DbVisibility {
    match visibility {
        WorkflowVisibility::User => DbVisibility::User,
        WorkflowVisibility::DashboardHidden => DbVisibility::DashboardHidden,
    }
}

fn record_to_definition_record(record: &WorkflowRecord) -> WorkflowDefinitionRecord {
    WorkflowDefinitionRecord {
        id: record.id.clone(),
        name: record.name.clone(),
        description: record.description.clone(),
        visibility: match record.visibility {
            DbVisibility::User => WorkflowVisibility::User,
            DbVisibility::DashboardHidden => WorkflowVisibility::DashboardHidden,
        },
        definition_yaml: record.definition_yaml.clone(),
        created_at: record.created_at.clone(),
        updated_at: record.updated_at.clone(),
    }
}

pub struct WorkflowRegistry {
    core: CoreRegistry<AppDbWorkflowStore>,
    app_db: Arc<AppDb>,
    /// Legacy path kept for `open_workflows_dir` / diagnostics (may be empty).
    workflows_dir: PathBuf,
}

impl WorkflowRegistry {
    pub fn new(app_db: Arc<AppDb>, data_dir: PathBuf) -> Self {
        Self {
            core: CoreRegistry::new(
                AppDbWorkflowStore {
                    app_db: app_db.clone(),
                },
                BUILTIN_DEFINITIONS.to_vec(),
            ),
            app_db,
            workflows_dir: data_dir.join("workflows"),
        }
    }

    pub fn workflows_dir(&self) -> &PathBuf {
        &self.workflows_dir
    }

    pub fn app_db(&self) -> Arc<AppDb> {
        self.app_db.clone()
    }

    pub async fn get(&self, id: &str) -> Option<WorkflowDefinition> {
        self.core.get(id).await
    }

    /// List user-visible workflows only (excludes `dashboardHidden`).
    pub async fn list(&self) -> Vec<WorkflowListItem> {
        self.core.list().await
    }

    /// Definitions for scheduler: user-visible only (hidden are dashboard-driven).
    pub async fn list_definitions(&self) -> Vec<WorkflowDefinition> {
        self.core.list_definitions().await
    }

    pub async fn save_workflow(&self, workflow: &WorkflowDefinition) -> Result<(), String> {
        self.core.save(workflow).await
    }

    /// Save from raw YAML text (dual-mode editor). Parses, validates, persists.
    pub async fn save_workflow_yaml(&self, yaml: &str) -> Result<WorkflowDefinition, String> {
        self.core.save_yaml(yaml).await
    }

    pub async fn delete_workflow(&self, id: &str) -> Result<(), String> {
        self.core.delete(id).await
    }

    /// Reload is a no-op for DB backend (kept for IPC compatibility).
    pub async fn load_all(&self) -> Result<(), String> {
        self.core.load_all().await
    }

    pub async fn ensure_loaded(&self) -> Result<(), String> {
        self.core.ensure_loaded().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use datazen_feature_core::workflow::model::WorkflowStep;

    fn sample_def(id: &str, visibility: WorkflowVisibility) -> WorkflowDefinition {
        WorkflowDefinition {
            database: None,
            id: id.into(),
            name: format!("Name {id}"),
            description: "d".into(),
            version: None,
            blocks: Vec::new(),
            author: None,
            variables: vec![],
            connection: None,
            steps: vec![WorkflowStep::Query {
                id: "q1".into(),
                sql: "SELECT 1".into(),
                connection: None,
                database: None,
                timeout_secs: None,
                on_error: None,
            }],
            output: None,
            timeout_secs: None,
            error_handling: None,
            schedule: None,
            visibility,
        }
    }

    #[tokio::test]
    async fn list_excludes_dashboard_hidden() {
        let db = AppDb::open_in_memory().unwrap();
        let reg = WorkflowRegistry::new(db, PathBuf::from("/tmp/datazen-wf-test"));
        reg.save_workflow(&sample_def("user-1", WorkflowVisibility::User))
            .await
            .unwrap();
        reg.save_workflow(&sample_def("hidden-1", WorkflowVisibility::DashboardHidden))
            .await
            .unwrap();

        let list = reg.list().await;
        assert!(
            list.iter().any(|w| w.id == "user-1"),
            "user workflow should be listed"
        );
        assert!(
            list.iter().all(|w| w.id != "hidden-1"),
            "hidden workflow must not appear in list: {:?}",
            list.iter().map(|w| w.id.as_str()).collect::<Vec<_>>()
        );

        assert!(reg.get("hidden-1").await.is_some());
    }

    #[tokio::test]
    async fn save_yaml_roundtrip() {
        let db = AppDb::open_in_memory().unwrap();
        let reg = WorkflowRegistry::new(db, PathBuf::from("/tmp/datazen-wf-test2"));
        let def = sample_def("yaml-1", WorkflowVisibility::User);
        let yaml = serde_yaml::to_string(&def).unwrap();
        let saved = reg.save_workflow_yaml(&yaml).await.unwrap();
        assert_eq!(saved.id, "yaml-1");
        let got = reg.get("yaml-1").await.unwrap();
        assert_eq!(got.name, "Name yaml-1");
    }

    #[tokio::test]
    async fn delete_blocked_when_widget_refs() {
        use crate::store::{DashboardRecord, WidgetRecord};

        let db = AppDb::open_in_memory().unwrap();
        let reg = WorkflowRegistry::new(db.clone(), PathBuf::from("/tmp/datazen-wf-test3"));
        reg.save_workflow(&sample_def("wf-ref", WorkflowVisibility::User))
            .await
            .unwrap();

        let now = Utc::now().to_rfc3339();
        db.upsert_dashboard(&DashboardRecord {
            id: "d1".into(),
            name: "Dash".into(),
            created_at: now.clone(),
            updated_at: now.clone(),
            layout_cols: 12,
            layout_row_height: 80,
            enabled: true,
            refresh_paused: false,
        })
        .unwrap();
        db.upsert_widget(&WidgetRecord {
            id: "w1".into(),
            dashboard_id: "d1".into(),
            title: "Tile".into(),
            workflow_id: "wf-ref".into(),
            view_mode: "table".into(),
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
            created_at: now.clone(),
            updated_at: now,
        })
        .unwrap();

        let err = reg.delete_workflow("wf-ref").await.unwrap_err();
        assert!(err.contains("referenced"));
    }

    #[tokio::test]
    async fn seeds_builtins_when_empty() {
        let db = AppDb::open_in_memory().unwrap();
        let reg = WorkflowRegistry::new(db, PathBuf::from("/tmp/datazen-wf-seed"));
        let list = reg.list().await;
        assert!(
            list.len() >= 3,
            "expected builtin workflows, got {}",
            list.len()
        );
    }
}
