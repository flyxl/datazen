//! The host's workflows and dashboards (N13): two JSON files under the data
//! directory, written atomically. The workflow definitions go through the
//! registry use case (`datazen::workflow::application::registry`), the
//! dashboard rows through the dashboard domain rules
//! (`datazen::dashboard::domain::repository`) — the stores only move rows;
//! every rule that decides what a saved workflow or a dashboard contains
//! stays in the use cases, never here.
//!
//! The native host is single-tenant: one organization (`desktop`), one pair
//! of files. The repository contracts carry an [`OrganizationScope`] for
//! multi-tenant hosts; this store ignores it — the adapter only ever passes
//! the one scope.

use async_trait::async_trait;
use datazen::dashboard::application::create::WidgetCreationHost;
use datazen::dashboard::domain::records::{DashboardRecord, WidgetRecord};
use datazen::dashboard::domain::repository::{
    DashboardRepository, DashboardRepositoryError,
};
use datazen::dashboard::domain::repository as dashboard_rules;
use datazen::dashboard::domain::types::{
    Dashboard, DashboardLayout, DashboardWorkflowRef, ViewMode,
};
use datazen::shared::id::OrganizationId;
use datazen::shared::scope::OrganizationScope;
use datazen::workflow::application::registry::WorkflowRegistry;
use datazen::workflow::domain::model::{
    WorkflowDefinition, WorkflowStep, WorkflowVisibility,
};
use datazen::workflow::domain::repository::{
    WorkflowDefinitionRecord, WorkflowDefinitionRepository,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The one dashboard a native install has. The host owns rows; the first
/// widget's creation brings the row into existence.
pub(crate) const DEFAULT_DASHBOARD_ID: &str = "default";

/// The one organization a desktop process recognizes.
pub(crate) fn host_scope() -> OrganizationScope {
    OrganizationScope::new(OrganizationId::new(datazen_ui_api::DESKTOP_ORGANIZATION))
}

/// The UI's widget ids (N13): the first 16 hex characters of the widget's
/// UUID — a stable `u64` for the same widget across restarts, the shape the
/// UI's port speaks.
pub(crate) fn widget_ui_id(id: &str) -> u64 {
    id.replace('-', "")
        .get(..16)
        .and_then(|hex| u64::from_str_radix(hex, 16).ok())
        .unwrap_or(0)
}

/// A workflow id the registry accepts (alphanumeric, dash, underscore): the
/// name's slug, or a hash when the name has no slug characters. A slug
/// another workflow already owns gets the hash suffix — ids stay unique,
/// names decide identity.
pub(crate) fn workflow_id_for(name: &str, taken: &[String]) -> String {
    let slug: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let base = slug.trim_matches('-').to_owned();
    let mut id = if base.is_empty() {
        format!("wf-{:x}", fnv1a(name))
    } else {
        base
    };
    if taken.iter().any(|taken| taken == &id) {
        id = format!("{id}-{:x}", fnv1a(name));
    }
    id
}

/// FNV-1a: a stable hash with no dependency.
fn fnv1a(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// One step as the UI's workflow editor shows it (N13): a query step is its
/// SQL; every other kind names itself.
pub(crate) fn step_text(step: &WorkflowStep) -> String {
    match step {
        WorkflowStep::Query { sql, .. } => sql.clone(),
        WorkflowStep::Command { command, .. } => format!("[command] {command}"),
        WorkflowStep::Ai { .. } => "[ai step]".into(),
        WorkflowStep::Condition { .. } => "[condition]".into(),
        WorkflowStep::ForEach { .. } => "[foreach]".into(),
        WorkflowStep::Merge { .. } => "[merge]".into(),
        WorkflowStep::Transform { .. } => "[transform]".into(),
    }
}

/// The workflow store: a JSON array of definition records, written
/// atomically. The registry use case reads and writes through the
/// repository contract; this side only moves rows.
pub struct FileWorkflowStore {
    path: PathBuf,
    state: Mutex<Vec<WorkflowDefinitionRecord>>,
}

impl FileWorkflowStore {
    pub fn open(path: &Path) -> Result<Self, String> {
        let records = match std::fs::read_to_string(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(format!("read {}: {error}", path.display())),
            Ok(text) if text.trim().is_empty() => Vec::new(),
            Ok(text) => serde_json::from_str(&text)
                .map_err(|error| format!("parse {}: {error}", path.display()))?,
        };
        Ok(Self { path: path.to_owned(), state: Mutex::new(records) })
    }

    fn persist(&self, records: &[WorkflowDefinitionRecord]) -> Result<(), String> {
        let text = serde_json::to_string_pretty(records)
            .map_err(|error| format!("serialize the workflows: {error}"))?;
        let temp = self.path.with_extension("json.tmp");
        std::fs::write(&temp, text).map_err(|error| error.to_string())?;
        std::fs::rename(&temp, &self.path).map_err(|error| error.to_string())
    }
}

impl WorkflowDefinitionRepository for FileWorkflowStore {
    fn get_workflow(
        &self,
        _scope: &OrganizationScope,
        id: &str,
    ) -> Result<Option<WorkflowDefinitionRecord>, String> {
        let records = self.state.lock().expect("the workflow store lock");
        Ok(records.iter().find(|record| record.id == id).cloned())
    }

    fn list_workflows(
        &self,
        _scope: &OrganizationScope,
        visibility: WorkflowVisibility,
    ) -> Result<Vec<WorkflowDefinitionRecord>, String> {
        let records = self.state.lock().expect("the workflow store lock");
        Ok(records
            .iter()
            .filter(|record| record.visibility == visibility)
            .cloned()
            .collect())
    }

    fn upsert_workflow(
        &self,
        _scope: &OrganizationScope,
        record: &WorkflowDefinitionRecord,
    ) -> Result<(), String> {
        let mut records = self.state.lock().expect("the workflow store lock");
        match records.iter_mut().find(|existing| existing.id == record.id) {
            Some(existing) => *existing = record.clone(),
            None => records.push(record.clone()),
        }
        self.persist(&records)
    }

    fn delete_workflow(&self, _scope: &OrganizationScope, id: &str) -> Result<(), String> {
        let mut records = self.state.lock().expect("the workflow store lock");
        let before = records.len();
        records.retain(|record| record.id != id);
        if records.len() != before {
            self.persist(&records)?;
        }
        Ok(())
    }
}

/// The dashboard document: the dashboard rows and the widget rows, one JSON
/// file, written atomically.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct DashboardDocument {
    #[serde(default)]
    dashboards: Vec<DashboardRecord>,
    #[serde(default)]
    widgets: Vec<WidgetRecord>,
}

/// The dashboard store: one JSON file of dashboard and widget rows. The
/// domain rules (what a dashboard contains, when a widget is removed) live
/// in `datazen::dashboard`; this side only persists rows.
pub struct FileDashboardStore {
    path: PathBuf,
    state: Mutex<DashboardDocument>,
}

impl FileDashboardStore {
    pub fn open(path: &Path) -> Result<Self, String> {
        let document = match std::fs::read_to_string(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                DashboardDocument::default()
            }
            Err(error) => return Err(format!("read {}: {error}", path.display())),
            Ok(text) if text.trim().is_empty() => DashboardDocument::default(),
            Ok(text) => serde_json::from_str(&text)
                .map_err(|error| format!("parse {}: {error}", path.display()))?,
        };
        Ok(Self { path: path.to_owned(), state: Mutex::new(document) })
    }

    fn persist(&self, document: &DashboardDocument) -> Result<(), DashboardRepositoryError> {
        let text = serde_json::to_string_pretty(document).map_err(|error| {
            DashboardRepositoryError::Db(format!("serialize the dashboard: {error}"))
        })?;
        let temp = self.path.with_extension("json.tmp");
        std::fs::write(&temp, text)
            .map_err(|error| DashboardRepositoryError::Db(error.to_string()))?;
        std::fs::rename(&temp, &self.path)
            .map_err(|error| DashboardRepositoryError::Db(error.to_string()))
    }
}

fn store_error(error: String) -> DashboardRepositoryError {
    DashboardRepositoryError::Db(error)
}

impl DashboardRepository for FileDashboardStore {
    fn list_dashboard_rows(
        &self,
        _scope: &OrganizationScope,
    ) -> Result<Vec<DashboardRecord>, DashboardRepositoryError> {
        let document = self.state.lock().expect("the dashboard store lock");
        Ok(document.dashboards.clone())
    }

    fn dashboard_row(
        &self,
        _scope: &OrganizationScope,
        id: &str,
    ) -> Result<DashboardRecord, DashboardRepositoryError> {
        let document = self.state.lock().expect("the dashboard store lock");
        document
            .dashboards
            .iter()
            .find(|record| record.id == id)
            .cloned()
            .ok_or_else(|| DashboardRepositoryError::NotFound(id.to_owned()))
    }

    fn upsert_dashboard_row(
        &self,
        _scope: &OrganizationScope,
        row: &DashboardRecord,
    ) -> Result<(), DashboardRepositoryError> {
        let mut document = self.state.lock().expect("the dashboard store lock");
        match document
            .dashboards
            .iter_mut()
            .find(|existing| existing.id == row.id)
        {
            Some(existing) => *existing = row.clone(),
            None => document.dashboards.push(row.clone()),
        }
        self.persist(&document)
    }

    fn delete_dashboard_row(
        &self,
        _scope: &OrganizationScope,
        id: &str,
    ) -> Result<(), DashboardRepositoryError> {
        let mut document = self.state.lock().expect("the dashboard store lock");
        document.dashboards.retain(|record| record.id != id);
        document.widgets.retain(|widget| widget.dashboard_id != id);
        self.persist(&document)
    }

    fn set_dashboard_refresh_paused(
        &self,
        _scope: &OrganizationScope,
        id: &str,
        paused: bool,
    ) -> Result<(), DashboardRepositoryError> {
        let mut document = self.state.lock().expect("the dashboard store lock");
        let record = document
            .dashboards
            .iter_mut()
            .find(|record| record.id == id)
            .ok_or_else(|| DashboardRepositoryError::NotFound(id.to_owned()))?;
        record.refresh_paused = paused;
        self.persist(&document)
    }

    fn list_widget_rows(
        &self,
        _scope: &OrganizationScope,
        dashboard_id: &str,
    ) -> Result<Vec<WidgetRecord>, DashboardRepositoryError> {
        let document = self.state.lock().expect("the dashboard store lock");
        let mut widgets: Vec<WidgetRecord> = document
            .widgets
            .iter()
            .filter(|widget| widget.dashboard_id == dashboard_id)
            .cloned()
            .collect();
        // The stored order is the display order (the domain's save writes
        // `sort_order` as the widget's position).
        widgets.sort_by_key(|widget| widget.sort_order);
        Ok(widgets)
    }

    fn widget_row(
        &self,
        _scope: &OrganizationScope,
        id: &str,
    ) -> Result<WidgetRecord, DashboardRepositoryError> {
        let document = self.state.lock().expect("the dashboard store lock");
        document
            .widgets
            .iter()
            .find(|widget| widget.id == id)
            .cloned()
            .ok_or_else(|| DashboardRepositoryError::NotFound(id.to_owned()))
    }

    fn upsert_widget_row(
        &self,
        _scope: &OrganizationScope,
        row: &WidgetRecord,
    ) -> Result<(), DashboardRepositoryError> {
        let mut document = self.state.lock().expect("the dashboard store lock");
        match document
            .widgets
            .iter_mut()
            .find(|existing| existing.id == row.id)
        {
            Some(existing) => *existing = row.clone(),
            None => document.widgets.push(row.clone()),
        }
        self.persist(&document)
    }

    fn delete_widget_row(
        &self,
        _scope: &OrganizationScope,
        id: &str,
    ) -> Result<(), DashboardRepositoryError> {
        let mut document = self.state.lock().expect("the dashboard store lock");
        document.widgets.retain(|widget| widget.id != id);
        self.persist(&document)
    }

    fn find_workflow_refs(
        &self,
        _scope: &OrganizationScope,
        workflow_id: &str,
    ) -> Result<Vec<DashboardWorkflowRef>, DashboardRepositoryError> {
        let document = self.state.lock().expect("the dashboard store lock");
        let refs = document
            .widgets
            .iter()
            .filter(|widget| widget.workflow_id == workflow_id)
            .filter_map(|widget| {
                let dashboard_name = document
                    .dashboards
                    .iter()
                    .find(|dashboard| dashboard.id == widget.dashboard_id)
                    .map(|dashboard| dashboard.name.clone())?;
                Some(DashboardWorkflowRef {
                    workflow_id: widget.workflow_id.clone(),
                    dashboard_id: widget.dashboard_id.clone(),
                    widget_id: widget.id.clone(),
                    dashboard_name,
                    widget_title: widget.title.clone(),
                })
            })
            .collect();
        Ok(refs)
    }
}

/// The dashboard use case's view of the host (N13): dashboards through the
/// domain rules over the file store, workflows through the registry use
/// case. `create_widget_from_sql` decides everything; this object only
/// answers for the rows.
pub(crate) struct HostWidgets {
    registry: Arc<WorkflowRegistry>,
    store: Arc<FileDashboardStore>,
}

impl HostWidgets {
    pub fn new(registry: Arc<WorkflowRegistry>, store: Arc<FileDashboardStore>) -> Self {
        Self { registry, store }
    }
}

#[async_trait]
impl WidgetCreationHost for HostWidgets {
    fn dashboard(&self, scope: &OrganizationScope, id: &str) -> Result<Dashboard, String> {
        match dashboard_rules::get_dashboard(scope, self.store.as_ref(), id) {
            Ok(dashboard) => Ok(dashboard),
            Err(DashboardRepositoryError::NotFound(_)) => {
                // A native install starts with one dashboard; its row comes
                // into existence the first time a widget needs it.
                let now = chrono::Utc::now().to_rfc3339();
                let dashboard = Dashboard {
                    id: id.to_owned(),
                    name: "Dashboard".into(),
                    created_at: now.clone(),
                    updated_at: now,
                    layout: DashboardLayout { cols: 12, row_height: 4 },
                    widgets: Vec::new(),
                    enabled: true,
                    refresh_paused: false,
                };
                dashboard_rules::save_dashboard(scope, self.store.as_ref(), dashboard.clone())
                    .map_err(|error| error.to_string())?;
                Ok(dashboard)
            }
            Err(error) => Err(error.to_string()),
        }
    }

    fn save_dashboard(&self, scope: &OrganizationScope, dashboard: Dashboard) -> Result<(), String> {
        dashboard_rules::save_dashboard(scope, self.store.as_ref(), dashboard)
            .map_err(|error| error.to_string())
    }

    fn workflow_summary(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<(String, WorkflowVisibility), String> {
        // The registry's own store answers: a widget's workflow is a saved
        // workflow like any other, read through the same handle.
        self.registry
            .store()
            .get_workflow(scope, id)?
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

/// The widget the UI's save form builds (N13): the view mode the port's bar
/// chart reads. The UI's form has no view-mode input, so a saved widget is
/// a chart.
pub(crate) fn form_view_mode() -> ViewMode {
    ViewMode::Chart
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen::dashboard::application::create::create_widget_from_sql;
    use datazen::workflow::domain::repository::encode_definition;

    fn temp_path(tag: &str, file: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("datazen-rune-dashboards-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(file)
    }

    fn definition(name: &str, sql: &str) -> WorkflowDefinition {
        WorkflowDefinition {
            database: None,
            id: name.to_owned(),
            name: name.to_owned(),
            description: "test".into(),
            version: None,
            author: None,
            variables: Vec::new(),
            connection: None,
            steps: vec![WorkflowStep::Query {
                id: "q1".into(),
                sql: sql.to_owned(),
                connection: None,
                database: None,
                timeout_secs: None,
                on_error: None,
            }],
            blocks: Vec::new(),
            output: None,
            timeout_secs: None,
            error_handling: None,
            schedule: None,
            visibility: WorkflowVisibility::User,
        }
    }

    #[test]
    fn a_workflow_round_trips_through_the_registry_over_the_file() {
        let path = temp_path("workflows", "workflows.json");
        let registry =
            WorkflowRegistry::new(Arc::new(FileWorkflowStore::open(&path).unwrap()), Vec::new());
        let scope = host_scope();
        futures_executor::block_on(registry.save(&scope, &definition("etl", "SELECT 1")))
            .expect("the save lands");

        // A fresh registry over the same file reads it back: persistence,
        // not a session cache.
        let registry =
            WorkflowRegistry::new(Arc::new(FileWorkflowStore::open(&path).unwrap()), Vec::new());
        let listed = futures_executor::block_on(registry.list_definitions(&scope));
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "etl");
        assert_eq!(
            listed[0].steps,
            vec![WorkflowStep::Query {
                id: "q1".into(),
                sql: "SELECT 1".into(),
                connection: None,
                database: None,
                timeout_secs: None,
                on_error: None,
            }],
            "the stored YAML round-trips as the same definition"
        );
    }

    #[test]
    fn a_widget_created_through_the_use_case_survives_a_reopen() {
        let workflows = temp_path("widget", "workflows.json");
        let dashboards = temp_path("widget", "dashboards.json");
        let registry = WorkflowRegistry::new(
            Arc::new(FileWorkflowStore::open(&workflows).unwrap()),
            Vec::new(),
        );
        let store = Arc::new(FileDashboardStore::open(&dashboards).unwrap());
        let host = HostWidgets::new(Arc::new(registry), Arc::clone(&store));
        let scope = host_scope();

        let result = futures_executor::block_on(create_widget_from_sql(
            &scope,
            &host,
            DEFAULT_DASHBOARD_ID,
            "live",
            "SELECT label, n FROM facts",
            Some("Facts".into()),
            form_view_mode(),
            None,
        ))
        .expect("the widget is created through the use case");
        assert_eq!(result.widget.title, "Facts");

        // A fresh store over the same files (a restarted process) still
        // lists the widget and its hidden workflow.
        let reopened = FileDashboardStore::open(&dashboards).unwrap();
        let widgets = reopened
            .list_widget_rows(&scope, DEFAULT_DASHBOARD_ID)
            .expect("the dashboard row is there");
        assert_eq!(widgets.len(), 1, "the widget survived the reopen");
        assert_eq!(widgets[0].title, "Facts");
        assert_eq!(widgets[0].workflow_id, result.widget.workflow_id);

        let reopened_registry = WorkflowRegistry::new(
            Arc::new(FileWorkflowStore::open(&workflows).unwrap()),
            Vec::new(),
        );
        let hidden = futures_executor::block_on(
            reopened_registry.get(&scope, &result.widget.workflow_id),
        )
        .expect("the hidden workflow survived");
        assert_eq!(hidden.visibility, WorkflowVisibility::DashboardHidden);
        assert!(encode_definition(&hidden)
            .expect("the definition encodes")
            .contains("SELECT label, n FROM facts"));
    }

    #[test]
    fn widget_ui_ids_are_stable_and_workflow_ids_slug_cleanly() {
        assert_eq!(widget_ui_id("6f2c1b4a-9d3e-4f5a-8b7c-1d2e3f4a5b6c"), 0x6f2c1b4a9d3e4f5a);
        assert_eq!(widget_ui_id("short"), 0, "an id without 16 hex characters reads 0");
        assert_eq!(workflow_id_for("Nightly Backup", &[]), "nightly-backup");
        // A name with no slug characters falls back to the hash spelling.
        let hashed = workflow_id_for("  数据同步  ", &[]);
        assert!(hashed.starts_with("wf-"), "the hashed id spells wf-…, got {hashed}");
        // A slug another workflow owns gets the hash suffix.
        let taken = vec!["nightly-backup".to_owned()];
        assert_ne!(workflow_id_for("nightly backup", &taken), "nightly-backup");
    }
}
