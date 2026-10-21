//! The workflow registry: what a saved workflow is, and the rules that decide
//! which saved workflows a user sees.
//!
//! The rules live here so every host agrees on them. The host owns only the
//! rows: it implements [`WorkflowDefinitionStore`] over its own persistence
//! and hands the store to [`WorkflowRegistry`] as an
//! `Arc<dyn WorkflowDefinitionStore>`, so the host's persistence type never
//! enters this crate's signatures.

use super::model::{WorkflowDefinition, WorkflowListItem, WorkflowVisibility};
use chrono::Utc;
use datazen_platform_api::id::OrganizationId;
use datazen_platform_api::ports::scope::OrganizationScope;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use tokio::sync::Mutex;

/// A saved workflow as the registry stores it.
///
/// The definition itself travels as YAML so a saved workflow stays the same
/// document the user wrote in the editor.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDefinitionRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub visibility: WorkflowVisibility,
    pub definition_yaml: String,
    pub created_at: String,
    pub updated_at: String,
}

/// The persistence a workflow registry needs from the environment it runs in.
///
/// The host implements this over its own store (the desktop host binds it to
/// the local SQLite database). Everything the registry *decides* — which
/// workflows a user sees, how a definition is encoded, what a saved workflow
/// looks like — stays in this crate. The registry holds the store as a trait
/// object, so the host's implementation type stays out of the shared
/// signatures and the host picks the implementation at assembly time.
pub trait WorkflowDefinitionStore: Send + Sync {
    /// One saved workflow of the organization, or `None` when the id is unknown.
    fn get_workflow(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<Option<WorkflowDefinitionRecord>, String>;

    /// Every saved workflow of the organization with the given visibility.
    fn list_workflows(
        &self,
        scope: &OrganizationScope,
        visibility: WorkflowVisibility,
    ) -> Result<Vec<WorkflowDefinitionRecord>, String>;

    /// Insert or update one saved workflow.
    fn upsert_workflow(
        &self,
        scope: &OrganizationScope,
        record: &WorkflowDefinitionRecord,
    ) -> Result<(), String>;

    /// Delete one saved workflow. Deleting an unknown id is not an error.
    ///
    /// **跨组织删除与删除一个不存在的 id 一样返回 `Ok(())`**：在本契约下二者
    /// 不可区分——越组织一律按「不存在」处理（CM-05），而「删除不存在的 id 不是
    /// 错误」是既有约定。因此调用方**不应**把这个 `Ok(())` 读作「目标已删除」，
    /// 它只说明「没有需要删除的东西」。
    fn delete_workflow(&self, scope: &OrganizationScope, id: &str) -> Result<(), String>;
}

/// A workflow id may only contain characters that survive a URL, a file name
/// and a SQL parameter without quoting.
pub fn validate_workflow_id(id: &str) -> Result<(), String> {
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

/// Encode a definition as the YAML the registry stores.
pub fn encode_definition(definition: &WorkflowDefinition) -> Result<String, String> {
    serde_yaml::to_string(definition).map_err(|error| error.to_string())
}

/// Decode a definition from the YAML a user wrote.
pub fn decode_definition(yaml: &str) -> Result<WorkflowDefinition, String> {
    serde_yaml::from_str(yaml).map_err(|error| format!("Invalid workflow YAML: {error}"))
}

/// The list projection of one definition: what the workflow list shows.
pub fn list_item_for(definition: &WorkflowDefinition) -> WorkflowListItem {
    WorkflowListItem {
        id: definition.id.clone(),
        name: definition.name.clone(),
        description: definition.description.clone(),
        variables: definition.variables.clone(),
        scheduled: definition
            .schedule
            .as_ref()
            .map(|schedule| schedule.enabled)
            .unwrap_or(false),
    }
}

/// The saved workflows of one environment.
///
/// The registry owns the rules; the store owns the rows.
///
/// The store is held as a trait object on purpose: the registry must not
/// name the host's persistence type, so a host can bind any
/// [`WorkflowDefinitionStore`] implementation without the shared crate's
/// signatures changing.
pub struct WorkflowRegistry {
    store: Arc<dyn WorkflowDefinitionStore>,
    /// The builtin definitions the host ships, as `(label, yaml)`.
    builtins: Vec<(&'static str, &'static str)>,
    /// 已经播种过的组织。
    ///
    /// 播种是**按组织**的：多租户宿主里每个组织都要拿到自己的内置 workflow，
    /// 全局一个标志会让第二个组织永远拿不到自己的那一份。
    seeded: StdMutex<HashSet<OrganizationId>>,
    seed_lock: Mutex<()>,
}

impl WorkflowRegistry {
    /// A registry over `store`, seeding `builtins` when the environment has
    /// no user-visible workflow yet.
    pub fn new(
        store: Arc<dyn WorkflowDefinitionStore>,
        builtins: Vec<(&'static str, &'static str)>,
    ) -> Self {
        Self {
            store,
            builtins,
            seeded: StdMutex::new(HashSet::new()),
            seed_lock: Mutex::new(()),
        }
    }

    /// The store this registry reads and writes through.
    pub fn store(&self) -> &dyn WorkflowDefinitionStore {
        self.store.as_ref()
    }

    async fn ensure_seeded(&self, scope: &OrganizationScope) -> Result<(), String> {
        if self.is_seeded(scope) {
            return Ok(());
        }
        let _guard = self.seed_lock.lock().await;
        if self.is_seeded(scope) {
            return Ok(());
        }
        self.seed_builtins_if_empty(scope)?;
        self.mark_seeded(scope);
        Ok(())
    }

    /// 这个组织是否已经播种过内置 workflow。
    fn is_seeded(&self, scope: &OrganizationScope) -> bool {
        self.seeded
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .contains(scope.organization_id())
    }

    /// 记下这个组织已经播种过。
    fn mark_seeded(&self, scope: &OrganizationScope) {
        self.seeded
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(scope.organization_id().clone());
    }

    /// Seed the host's builtin workflows into an empty environment.
    ///
    /// A builtin that was authored for a dashboard is published as a user
    /// workflow instead: the dashboard owns its own copy, and the user list
    /// must not start empty.
    fn seed_builtins_if_empty(&self, scope: &OrganizationScope) -> Result<(), String> {
        let existing = self
            .store
            .list_workflows(scope, WorkflowVisibility::User)
            .map_err(|error| error.to_string())?;
        if !existing.is_empty() {
            return Ok(());
        }

        for (label, content) in &self.builtins {
            let mut definition: WorkflowDefinition = serde_yaml::from_str(content)
                .map_err(|error| format!("Failed to parse builtin {label}: {error}"))?;
            if definition.visibility == WorkflowVisibility::DashboardHidden {
                definition.visibility = WorkflowVisibility::User;
            }
            validate_workflow_id(&definition.id)?;
            self.persist_definition(scope, &definition)
                .map_err(|error| format!("Failed to seed builtin {label}: {error}"))?;
            tracing::info!(
                "Seeded builtin workflow {} ({})",
                definition.name,
                definition.id
            );
        }
        Ok(())
    }

    fn persist_definition(
        &self,
        scope: &OrganizationScope,
        workflow: &WorkflowDefinition,
    ) -> Result<(), String> {
        let now = Utc::now().to_rfc3339();
        let created_at = self
            .store
            .get_workflow(scope, &workflow.id)
            .ok()
            .flatten()
            .map(|record| record.created_at)
            .unwrap_or_else(|| now.clone());

        let record = WorkflowDefinitionRecord {
            id: workflow.id.clone(),
            name: workflow.name.clone(),
            description: workflow.description.clone(),
            visibility: workflow.visibility,
            definition_yaml: encode_definition(workflow)?,
            created_at,
            updated_at: now,
        };
        self.store.upsert_workflow(scope, &record)
    }

    /// The definition of one saved workflow.
    pub async fn get(&self, scope: &OrganizationScope, id: &str) -> Option<WorkflowDefinition> {
        if let Err(error) = self.ensure_seeded(scope).await {
            tracing::warn!("Failed to seed workflows before get: {error}");
        }
        let record = match self.store.get_workflow(scope, id) {
            Ok(record) => record?,
            Err(error) => {
                tracing::warn!("Failed to get workflow {id}: {error}");
                return None;
            }
        };
        match decode_record(&record) {
            Ok(definition) => Some(definition),
            Err(error) => {
                tracing::warn!("Failed to load workflow {id}: {error}");
                None
            }
        }
    }

    /// The user-visible workflows, as the workflow list shows them.
    pub async fn list(&self, scope: &OrganizationScope) -> Vec<WorkflowListItem> {
        self.user_visible_definitions(scope)
            .await
            .into_iter()
            .map(|definition| list_item_for(&definition))
            .collect()
    }

    /// The user-visible definitions — the scheduler runs these.
    pub async fn list_definitions(&self, scope: &OrganizationScope) -> Vec<WorkflowDefinition> {
        self.user_visible_definitions(scope).await
    }

    async fn user_visible_definitions(&self, scope: &OrganizationScope) -> Vec<WorkflowDefinition> {
        if let Err(error) = self.ensure_seeded(scope).await {
            tracing::warn!("Failed to seed workflows before list: {error}");
            return Vec::new();
        }
        let records = match self.store.list_workflows(scope, WorkflowVisibility::User) {
            Ok(records) => records,
            Err(error) => {
                tracing::warn!("Failed to list workflows: {error}");
                return Vec::new();
            }
        };
        records
            .iter()
            .filter_map(|record| match decode_record(record) {
                Ok(definition) => Some(definition),
                Err(error) => {
                    tracing::warn!("Failed to list workflow definitions: {error}");
                    None
                }
            })
            .collect()
    }

    /// Save one definition.
    pub async fn save(
        &self,
        scope: &OrganizationScope,
        workflow: &WorkflowDefinition,
    ) -> Result<(), String> {
        validate_workflow_id(&workflow.id)?;
        let _ = self.ensure_seeded(scope).await;
        self.persist_definition(scope, workflow)
    }

    /// Save from raw YAML text (dual-mode editor). Parses, validates, persists.
    pub async fn save_yaml(
        &self,
        scope: &OrganizationScope,
        yaml: &str,
    ) -> Result<WorkflowDefinition, String> {
        let workflow = decode_definition(yaml)?;
        self.save(scope, &workflow).await?;
        Ok(workflow)
    }

    /// Delete one saved workflow.
    ///
    /// 跨组织删除与删除一个不存在的 id 一样返回 `Ok(())`（见 trait 的契约注释）：
    /// 调用方**不应**把它读作「目标已删除」。
    pub async fn delete(&self, scope: &OrganizationScope, id: &str) -> Result<(), String> {
        validate_workflow_id(id)?;
        let _ = self.ensure_seeded(scope).await;
        self.store.delete_workflow(scope, id)
    }

    /// Reload is a no-op for a store-backed registry (kept for the host's IPC).
    pub async fn load_all(&self, scope: &OrganizationScope) -> Result<(), String> {
        self.ensure_seeded(scope).await
    }

    /// Make sure the registry is ready to serve.
    pub async fn ensure_loaded(&self, scope: &OrganizationScope) -> Result<(), String> {
        self.ensure_seeded(scope).await
    }
}

/// A stored record becomes a definition: the stored columns win over the
/// document, so a renamed workflow keeps its name even if an old definition
/// still carries the previous one.
fn decode_record(record: &WorkflowDefinitionRecord) -> Result<WorkflowDefinition, String> {
    let mut definition: WorkflowDefinition = serde_yaml::from_str(&record.definition_yaml)
        .map_err(|error| format!("Failed to parse workflow {}: {error}", record.id))?;
    definition.id = record.id.clone();
    definition.name = record.name.clone();
    definition.description = record.description.clone();
    definition.visibility = record.visibility;
    Ok(definition)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::model::WorkflowSchedule;
    use std::sync::Mutex;

    /// 一个**按组织分区**的内存存储：多个组织的 workflow 并存于同一份数据。
    #[derive(Default)]
    struct MemoryStore {
        records: Mutex<Vec<(OrganizationId, WorkflowDefinitionRecord)>>,
    }

    impl MemoryStore {
        fn records(&self) -> Vec<(OrganizationId, WorkflowDefinitionRecord)> {
            self.records.lock().expect("store lock").clone()
        }
    }

    /// 某个组织的作用域。
    fn scope_of(organization: &str) -> OrganizationScope {
        OrganizationScope::new(OrganizationId::new(organization))
    }

    impl WorkflowDefinitionStore for MemoryStore {
        fn get_workflow(
            &self,
            scope: &OrganizationScope,
            id: &str,
        ) -> Result<Option<WorkflowDefinitionRecord>, String> {
            Ok(self
                .records()
                .into_iter()
                .find(|(owner, record)| owner == scope.organization_id() && record.id == id)
                .map(|(_, record)| record))
        }

        fn list_workflows(
            &self,
            scope: &OrganizationScope,
            visibility: WorkflowVisibility,
        ) -> Result<Vec<WorkflowDefinitionRecord>, String> {
            Ok(self
                .records()
                .into_iter()
                .filter(|(owner, record)| {
                    owner == scope.organization_id() && record.visibility == visibility
                })
                .map(|(_, record)| record)
                .collect())
        }

        fn upsert_workflow(
            &self,
            scope: &OrganizationScope,
            record: &WorkflowDefinitionRecord,
        ) -> Result<(), String> {
            let mut records = self.records.lock().expect("store lock");
            match records
                .iter_mut()
                .find(|(owner, stored)| owner == scope.organization_id() && stored.id == record.id)
            {
                Some((_, stored)) => *stored = record.clone(),
                None => records.push((scope.organization_id().clone(), record.clone())),
            }
            Ok(())
        }

        fn delete_workflow(&self, scope: &OrganizationScope, id: &str) -> Result<(), String> {
            let mut records = self.records.lock().expect("store lock");
            records
                .retain(|(owner, record)| !(owner == scope.organization_id() && record.id == id));
            Ok(())
        }
    }

    fn definition(id: &str, visibility: WorkflowVisibility) -> WorkflowDefinition {
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
            steps: vec![],
            output: None,
            timeout_secs: None,
            error_handling: None,
            schedule: None,
            visibility,
        }
    }

    #[test]
    fn ids_may_only_carry_url_safe_characters() {
        assert!(validate_workflow_id("workflow-1_2").is_ok());
        assert!(validate_workflow_id("").is_err());
        assert!(validate_workflow_id("workflow 1").is_err());
        assert!(validate_workflow_id("workflow/1").is_err());
        assert!(validate_workflow_id("工作流").is_err());
    }

    #[test]
    fn the_list_projection_reports_the_schedule_flag() {
        let mut definition = definition("scheduled", WorkflowVisibility::User);
        definition.schedule = Some(WorkflowSchedule {
            enabled: true,
            interval_secs: Some(60),
        });
        assert!(list_item_for(&definition).scheduled);

        definition.schedule = Some(WorkflowSchedule {
            enabled: false,
            interval_secs: Some(60),
        });
        assert!(!list_item_for(&definition).scheduled);
    }

    #[tokio::test]
    async fn the_list_excludes_dashboard_hidden_workflows() {
        let scope = scope_of("org-a");
        let registry = WorkflowRegistry::new(Arc::new(MemoryStore::default()), Vec::new());
        registry
            .save(&scope, &definition("user-1", WorkflowVisibility::User))
            .await
            .expect("save user workflow");
        registry
            .save(
                &scope,
                &definition("hidden-1", WorkflowVisibility::DashboardHidden),
            )
            .await
            .expect("save hidden workflow");

        let listed = registry.list(&scope).await;
        assert!(listed.iter().any(|item| item.id == "user-1"));
        assert!(listed.iter().all(|item| item.id != "hidden-1"));
        assert!(registry.get(&scope, "hidden-1").await.is_some());
    }

    #[tokio::test]
    async fn saving_keeps_the_creation_time_of_an_existing_workflow() {
        let scope = scope_of("org-a");
        let registry = WorkflowRegistry::new(Arc::new(MemoryStore::default()), Vec::new());
        registry
            .save(&scope, &definition("kept", WorkflowVisibility::User))
            .await
            .expect("save");
        let created_at = registry
            .store()
            .get_workflow(&scope, "kept")
            .expect("read")
            .expect("record")
            .created_at;

        registry
            .save(&scope, &definition("kept", WorkflowVisibility::User))
            .await
            .expect("save again");
        let record = registry
            .store()
            .get_workflow(&scope, "kept")
            .expect("read")
            .expect("record");
        assert_eq!(record.created_at, created_at);
    }

    #[tokio::test]
    async fn the_builtin_workflows_are_seeded_once() {
        let scope = scope_of("org-a");
        let yaml = "id: builtin\nname: Builtin\ndescription: d\nsteps: []\n";
        let store = Arc::new(MemoryStore::default());
        let registry = WorkflowRegistry::new(store.clone(), vec![("builtin", yaml)]);
        registry.list(&scope).await;
        assert_eq!(store.records().len(), 1);

        // A second call must not seed again.
        registry.list(&scope).await;
        assert_eq!(store.records().len(), 1);
    }

    #[tokio::test]
    async fn a_dashboard_builtin_is_published_as_a_user_workflow() {
        let scope = scope_of("org-a");
        let yaml =
            "id: builtin\nname: Builtin\ndescription: d\nsteps: []\nvisibility: dashboardHidden\n";
        let store = Arc::new(MemoryStore::default());
        let registry = WorkflowRegistry::new(store.clone(), vec![("builtin", yaml)]);
        registry.list(&scope).await;
        let records = store.records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].1.visibility, WorkflowVisibility::User);
    }

    #[tokio::test]
    async fn yaml_round_trips_through_the_registry() {
        let scope = scope_of("org-a");
        let registry = WorkflowRegistry::new(Arc::new(MemoryStore::default()), Vec::new());
        let definition = definition("yaml-1", WorkflowVisibility::User);
        let yaml = encode_definition(&definition).expect("encode");
        let saved = registry.save_yaml(&scope, &yaml).await.expect("save yaml");
        assert_eq!(saved.id, "yaml-1");
        assert_eq!(
            registry.get(&scope, "yaml-1").await.expect("get").name,
            "Name yaml-1"
        );
    }

    #[tokio::test]
    async fn invalid_yaml_names_the_problem() {
        let scope = scope_of("org-a");
        let registry = WorkflowRegistry::new(Arc::new(MemoryStore::default()), Vec::new());
        let error = registry.save_yaml(&scope, "not: [valid").await.unwrap_err();
        assert!(error.starts_with("Invalid workflow YAML:"));
    }

    // ─── 组织隔离 ───

    /// 别的组织的 workflow 读不到：单条 `None`、列表为空（CM-05）。
    #[tokio::test]
    async fn the_workflows_of_another_organization_are_not_found() {
        let owner = scope_of("org-a");
        let other = scope_of("org-b");
        let registry = WorkflowRegistry::new(Arc::new(MemoryStore::default()), Vec::new());
        registry
            .save(&owner, &definition("wf-1", WorkflowVisibility::User))
            .await
            .expect("save");

        assert!(registry.get(&other, "wf-1").await.is_none());
        assert!(registry.list(&other).await.is_empty());
    }

    /// 两个组织各自看到自己的 workflow。
    #[tokio::test]
    async fn two_organizations_keep_their_own_workflows_apart() {
        let a = scope_of("org-a");
        let b = scope_of("org-b");
        let registry = WorkflowRegistry::new(Arc::new(MemoryStore::default()), Vec::new());
        registry
            .save(&a, &definition("shared-id", WorkflowVisibility::User))
            .await
            .expect("save a");
        registry
            .save(&b, &definition("shared-id", WorkflowVisibility::User))
            .await
            .expect("save b");

        assert_eq!(registry.list(&a).await.len(), 1);
        assert_eq!(registry.list(&b).await.len(), 1);
    }

    /// 别的组织删不掉本组织的 workflow。
    #[tokio::test]
    async fn a_workflow_of_another_organization_cannot_be_removed() {
        let owner = scope_of("org-a");
        let other = scope_of("org-b");
        let registry = WorkflowRegistry::new(Arc::new(MemoryStore::default()), Vec::new());
        registry
            .save(&owner, &definition("wf-1", WorkflowVisibility::User))
            .await
            .expect("save");

        registry.delete(&other, "wf-1").await.expect("delete");
        assert!(registry.get(&owner, "wf-1").await.is_some());
    }

    /// 内置 workflow **按组织**播种：第二个组织也能拿到自己的一份。
    #[tokio::test]
    async fn the_builtin_workflows_are_seeded_per_organization() {
        let a = scope_of("org-a");
        let b = scope_of("org-b");
        let yaml = "id: builtin\nname: Builtin\ndescription: d\nsteps: []\n";
        let store = Arc::new(MemoryStore::default());
        let registry = WorkflowRegistry::new(store.clone(), vec![("builtin", yaml)]);

        registry.list(&a).await;
        assert_eq!(store.records().len(), 1);

        registry.list(&b).await;
        assert_eq!(store.records().len(), 2);
        assert_eq!(registry.list(&b).await.len(), 1);
    }
}
