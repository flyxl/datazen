//! Document connection view, ported from
//! `packages/frontend/src/windows/connection/DocumentConnectionView.tsx`.
//!
//! React is a MongoDB document browser with two tabs (documents / queries), a
//! database+collection sidebar, a filter bar, a virtualized result table, an
//! insert/edit drawer and an embedded query panel. The Rune port keeps that
//! structure as a host-owned [`DocumentViewVm`] and emits [`DocAction`]s; the
//! host executes every command and feeds [`DocEvent`]s back. No fake IPC.
//!
//! ## Honest adaptations (removal stages)
//!
//! * MongoDB command builders (`mongodbFind` UI module) have no Rune port; the
//!   host builds the SQL in its action handler. Remove when that module lands.
//! * `useVirtualizer` is replaced by `ui::table::DataTable` (static rows) — the
//!   virtualized table base is a separate task.
//! * `SqlEditor` / `useQueryExec` are represented by a text edit and a small
//!   query VM; remove when those ports land.
//! * Schema store, panel store and native context menu are host-owned.
//!
//! ## Keys
//!
//! `doc-view` root with `-header`, `-refresh`, `-tab-documents`, `-tab-queries`,
//! `-connection-name`, `-sidebar`, `-db-<name>`, `-collection-search`,
//! `-collection-<name>`, `-toolbar`, `-filter`, `-apply-filter`, `-insert`,
//! `-doc-count`, `-filter-error`, `-docs-error`, `-result`, `-detail`,
//! `-query-*`; all strings come from the locale pack.

use std::rc::Rc;

use rune::El;

#[path = "document_connection_view_body.rs"]
mod body;

pub use body::build_body;

/// React `ActiveTab`.
#[allow(dead_code)] // VM surface; consumed by the body and the tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DocTab {
    #[default]
    Documents,
    Queries,
}

/// One loaded statement result, reduced to what the table shows.
#[allow(dead_code)] // VM surface; supplied by the host result feed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DocResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// The `_id` of each row, when present; used to keep selection across reloads.
    pub ids: Vec<Option<String>>,
}

/// The embedded query panel's VM (React `useQueryExec`).
#[allow(dead_code)] // VM surface; supplied by the host result feed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueryVm {
    pub sql: String,
    pub running: bool,
    pub error: String,
    pub results: Vec<DocResult>,
    pub active_result_idx: usize,
    pub elapsed_ms: Option<i64>,
}

/// The full view snapshot the host fills each frame.
#[allow(dead_code)] // VM surface; owned by the host until the panel store lands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DocumentViewVm {
    pub connection_name: String,
    pub db_session_id: String,
    pub connection_id: String,
    pub database_type: String,
    pub initial_database: Option<String>,

    pub active_tab: DocTab,
    pub databases: Vec<String>,
    pub loading_databases: bool,
    pub selected_db: Option<String>,
    pub collections: Vec<String>,
    pub collections_loading: bool,
    pub collection_filter: String,
    pub selected_collection: Option<String>,

    pub filter_text: String,
    pub filter_error: String,
    pub docs_loading: bool,
    pub docs_error: String,
    pub result: Option<DocResult>,
    pub selected_row_idx: Option<usize>,

    pub edit_text: String,
    pub edit_error: String,
    pub mutating: bool,
    pub insert_mode: bool,

    pub query: QueryVm,
}

impl DocumentViewVm {
    /// React `filteredCollections`: soft, case-insensitive substring filter.
    pub fn filtered_collections(&self) -> Vec<&String> {
        let needle = self.collection_filter.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return self.collections.iter().collect();
        }
        self.collections
            .iter()
            .filter(|name| name.to_ascii_lowercase().contains(&needle))
            .collect()
    }

    /// React `showEditor = selectedDoc !== null || insertMode`.
    pub fn show_editor(&self) -> bool {
        self.insert_mode || self.selected_row_idx.is_some()
    }

    /// React `canSave = hasDocumentId && !insertMode`.
    pub fn has_document_id(&self) -> bool {
        document_id(&self.edit_text).is_some()
    }

    /// React `docCount` display value.
    pub fn doc_count(&self) -> usize {
        self.result.as_ref().map_or(0, |result| result.rows.len())
    }

    /// React `initialDatabase ?? databases[0]` when a database must be picked.
    #[allow(dead_code)] // consumer: the document-view host wiring (not landed); remove when wired.
    pub fn initial_database_choice(&self) -> Option<&str> {
        self.initial_database
            .as_deref()
            .filter(|db| self.databases.iter().any(|item| item == db))
            .or_else(|| self.databases.first().map(String::as_str))
    }
}

/// Extract `_id` from a JSON document. A missing/null id is `None`. The whole
/// string value is returned — commas are part of the id, not a separator.
pub fn document_id(json: &str) -> Option<String> {
    let trimmed = json.trim();
    let rest = trimmed.strip_prefix('{')?.trim();
    let after_key = rest.strip_prefix("\"_id\"")?.trim_start().strip_prefix(':')?.trim_start();
    let value = if let Some(quoted) = after_key.strip_prefix('"') {
        // React `getDocumentId` keeps the full string, commas included.
        let end = quoted.find('"')?;
        &quoted[..end]
    } else {
        let end = after_key
            .find(|c: char| c == ',' || c == '}' || c.is_whitespace())
            .unwrap_or(after_key.len());
        &after_key[..end]
    };
    if value.is_empty() || value == "null" {
        return None;
    }
    Some(value.to_owned())
}

/// Host calls the view emits. No SQL is built here.
#[allow(dead_code)] // host port; remove when the panel-store view track wires it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocAction {
    Refresh,
    SelectTab(DocTab),
    SelectDb(String),
    SelectCollection(String),
    ApplyFilter,
    InsertNew,
    SaveDocument,
    InsertDocument,
    DeleteDocument,
    CloseEditor,
    SelectRow(usize),
    QueryExecute,
    QuerySqlChanged(String),
    SetActiveResult(usize),
}

/// Host answers fed back into the VM.
#[allow(dead_code)] // host port; remove when the panel-store view track wires it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocEvent {
    SchemaLoaded(Result<Vec<String>, String>),
    CollectionsLoaded(Result<Vec<String>, String>),
    DocumentsLoaded(Result<Option<DocResult>, String>),
    MutationDone(Result<(), String>),
    QueryExecuted(Result<Vec<DocResult>, String>),
}

impl DocumentViewVm {
    /// React `handleSelectDb` + `handleSelectCollection` reset rules.
    #[allow(dead_code)] // consumer: the document-view host wiring (not landed); remove when wired.
    pub fn select_db(&mut self, db: String) {
        self.selected_db = Some(db);
        self.selected_collection = None;
        self.result = None;
        self.selected_row_idx = None;
        self.insert_mode = false;
        self.edit_text.clear();
        self.edit_error.clear();
        self.docs_error.clear();
    }

    #[allow(dead_code)] // consumer: the document-view host wiring (not landed); remove when wired.
    pub fn select_collection(&mut self, name: String) {
        self.selected_collection = Some(name);
        self.insert_mode = false;
        self.edit_text.clear();
        self.edit_error.clear();
    }

    #[allow(dead_code)] // consumer: the document-view host wiring (not landed); remove when wired.
    pub fn open_insert(&mut self) {
        self.selected_row_idx = None;
        self.insert_mode = true;
        self.edit_text = "{}".to_owned();
        self.edit_error.clear();
    }

    #[allow(dead_code)] // consumer: the document-view host wiring (not landed); remove when wired.
    pub fn close_editor(&mut self) {
        self.selected_row_idx = None;
        self.insert_mode = false;
        self.edit_text.clear();
        self.edit_error.clear();
    }

    /// A load answer. React classifies filter-JSON failures separately.
    #[allow(dead_code)] // consumer: the document-view host result feed (not landed); remove when wired.
    pub fn apply_documents(&mut self, answer: Result<Option<DocResult>, String>) {
        self.docs_loading = false;
        self.filter_error.clear();
        self.docs_error.clear();
        match answer {
            Ok(result) => {
                self.result = result;
                self.selected_row_idx = None;
                self.insert_mode = false;
                self.edit_text.clear();
                self.edit_error.clear();
            }
            Err(message) => {
                self.result = None;
                if message.contains("Filter must be") || message.contains("JSON") {
                    self.filter_error = message;
                } else {
                    self.docs_error = message;
                }
            }
        }
    }

    /// Resolve any host event into the next VM state.
    #[allow(dead_code)] // consumer: the document-view host result feed (not landed); remove when wired.
    pub fn apply_event(&mut self, event: DocEvent) {
        match event {
            DocEvent::SchemaLoaded(Ok(databases)) => {
                self.loading_databases = false;
                self.databases = databases;
            }
            DocEvent::SchemaLoaded(Err(_)) => self.loading_databases = false,
            DocEvent::CollectionsLoaded(Ok(collections)) => {
                self.collections_loading = false;
                self.collections = collections;
            }
            DocEvent::CollectionsLoaded(Err(_)) => {
                self.collections_loading = false;
                self.collections.clear();
            }
            DocEvent::DocumentsLoaded(answer) => self.apply_documents(answer),
            DocEvent::MutationDone(Ok(())) => self.mutating = false,
            DocEvent::MutationDone(Err(message)) => {
                self.mutating = false;
                self.edit_error = message;
            }
            DocEvent::QueryExecuted(Ok(results)) => {
                self.query.running = false;
                self.query.error.clear();
                let len = results.len();
                self.query.results = results;
                if self.query.active_result_idx >= len {
                    self.query.active_result_idx = len.saturating_sub(1);
                }
            }
            DocEvent::QueryExecuted(Err(message)) => {
                self.query.running = false;
                self.query.error = message;
            }
        }
    }
}

/// Render the composed document view into `layer`.
#[allow(dead_code)] // consumer: connection_page composition (wired by T337).
pub fn build<S: 'static>(
    vm: &DocumentViewVm,
    on_action: Option<Rc<dyn Fn(&mut S, DocAction) + 'static>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    build_body::<S>(vm, on_action, dark, layer);
}
