//! Workflow persistence backed by [`crate::store::AppDb`].
//!
//! The registry's rules — which workflows a user sees, how a definition is
//! encoded, what a saved workflow looks like — live in
//! `datazen_feature_core::workflow::registry`. This module only binds them to
//! the desktop store: the rows are handed over as a trait object, so the
//! shared crate never names the desktop persistence type.

use crate::platform::scope::is_local_scope;
use crate::store::{AppDb, AppDbError, WorkflowRecord, WorkflowVisibility as DbVisibility};
use datazen_feature_core::workflow::model::{
    WorkflowDefinition, WorkflowListItem, WorkflowVisibility,
};
use datazen_feature_core::workflow::registry::{
    WorkflowDefinitionRecord, WorkflowDefinitionStore, WorkflowRegistry as CoreRegistry,
};
use datazen_platform_api::ports::scope::OrganizationScope;
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
///
/// The host binds this to the local SQLite database and injects it into the
/// shared registry as a trait object, so the shared crate never names the
/// desktop persistence type.
pub(crate) struct AppDbWorkflowStore {
    app_db: Arc<AppDb>,
}

impl AppDbWorkflowStore {
    /// Bind the desktop rows to the local SQLite database.
    pub(crate) fn new(app_db: Arc<AppDb>) -> Self {
        Self { app_db }
    }
}

impl WorkflowDefinitionStore for AppDbWorkflowStore {
    fn get_workflow(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<Option<WorkflowDefinitionRecord>, String> {
        // 桌面是单租户：别的组织的工作流对本组织**不存在**（CM-05）。
        if !is_local_scope(scope) {
            return Ok(None);
        }
        match self.app_db.get_workflow(id) {
            Ok(record) => Ok(Some(record_to_definition_record(&record))),
            Err(AppDbError::NotFound(_)) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn list_workflows(
        &self,
        scope: &OrganizationScope,
        visibility: WorkflowVisibility,
    ) -> Result<Vec<WorkflowDefinitionRecord>, String> {
        if !is_local_scope(scope) {
            return Ok(Vec::new());
        }
        self.app_db
            .list_workflows(Some(to_db_visibility(visibility)))
            .map(|rows| rows.iter().map(record_to_definition_record).collect())
            .map_err(|error| error.to_string())
    }

    fn upsert_workflow(
        &self,
        scope: &OrganizationScope,
        record: &WorkflowDefinitionRecord,
    ) -> Result<(), String> {
        if !is_local_scope(scope) {
            return Err(format!("workflow {} not found", record.id));
        }
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

    fn delete_workflow(&self, scope: &OrganizationScope, id: &str) -> Result<(), String> {
        // 删除不存在的 id 不是错误：别的组织的工作流对本组织同样「不存在」。
        // 二者在本契约下不可区分 ⇒ 这里的 `Ok(())` 只说明「没有需要删除的东西」，
        // **不是**「目标已删除」。见 trait 的契约注释。
        if !is_local_scope(scope) {
            return Ok(());
        }
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

/// The desktop workflow registry: the shared rules over the desktop rows.
pub struct WorkflowRegistry {
    core: CoreRegistry,
    /// Legacy path kept for `open_workflows_dir` / diagnostics (may be empty).
    workflows_dir: PathBuf,
}

impl WorkflowRegistry {
    /// The desktop registry over an injected store.
    ///
    /// The store arrives as a trait object: the desktop rows stay behind it,
    /// and the shared registry never learns the desktop persistence type.
    pub fn new(store: Arc<dyn WorkflowDefinitionStore>, data_dir: PathBuf) -> Self {
        Self {
            core: CoreRegistry::new(store, BUILTIN_DEFINITIONS.to_vec()),
            workflows_dir: data_dir.join("workflows"),
        }
    }

    pub fn workflows_dir(&self) -> &PathBuf {
        &self.workflows_dir
    }

    pub async fn get(&self, scope: &OrganizationScope, id: &str) -> Option<WorkflowDefinition> {
        self.core.get(scope, id).await
    }

    /// List user-visible workflows only (excludes `dashboardHidden`).
    pub async fn list(&self, scope: &OrganizationScope) -> Vec<WorkflowListItem> {
        self.core.list(scope).await
    }

    /// Definitions for scheduler: user-visible only (hidden are dashboard-driven).
    pub async fn list_definitions(&self, scope: &OrganizationScope) -> Vec<WorkflowDefinition> {
        self.core.list_definitions(scope).await
    }

    pub async fn save_workflow(
        &self,
        scope: &OrganizationScope,
        workflow: &WorkflowDefinition,
    ) -> Result<(), String> {
        self.core.save(scope, workflow).await
    }

    /// Save from raw YAML text (dual-mode editor). Parses, validates, persists.
    pub async fn save_workflow_yaml(
        &self,
        scope: &OrganizationScope,
        yaml: &str,
    ) -> Result<WorkflowDefinition, String> {
        self.core.save_yaml(scope, yaml).await
    }

    pub async fn delete_workflow(&self, scope: &OrganizationScope, id: &str) -> Result<(), String> {
        self.core.delete(scope, id).await
    }

    /// Reload is a no-op for DB backend (kept for IPC compatibility).
    pub async fn load_all(&self, scope: &OrganizationScope) -> Result<(), String> {
        self.core.load_all(scope).await
    }

    pub async fn ensure_loaded(&self, scope: &OrganizationScope) -> Result<(), String> {
        self.core.ensure_loaded(scope).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use datazen_feature_core::workflow::model::WorkflowStep;
    use datazen_platform_api::id::OrganizationId;

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

    /// 桌面自己的组织作用域。
    fn local_scope() -> OrganizationScope {
        crate::platform::local_organization_scope()
    }

    /// 别的组织的作用域。
    fn foreign_scope() -> OrganizationScope {
        OrganizationScope::new(OrganizationId::new("another-organization"))
    }

    #[tokio::test]
    async fn list_excludes_dashboard_hidden() {
        let db = AppDb::open_in_memory().unwrap();
        let reg = WorkflowRegistry::new(
            Arc::new(AppDbWorkflowStore::new(db)),
            PathBuf::from("/tmp/datazen-wf-test"),
        );
        let scope = local_scope();
        reg.save_workflow(&scope, &sample_def("user-1", WorkflowVisibility::User))
            .await
            .unwrap();
        reg.save_workflow(
            &scope,
            &sample_def("hidden-1", WorkflowVisibility::DashboardHidden),
        )
        .await
        .unwrap();

        let list = reg.list(&scope).await;
        assert!(
            list.iter().any(|w| w.id == "user-1"),
            "user workflow should be listed"
        );
        assert!(
            list.iter().all(|w| w.id != "hidden-1"),
            "hidden workflow must not appear in list: {:?}",
            list.iter().map(|w| w.id.as_str()).collect::<Vec<_>>()
        );

        assert!(reg.get(&scope, "hidden-1").await.is_some());
    }

    #[tokio::test]
    async fn save_yaml_roundtrip() {
        let db = AppDb::open_in_memory().unwrap();
        let reg = WorkflowRegistry::new(
            Arc::new(AppDbWorkflowStore::new(db)),
            PathBuf::from("/tmp/datazen-wf-test2"),
        );
        let scope = local_scope();
        let def = sample_def("yaml-1", WorkflowVisibility::User);
        let yaml = serde_yaml::to_string(&def).unwrap();
        let saved = reg.save_workflow_yaml(&scope, &yaml).await.unwrap();
        assert_eq!(saved.id, "yaml-1");
        let got = reg.get(&scope, "yaml-1").await.unwrap();
        assert_eq!(got.name, "Name yaml-1");
    }

    #[tokio::test]
    async fn delete_blocked_when_widget_refs() {
        use crate::store::{DashboardRecord, WidgetRecord};

        let db = AppDb::open_in_memory().unwrap();
        let reg = WorkflowRegistry::new(
            Arc::new(AppDbWorkflowStore::new(db.clone())),
            PathBuf::from("/tmp/datazen-wf-test3"),
        );
        let scope = local_scope();
        reg.save_workflow(&scope, &sample_def("wf-ref", WorkflowVisibility::User))
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

        let err = reg.delete_workflow(&scope, "wf-ref").await.unwrap_err();
        assert!(err.contains("referenced"));
    }

    #[tokio::test]
    async fn seeds_builtins_when_empty() {
        let db = AppDb::open_in_memory().unwrap();
        let reg = WorkflowRegistry::new(
            Arc::new(AppDbWorkflowStore::new(db)),
            PathBuf::from("/tmp/datazen-wf-seed"),
        );
        let list = reg.list(&local_scope()).await;
        assert!(
            list.len() >= 3,
            "expected builtin workflows, got {}",
            list.len()
        );
    }

    /// 别的组织读不到本组织的工作流（CM-05：按「不存在」处理，不是「无权限」）。
    #[tokio::test]
    async fn the_workflows_of_another_organization_are_not_found() {
        let db = AppDb::open_in_memory().unwrap();
        let reg = WorkflowRegistry::new(
            Arc::new(AppDbWorkflowStore::new(db)),
            PathBuf::from("/tmp/datazen-wf-isolation"),
        );
        let scope = local_scope();
        let foreign = foreign_scope();
        reg.save_workflow(&scope, &sample_def("wf-1", WorkflowVisibility::User))
            .await
            .unwrap();

        assert!(reg.get(&foreign, "wf-1").await.is_none());
        assert!(reg
            .list(&foreign)
            .await
            .iter()
            .all(|item| item.id != "wf-1"));
    }

    /// 别的组织删不掉本组织的工作流。
    ///
    /// 注意这里的**语义歧义**：跨组织删除按「不存在」处理，而「删除不存在的 id
    /// 不是错误」是 `delete_workflow` 的既有契约 ⇒ 跨组织删除返回 `Ok(())`，
    /// 调用方拿到「成功」却什么都没删。数据没被删是对的，但「成功」的措辞会
    /// 让调用方误以为目标已删除。
    #[tokio::test]
    async fn a_workflow_of_another_organization_cannot_be_removed() {
        let db = AppDb::open_in_memory().unwrap();
        let reg = WorkflowRegistry::new(
            Arc::new(AppDbWorkflowStore::new(db)),
            PathBuf::from("/tmp/datazen-wf-isolation-delete"),
        );
        let scope = local_scope();
        reg.save_workflow(&scope, &sample_def("wf-1", WorkflowVisibility::User))
            .await
            .unwrap();

        // 跨组织删除返回 Ok（按「不存在」处理，不返回「无权限」）。
        let before = reg.list(&scope).await.len();
        reg.delete_workflow(&foreign_scope(), "wf-1").await.unwrap();
        // 但数据一个字节都没动。
        assert!(reg.get(&scope, "wf-1").await.is_some());
        assert_eq!(
            reg.list(&scope).await.len(),
            before,
            "跨组织删除不得影响本组织的工作流"
        );
    }
}
