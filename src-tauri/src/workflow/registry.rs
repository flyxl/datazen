//! Workflow persistence backed by [`crate::store::AppDb`].

use super::model::{WorkflowDefinition, WorkflowListItem, WorkflowVisibility};
use crate::store::{AppDb, AppDbError, WorkflowRecord, WorkflowVisibility as DbVisibility};
use chrono::Utc;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct WorkflowRegistry {
    app_db: Arc<AppDb>,
    /// Legacy path kept for `open_workflows_dir` / diagnostics (may be empty).
    workflows_dir: PathBuf,
    seeded: AtomicBool,
    seed_lock: Mutex<()>,
}

impl WorkflowRegistry {
    pub fn new(app_db: Arc<AppDb>, data_dir: PathBuf) -> Self {
        Self {
            app_db,
            workflows_dir: data_dir.join("workflows"),
            seeded: AtomicBool::new(false),
            seed_lock: Mutex::new(()),
        }
    }

    pub fn workflows_dir(&self) -> &PathBuf {
        &self.workflows_dir
    }

    pub fn app_db(&self) -> Arc<AppDb> {
        self.app_db.clone()
    }

    async fn ensure_seeded(&self) -> Result<(), String> {
        if self.seeded.load(Ordering::Acquire) {
            return Ok(());
        }
        let _guard = self.seed_lock.lock().await;
        if self.seeded.load(Ordering::Acquire) {
            return Ok(());
        }
        self.seed_builtins_if_empty()?;
        self.seeded.store(true, Ordering::Release);
        Ok(())
    }

    fn seed_builtins_if_empty(&self) -> Result<(), String> {
        let existing = self
            .app_db
            .list_workflows(Some(DbVisibility::User))
            .map_err(|e| e.to_string())?;
        if !existing.is_empty() {
            return Ok(());
        }

        const BUILTINS: &[(&str, &str)] = &[
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

        for (label, content) in BUILTINS {
            let mut def: WorkflowDefinition = serde_yaml::from_str(content)
                .map_err(|e| format!("Failed to parse builtin {label}: {e}"))?;
            if def.visibility == WorkflowVisibility::DashboardHidden {
                def.visibility = WorkflowVisibility::User;
            }
            Self::validate_id(&def.id)?;
            self.persist_definition(&def)
                .map_err(|e| format!("Failed to seed builtin {label}: {e}"))?;
            tracing::info!("Seeded builtin workflow {} ({})", def.name, def.id);
        }
        Ok(())
    }

    pub fn validate_id(id: &str) -> Result<(), String> {
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(format!(
                "Invalid workflow id: {id}. Only alphanumeric, dash, and underscore are allowed."
            ));
        }
        Ok(())
    }

    fn to_db_visibility(v: WorkflowVisibility) -> DbVisibility {
        match v {
            WorkflowVisibility::User => DbVisibility::User,
            WorkflowVisibility::DashboardHidden => DbVisibility::DashboardHidden,
        }
    }

    fn from_db_visibility(v: DbVisibility) -> WorkflowVisibility {
        match v {
            DbVisibility::User => WorkflowVisibility::User,
            DbVisibility::DashboardHidden => WorkflowVisibility::DashboardHidden,
        }
    }

    fn record_to_definition(record: &WorkflowRecord) -> Result<WorkflowDefinition, String> {
        let mut def: WorkflowDefinition = serde_yaml::from_str(&record.definition_yaml)
            .map_err(|e| format!("Failed to parse workflow {}: {e}", record.id))?;
        def.id = record.id.clone();
        def.name = record.name.clone();
        def.description = record.description.clone();
        def.visibility = Self::from_db_visibility(record.visibility);
        Ok(def)
    }

    fn persist_definition(&self, workflow: &WorkflowDefinition) -> Result<(), String> {
        let now = Utc::now().to_rfc3339();
        let existing = self.app_db.get_workflow(&workflow.id).ok();
        let created_at = existing
            .as_ref()
            .map(|r| r.created_at.clone())
            .unwrap_or_else(|| now.clone());

        let definition_yaml = serde_yaml::to_string(workflow).map_err(|e| e.to_string())?;

        let record = WorkflowRecord {
            id: workflow.id.clone(),
            name: workflow.name.clone(),
            description: workflow.description.clone(),
            visibility: Self::to_db_visibility(workflow.visibility),
            definition_yaml,
            created_at,
            updated_at: now,
        };
        self.app_db
            .upsert_workflow(&record)
            .map_err(|e| e.to_string())
    }

    pub async fn get(&self, id: &str) -> Option<WorkflowDefinition> {
        if let Err(e) = self.ensure_seeded().await {
            tracing::warn!("Failed to seed workflows before get: {e}");
        }
        match self.app_db.get_workflow(id) {
            Ok(record) => match Self::record_to_definition(&record) {
                Ok(def) => Some(def),
                Err(e) => {
                    tracing::warn!("Failed to load workflow {id}: {e}");
                    None
                }
            },
            Err(AppDbError::NotFound(_)) => None,
            Err(e) => {
                tracing::warn!("Failed to get workflow {id}: {e}");
                None
            }
        }
    }

    /// List user-visible workflows only (excludes `dashboardHidden`).
    pub async fn list(&self) -> Vec<WorkflowListItem> {
        if let Err(e) = self.ensure_seeded().await {
            tracing::warn!("Failed to seed workflows before list: {e}");
            return Vec::new();
        }
        match self.app_db.list_workflows(Some(DbVisibility::User)) {
            Ok(rows) => rows
                .iter()
                .filter_map(|r| Self::record_to_definition(r).ok())
                .map(|s| WorkflowListItem {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    description: s.description.clone(),
                    variables: s.variables.clone(),
                    scheduled: s.schedule.as_ref().map(|sc| sc.enabled).unwrap_or(false),
                })
                .collect(),
            Err(e) => {
                tracing::warn!("Failed to list workflows: {e}");
                Vec::new()
            }
        }
    }

    /// Definitions for scheduler: user-visible only (hidden are dashboard-driven).
    pub async fn list_definitions(&self) -> Vec<WorkflowDefinition> {
        if let Err(e) = self.ensure_seeded().await {
            tracing::warn!("Failed to seed workflows before list_definitions: {e}");
            return Vec::new();
        }
        match self.app_db.list_workflows(Some(DbVisibility::User)) {
            Ok(rows) => rows
                .iter()
                .filter_map(|r| Self::record_to_definition(r).ok())
                .collect(),
            Err(e) => {
                tracing::warn!("Failed to list workflow definitions: {e}");
                Vec::new()
            }
        }
    }

    pub async fn save_workflow(&self, workflow: &WorkflowDefinition) -> Result<(), String> {
        Self::validate_id(&workflow.id)?;
        let _ = self.ensure_seeded().await;
        self.persist_definition(workflow)
    }

    /// Save from raw YAML text (dual-mode editor). Parses, validates, persists.
    pub async fn save_workflow_yaml(&self, yaml: &str) -> Result<WorkflowDefinition, String> {
        let workflow: WorkflowDefinition =
            serde_yaml::from_str(yaml).map_err(|e| format!("Invalid workflow YAML: {e}"))?;
        self.save_workflow(&workflow).await?;
        Ok(workflow)
    }

    pub async fn delete_workflow(&self, id: &str) -> Result<(), String> {
        Self::validate_id(id)?;
        let _ = self.ensure_seeded().await;
        match self.app_db.delete_workflow(id) {
            Ok(()) => Ok(()),
            Err(AppDbError::NotFound(_)) => Ok(()),
            Err(AppDbError::WorkflowInUse(summary)) => Err(format!(
                "Workflow is still referenced by dashboards: {summary}"
            )),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Reload is a no-op for DB backend (kept for IPC compatibility).
    pub async fn load_all(&self) -> Result<(), String> {
        self.ensure_seeded().await
    }

    pub async fn ensure_loaded(&self) -> Result<(), String> {
        self.ensure_seeded().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::model::WorkflowStep;

    fn sample_def(id: &str, visibility: WorkflowVisibility) -> WorkflowDefinition {
        WorkflowDefinition {
            id: id.into(),
            name: format!("Name {id}"),
            description: "d".into(),
            version: None,
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
