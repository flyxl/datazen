//! Workflow persistence and in-memory registry.

use super::model::{WorkflowDefinition, WorkflowListItem};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{Mutex, RwLock};

pub struct WorkflowRegistry {
    workflows: RwLock<HashMap<String, WorkflowDefinition>>,
    workflows_dir: PathBuf,
    loaded: AtomicBool,
    load_lock: Mutex<()>,
}

impl WorkflowRegistry {
    pub fn new(workflows_dir: PathBuf) -> Self {
        Self {
            workflows: RwLock::new(HashMap::new()),
            workflows_dir,
            loaded: AtomicBool::new(false),
            load_lock: Mutex::new(()),
        }
    }

    pub fn workflows_dir(&self) -> &PathBuf {
        &self.workflows_dir
    }

    pub async fn ensure_loaded(&self) -> Result<(), String> {
        if self.loaded.load(Ordering::Acquire) {
            return Ok(());
        }
        let _guard = self.load_lock.lock().await;
        if self.loaded.load(Ordering::Acquire) {
            return Ok(());
        }
        self.load_all_unlocked().await?;
        self.loaded.store(true, Ordering::Release);
        Ok(())
    }

    pub async fn load_all(&self) -> Result<(), String> {
        let _guard = self.load_lock.lock().await;
        self.load_all_unlocked().await?;
        self.loaded.store(true, Ordering::Release);
        Ok(())
    }

    async fn load_all_unlocked(&self) -> Result<(), String> {
        if !self.workflows_dir.exists() {
            std::fs::create_dir_all(&self.workflows_dir).map_err(|e| e.to_string())?;
        }
        Self::seed_builtins_if_empty(&self.workflows_dir)?;

        let mut workflows = self.workflows.write().await;
        workflows.clear();

        let entries = std::fs::read_dir(&self.workflows_dir).map_err(|e| e.to_string())?;

        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();

            if path
                .extension()
                .map_or(false, |ext| ext == "yaml" || ext == "yml")
            {
                match Self::load_workflow_file(&path) {
                    Ok(workflow) => {
                        tracing::info!("Loaded workflow: {} ({})", workflow.name, workflow.id);
                        workflows.insert(workflow.id.clone(), workflow);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load workflow {:?}: {}", path, e);
                    }
                }
            }
        }

        tracing::info!("Loaded {} workflows", workflows.len());
        Ok(())
    }

    fn seed_builtins_if_empty(dir: &Path) -> Result<(), String> {
        let has_yaml = std::fs::read_dir(dir)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .any(|e| {
                e.path()
                    .extension()
                    .map_or(false, |ext| ext == "yaml" || ext == "yml")
            });
        if has_yaml {
            return Ok(());
        }

        const BUILTINS: &[(&str, &str)] = &[
            (
                "builtin-hello-query.yaml",
                include_str!("../../resources/builtin-workflows/hello-query.yaml"),
            ),
            (
                "builtin-cross-db-sample.yaml",
                include_str!("../../resources/builtin-workflows/cross-db-sample.yaml"),
            ),
            (
                "builtin-ai-summarize.yaml",
                include_str!("../../resources/builtin-workflows/ai-summarize.yaml"),
            ),
        ];
        for (name, content) in BUILTINS {
            let path = dir.join(name);
            std::fs::write(&path, content)
                .map_err(|e| format!("Failed to seed builtin workflow {name}: {e}"))?;
            tracing::info!("Seeded builtin workflow {}", name);
        }
        Ok(())
    }

    fn load_workflow_file(path: &Path) -> Result<WorkflowDefinition, String> {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read {path:?}: {e}"))?;
        serde_yaml::from_str::<WorkflowDefinition>(&content)
            .map_err(|e| format!("Failed to parse {path:?}: {e}"))
    }

    pub async fn get(&self, id: &str) -> Option<WorkflowDefinition> {
        if let Err(e) = self.ensure_loaded().await {
            tracing::warn!("Failed to load workflows before get: {e}");
            return None;
        }
        self.workflows.read().await.get(id).cloned()
    }

    pub async fn list(&self) -> Vec<WorkflowListItem> {
        if let Err(e) = self.ensure_loaded().await {
            tracing::warn!("Failed to load workflows before list: {e}");
            return Vec::new();
        }
        self.workflows
            .read()
            .await
            .values()
            .map(|s| WorkflowListItem {
                id: s.id.clone(),
                name: s.name.clone(),
                description: s.description.clone(),
                variables: s.variables.clone(),
            })
            .collect()
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

    pub async fn save_workflow(&self, workflow: &WorkflowDefinition) -> Result<(), String> {
        Self::validate_id(&workflow.id)?;
        let yaml = serde_yaml::to_string(workflow).map_err(|e| e.to_string())?;
        let path = self.workflows_dir.join(format!("{}.yaml", workflow.id));
        std::fs::write(&path, yaml).map_err(|e| e.to_string())?;
        self.workflows
            .write()
            .await
            .insert(workflow.id.clone(), workflow.clone());
        Ok(())
    }

    pub async fn delete_workflow(&self, id: &str) -> Result<(), String> {
        Self::validate_id(id)?;
        let path = self.workflows_dir.join(format!("{id}.yaml"));
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        self.workflows.write().await.remove(id);
        Ok(())
    }
}
