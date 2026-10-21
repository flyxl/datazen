//! The workflow registry: what a saved workflow is, and the rules that decide
//! which saved workflows a user sees.
//!
//! The rules live here so every host agrees on them. The host owns only the
//! rows: it implements [`WorkflowDefinitionStore`] over its own persistence
//! and hands the store to [`WorkflowRegistry`].

use super::model::{WorkflowDefinition, WorkflowListItem, WorkflowVisibility};
use chrono::Utc;
use std::sync::atomic::{AtomicBool, Ordering};
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
/// looks like — stays in this crate.
pub trait WorkflowDefinitionStore: Send + Sync {
    /// One saved workflow, or `None` when the id is unknown.
    fn get_workflow(&self, id: &str) -> Result<Option<WorkflowDefinitionRecord>, String>;

    /// Every saved workflow with the given visibility.
    fn list_workflows(
        &self,
        visibility: WorkflowVisibility,
    ) -> Result<Vec<WorkflowDefinitionRecord>, String>;

    /// Insert or update one saved workflow.
    fn upsert_workflow(&self, record: &WorkflowDefinitionRecord) -> Result<(), String>;

    /// Delete one saved workflow. Deleting an unknown id is not an error.
    fn delete_workflow(&self, id: &str) -> Result<(), String>;
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
pub struct WorkflowRegistry<S: WorkflowDefinitionStore> {
    store: S,
    /// The builtin definitions the host ships, as `(label, yaml)`.
    builtins: Vec<(&'static str, &'static str)>,
    seeded: AtomicBool,
    seed_lock: Mutex<()>,
}

impl<S: WorkflowDefinitionStore> WorkflowRegistry<S> {
    /// A registry over `store`, seeding `builtins` when the environment has
    /// no user-visible workflow yet.
    pub fn new(store: S, builtins: Vec<(&'static str, &'static str)>) -> Self {
        Self {
            store,
            builtins,
            seeded: AtomicBool::new(false),
            seed_lock: Mutex::new(()),
        }
    }

    /// The store this registry reads and writes through.
    pub fn store(&self) -> &S {
        &self.store
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

    /// Seed the host's builtin workflows into an empty environment.
    ///
    /// A builtin that was authored for a dashboard is published as a user
    /// workflow instead: the dashboard owns its own copy, and the user list
    /// must not start empty.
    fn seed_builtins_if_empty(&self) -> Result<(), String> {
        let existing = self
            .store
            .list_workflows(WorkflowVisibility::User)
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
            self.persist_definition(&definition)
                .map_err(|error| format!("Failed to seed builtin {label}: {error}"))?;
            tracing::info!(
                "Seeded builtin workflow {} ({})",
                definition.name,
                definition.id
            );
        }
        Ok(())
    }

    fn persist_definition(&self, workflow: &WorkflowDefinition) -> Result<(), String> {
        let now = Utc::now().to_rfc3339();
        let created_at = self
            .store
            .get_workflow(&workflow.id)
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
        self.store.upsert_workflow(&record)
    }

    /// The definition of one saved workflow.
    pub async fn get(&self, id: &str) -> Option<WorkflowDefinition> {
        if let Err(error) = self.ensure_seeded().await {
            tracing::warn!("Failed to seed workflows before get: {error}");
        }
        let record = match self.store.get_workflow(id) {
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
    pub async fn list(&self) -> Vec<WorkflowListItem> {
        self.user_visible_definitions()
            .await
            .into_iter()
            .map(|definition| list_item_for(&definition))
            .collect()
    }

    /// The user-visible definitions — the scheduler runs these.
    pub async fn list_definitions(&self) -> Vec<WorkflowDefinition> {
        self.user_visible_definitions().await
    }

    async fn user_visible_definitions(&self) -> Vec<WorkflowDefinition> {
        if let Err(error) = self.ensure_seeded().await {
            tracing::warn!("Failed to seed workflows before list: {error}");
            return Vec::new();
        }
        let records = match self.store.list_workflows(WorkflowVisibility::User) {
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
    pub async fn save(&self, workflow: &WorkflowDefinition) -> Result<(), String> {
        validate_workflow_id(&workflow.id)?;
        let _ = self.ensure_seeded().await;
        self.persist_definition(workflow)
    }

    /// Save from raw YAML text (dual-mode editor). Parses, validates, persists.
    pub async fn save_yaml(&self, yaml: &str) -> Result<WorkflowDefinition, String> {
        let workflow = decode_definition(yaml)?;
        self.save(&workflow).await?;
        Ok(workflow)
    }

    /// Delete one saved workflow.
    pub async fn delete(&self, id: &str) -> Result<(), String> {
        validate_workflow_id(id)?;
        let _ = self.ensure_seeded().await;
        self.store.delete_workflow(id)
    }

    /// Reload is a no-op for a store-backed registry (kept for the host's IPC).
    pub async fn load_all(&self) -> Result<(), String> {
        self.ensure_seeded().await
    }

    /// Make sure the registry is ready to serve.
    pub async fn ensure_loaded(&self) -> Result<(), String> {
        self.ensure_seeded().await
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

    #[derive(Default)]
    struct MemoryStore {
        records: Mutex<Vec<WorkflowDefinitionRecord>>,
    }

    impl MemoryStore {
        fn records(&self) -> Vec<WorkflowDefinitionRecord> {
            self.records.lock().expect("store lock").clone()
        }
    }

    impl WorkflowDefinitionStore for MemoryStore {
        fn get_workflow(&self, id: &str) -> Result<Option<WorkflowDefinitionRecord>, String> {
            Ok(self.records().into_iter().find(|record| record.id == id))
        }

        fn list_workflows(
            &self,
            visibility: WorkflowVisibility,
        ) -> Result<Vec<WorkflowDefinitionRecord>, String> {
            Ok(self
                .records()
                .into_iter()
                .filter(|record| record.visibility == visibility)
                .collect())
        }

        fn upsert_workflow(&self, record: &WorkflowDefinitionRecord) -> Result<(), String> {
            let mut records = self.records.lock().expect("store lock");
            match records.iter_mut().find(|stored| stored.id == record.id) {
                Some(stored) => *stored = record.clone(),
                None => records.push(record.clone()),
            }
            Ok(())
        }

        fn delete_workflow(&self, id: &str) -> Result<(), String> {
            self.records
                .lock()
                .expect("store lock")
                .retain(|record| record.id != id);
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
        let registry = WorkflowRegistry::new(MemoryStore::default(), Vec::new());
        registry
            .save(&definition("user-1", WorkflowVisibility::User))
            .await
            .expect("save user workflow");
        registry
            .save(&definition("hidden-1", WorkflowVisibility::DashboardHidden))
            .await
            .expect("save hidden workflow");

        let listed = registry.list().await;
        assert!(listed.iter().any(|item| item.id == "user-1"));
        assert!(listed.iter().all(|item| item.id != "hidden-1"));
        assert!(registry.get("hidden-1").await.is_some());
    }

    #[tokio::test]
    async fn saving_keeps_the_creation_time_of_an_existing_workflow() {
        let registry = WorkflowRegistry::new(MemoryStore::default(), Vec::new());
        registry
            .save(&definition("kept", WorkflowVisibility::User))
            .await
            .expect("save");
        let created_at = registry
            .store()
            .get_workflow("kept")
            .expect("read")
            .expect("record")
            .created_at;

        registry
            .save(&definition("kept", WorkflowVisibility::User))
            .await
            .expect("save again");
        let record = registry
            .store()
            .get_workflow("kept")
            .expect("read")
            .expect("record");
        assert_eq!(record.created_at, created_at);
    }

    #[tokio::test]
    async fn the_builtin_workflows_are_seeded_once() {
        let yaml = "id: builtin\nname: Builtin\ndescription: d\nsteps: []\n";
        let registry = WorkflowRegistry::new(MemoryStore::default(), vec![("builtin", yaml)]);
        registry.list().await;
        assert_eq!(registry.store().records().len(), 1);

        // A second call must not seed again.
        registry.list().await;
        assert_eq!(registry.store().records().len(), 1);
    }

    #[tokio::test]
    async fn a_dashboard_builtin_is_published_as_a_user_workflow() {
        let yaml =
            "id: builtin\nname: Builtin\ndescription: d\nsteps: []\nvisibility: dashboardHidden\n";
        let registry = WorkflowRegistry::new(MemoryStore::default(), vec![("builtin", yaml)]);
        registry.list().await;
        let records = registry.store().records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].visibility, WorkflowVisibility::User);
    }

    #[tokio::test]
    async fn yaml_round_trips_through_the_registry() {
        let registry = WorkflowRegistry::new(MemoryStore::default(), Vec::new());
        let definition = definition("yaml-1", WorkflowVisibility::User);
        let yaml = encode_definition(&definition).expect("encode");
        let saved = registry.save_yaml(&yaml).await.expect("save yaml");
        assert_eq!(saved.id, "yaml-1");
        assert_eq!(
            registry.get("yaml-1").await.expect("get").name,
            "Name yaml-1"
        );
    }

    #[tokio::test]
    async fn invalid_yaml_names_the_problem() {
        let registry = WorkflowRegistry::new(MemoryStore::default(), Vec::new());
        let error = registry.save_yaml("not: [valid").await.unwrap_err();
        assert!(error.starts_with("Invalid workflow YAML:"));
    }
}
