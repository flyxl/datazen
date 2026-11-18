//! Shared test fixtures for the ai_sessions module (test-only).
//!
//! Mirrors the React suites' harnesses: `MemoryStorage` is the
//! `vi.stubGlobal('localStorage', {...})` object with optional per-op
//! failures, `MockBackend` is the `mockAiCommands` double (both its
//! programmed-responses mode and the row-store mode the legacy-rebind
//! suite installs), and `TestEnv` wires an [`AiSessionStore`] with a
//! deterministic clock and id counter.

#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use serde_json::Value;

use super::local::SessionLocalStorage;
use super::model::{
    AiMessageRole, AiMessageStatus, AiSessionDetail, AiSessionMessage, AiSessionSummary,
    AiSessionUpsertInput, ExportFormat,
};
use super::store::{AiSessionBackend, AiSessionStore, SessionClock, SessionListParams};

/// The fixed `now` the test clock reports (React's `new Date()` is stubbed
/// implicitly by assertions on derived values).
pub const NOW_ISO: &str = "2026-02-02T00:00:00.000Z";

/// In-memory localStorage with optional per-op failures.
#[derive(Default)]
pub struct MemoryStorage {
    pub cells: RefCell<std::collections::HashMap<String, String>>,
    pub fail_get: Cell<bool>,
    pub fail_set: Cell<bool>,
}

impl MemoryStorage {
    /// The raw stored value, for byte-level assertions.
    pub fn raw(&self, key: &str) -> Option<String> {
        self.cells.borrow().get(key).cloned()
    }
}

impl SessionLocalStorage for MemoryStorage {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        if self.fail_get.get() {
            return Err("storage unavailable".to_string());
        }
        Ok(self.cells.borrow().get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        if self.fail_set.get() {
            return Err("quota exceeded".to_string());
        }
        self.cells.borrow_mut().insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn remove(&self, key: &str) -> Result<(), String> {
        self.cells.borrow_mut().remove(key);
        Ok(())
    }
}

/// The `aiCommands.sessions*` double's programmable behaviors.
#[derive(Clone, Debug)]
pub(crate) enum ListBehavior {
    /// `sessionsList.mockResolvedValue([])`-style default.
    Empty,
    /// `mockResolvedValue([...])`.
    Summaries(Vec<AiSessionSummary>),
    /// `mockRejectedValue(new Error(msg))`.
    Error(String),
    /// The row-store mode `installBackend()` installs.
    Rows,
}

impl Default for ListBehavior {
    fn default() -> Self {
        ListBehavior::Empty
    }
}

#[derive(Clone, Debug)]
pub(crate) enum GetBehavior {
    /// Row not found (the row-store mode's throw).
    NotFound,
    Detail(AiSessionDetail),
    Error(String),
    Rows,
}

impl Default for GetBehavior {
    fn default() -> Self {
        GetBehavior::NotFound
    }
}

#[derive(Clone, Debug)]
pub(crate) enum UpsertBehavior {
    /// `mockResolvedValue({})` — the suite's beforeEach default.
    Junk,
    Error(String),
    Rows,
}

impl Default for UpsertBehavior {
    fn default() -> Self {
        UpsertBehavior::Junk
    }
}

#[derive(Clone, Debug)]
pub(crate) enum DeleteBehavior {
    Ok,
    Error(String),
}

impl Default for DeleteBehavior {
    fn default() -> Self {
        DeleteBehavior::Ok
    }
}

#[derive(Clone, Debug)]
pub(crate) enum ExportBehavior {
    Content(String),
    Error(String),
}

impl Default for ExportBehavior {
    fn default() -> Self {
        // React's un-stubbed double returns undefined; nothing in the
        // suite calls export without programming it first.
        ExportBehavior::Error("not programmed".to_string())
    }
}

/// The React suite's `mockAiCommands` double: every op logs its calls, and
/// each op's behavior is either programmed (fixed response/error) or the
/// row-store mode.
#[derive(Default)]
pub struct MockBackend {
    list: RefCell<ListBehavior>,
    get: RefCell<GetBehavior>,
    upsert: RefCell<UpsertBehavior>,
    delete: RefCell<DeleteBehavior>,
    export: RefCell<ExportBehavior>,
    rows: RefCell<Vec<AiSessionDetail>>,
    upsert_log: RefCell<Vec<AiSessionUpsertInput>>,
    list_log: RefCell<Vec<SessionListParams>>,
    get_log: RefCell<Vec<String>>,
    delete_log: RefCell<Vec<String>>,
    export_log: RefCell<Vec<(String, ExportFormat)>>,
    /// One-shot list responses, consumed in call order before the
    /// programmed behavior (for sequence-sensitive failures).
    list_once: RefCell<VecDeque<Result<Vec<AiSessionSummary>, String>>>,
}

impl MockBackend {
    /// The suite's `installBackend()`: list/get/upsert operate on the row
    /// store (delete/export stay at their defaults, exactly like React).
    pub fn install_rows(&self) {
        *self.list.borrow_mut() = ListBehavior::Rows;
        *self.get.borrow_mut() = GetBehavior::Rows;
        *self.upsert.borrow_mut() = UpsertBehavior::Rows;
    }

    pub fn program_list_summaries(&self, summaries: Vec<AiSessionSummary>) {
        *self.list.borrow_mut() = ListBehavior::Summaries(summaries);
    }

    pub fn program_list_error(&self, message: &str) {
        *self.list.borrow_mut() = ListBehavior::Error(message.to_string());
    }

    pub fn program_get_detail(&self, detail: AiSessionDetail) {
        *self.get.borrow_mut() = GetBehavior::Detail(detail);
    }

    pub fn program_get_error(&self, message: &str) {
        *self.get.borrow_mut() = GetBehavior::Error(message.to_string());
    }

    pub fn program_upsert_error(&self, message: &str) {
        *self.upsert.borrow_mut() = UpsertBehavior::Error(message.to_string());
    }

    pub fn program_export_content(&self, content: &str) {
        *self.export.borrow_mut() = ExportBehavior::Content(content.to_string());
    }

    pub fn program_export_error(&self, message: &str) {
        *self.export.borrow_mut() = ExportBehavior::Error(message.to_string());
    }

    /// Queues a one-shot list response, consumed before the programmed
    /// behavior in call order.
    pub fn push_list_once_ok(&self, summaries: Vec<AiSessionSummary>) {
        self.list_once.borrow_mut().push_back(Ok(summaries));
    }

    /// Queues a one-shot list failure, consumed before the programmed
    /// behavior in call order.
    pub fn push_list_once_error(&self, message: &str) {
        self.list_once
            .borrow_mut()
            .push_back(Err(message.to_string()));
    }

    /// Plants a row in the row store without going through upsert.
    pub fn seed_row(&self, detail: AiSessionDetail) {
        self.rows.borrow_mut().push(detail);
    }

    pub fn upsert_inputs(&self) -> Vec<AiSessionUpsertInput> {
        self.upsert_log.borrow().clone()
    }

    pub fn list_params(&self) -> Vec<SessionListParams> {
        self.list_log.borrow().clone()
    }

    pub fn get_ids(&self) -> Vec<String> {
        self.get_log.borrow().clone()
    }

    pub fn delete_ids(&self) -> Vec<String> {
        self.delete_log.borrow().clone()
    }

    pub fn export_ids(&self) -> Vec<(String, ExportFormat)> {
        self.export_log.borrow().clone()
    }

    pub fn rows(&self) -> Vec<AiSessionDetail> {
        self.rows.borrow().clone()
    }

    pub fn row_count(&self) -> usize {
        self.rows.borrow().len()
    }

    fn blank_detail() -> AiSessionDetail {
        AiSessionDetail {
            id: String::new(),
            session_key: String::new(),
            connection_id: None,
            database: None,
            title: String::new(),
            metadata: None,
            message_count: 0,
            preview: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
            last_access_at: String::new(),
            messages: Vec::new(),
        }
    }
}

impl AiSessionBackend for MockBackend {
    fn sessions_list(&self, params: &SessionListParams) -> Result<Vec<AiSessionSummary>, String> {
        self.list_log.borrow_mut().push(params.clone());
        if let Some(result) = self.list_once.borrow_mut().pop_front() {
            return result;
        }
        match self.list.borrow().clone() {
            ListBehavior::Empty => Ok(Vec::new()),
            ListBehavior::Summaries(summaries) => Ok(summaries),
            ListBehavior::Error(message) => Err(message),
            // The suite's row-store filter: `!args.connectionId ||
            // row.connectionId === args.connectionId` — falsy (null, '')
            // disables the filter.
            ListBehavior::Rows => {
                let rows = self.rows.borrow().clone();
                let want_connection =
                    params.connection_id.as_deref().filter(|id| !id.is_empty());
                let want_database = params.database.as_deref().filter(|db| !db.is_empty());
                let want_key = params.session_key.as_deref().filter(|key| !key.is_empty());
                Ok(rows
                    .iter()
                    .filter(|row| {
                        want_connection
                            .map_or(true, |want| row.connection_id.as_deref() == Some(want))
                            && want_database
                                .map_or(true, |want| row.database.as_deref() == Some(want))
                            && want_key.map_or(true, |want| row.session_key == want)
                    })
                    .map(AiSessionDetail::to_summary)
                    .collect())
            }
        }
    }

    fn sessions_get(&self, id: &str) -> Result<AiSessionDetail, String> {
        self.get_log.borrow_mut().push(id.to_string());
        match self.get.borrow().clone() {
            GetBehavior::Rows => {
                let rows = self.rows.borrow();
                match rows.iter().find(|row| row.id == id) {
                    Some(row) => Ok(row.clone()),
                    None => Err("not found".to_string()),
                }
            }
            GetBehavior::NotFound => Err("not found".to_string()),
            GetBehavior::Detail(detail) => Ok(detail),
            GetBehavior::Error(message) => Err(message),
        }
    }

    fn sessions_upsert(&self, input: &AiSessionUpsertInput) -> Result<AiSessionDetail, String> {
        self.upsert_log.borrow_mut().push(input.clone());
        match self.upsert.borrow().clone() {
            UpsertBehavior::Junk => Ok(Self::blank_detail()),
            UpsertBehavior::Error(message) => Err(message),
            UpsertBehavior::Rows => {
                // The suite's row-store upsert: id-keyed replace with the
                // input's fields backfilling from the previous row.
                let id = input.id.clone().unwrap_or_else(|| "generated".to_string());
                let previous = self.rows.borrow().iter().find(|row| row.id == id).cloned();
                let messages: Vec<AiSessionMessage> = input
                    .messages
                    .iter()
                    .enumerate()
                    .map(|(index, message)| AiSessionMessage {
                        id: message
                            .id
                            .clone()
                            .unwrap_or_else(|| format!("{id}:m{index}")),
                        role: message.role,
                        content: message.content.clone(),
                        status: message.status.unwrap_or(AiMessageStatus::Complete),
                        metadata: message.metadata.clone(),
                        created_at: message
                            .created_at
                            .clone()
                            .unwrap_or_else(|| "2026-01-01T00:00:00Z".to_string()),
                    })
                    .collect();
                let row = AiSessionDetail {
                    id: id.clone(),
                    session_key: input.session_key.clone(),
                    connection_id: input.connection_id.clone(),
                    database: input.database.clone(),
                    title: input
                        .title
                        .clone()
                        .or_else(|| previous.as_ref().map(|row| row.title.clone()))
                        .unwrap_or_default(),
                    metadata: input
                        .metadata
                        .clone()
                        .or_else(|| previous.as_ref().and_then(|row| row.metadata.clone())),
                    message_count: messages.len(),
                    preview: messages
                        .iter()
                        .find(|message| message.role == AiMessageRole::User)
                        .map(|message| message.content.clone())
                        .unwrap_or_default(),
                    created_at: previous
                        .as_ref()
                        .map(|row| row.created_at.clone())
                        .unwrap_or_else(|| "2026-01-01T00:00:00Z".to_string()),
                    updated_at: "2026-01-01T00:00:02Z".to_string(),
                    last_access_at: input
                        .last_access_at
                        .clone()
                        .unwrap_or_else(|| "2026-01-01T00:00:02Z".to_string()),
                    messages,
                };
                let mut rows = self.rows.borrow_mut();
                match rows.iter_mut().find(|existing| existing.id == row.id) {
                    Some(slot) => *slot = row.clone(),
                    None => rows.push(row.clone()),
                }
                Ok(row)
            }
        }
    }

    fn sessions_delete(&self, id: &str) -> Result<bool, String> {
        self.delete_log.borrow_mut().push(id.to_string());
        match self.delete.borrow().clone() {
            DeleteBehavior::Ok => Ok(true),
            DeleteBehavior::Error(message) => Err(message),
        }
    }

    fn sessions_export(&self, id: &str, format: ExportFormat) -> Result<String, String> {
        self.export_log.borrow_mut().push((id.to_string(), format));
        match self.export.borrow().clone() {
            ExportBehavior::Content(content) => Ok(content),
            ExportBehavior::Error(message) => Err(message),
        }
    }
}

/// Rc-sharing adapters so tests keep inspecting the doubles the store owns.
pub struct SharedBackend(pub Rc<MockBackend>);

impl AiSessionBackend for SharedBackend {
    fn sessions_list(&self, params: &SessionListParams) -> Result<Vec<AiSessionSummary>, String> {
        self.0.sessions_list(params)
    }

    fn sessions_get(&self, id: &str) -> Result<AiSessionDetail, String> {
        self.0.sessions_get(id)
    }

    fn sessions_upsert(&self, input: &AiSessionUpsertInput) -> Result<AiSessionDetail, String> {
        self.0.sessions_upsert(input)
    }

    fn sessions_delete(&self, id: &str) -> Result<bool, String> {
        self.0.sessions_delete(id)
    }

    fn sessions_export(&self, id: &str, format: ExportFormat) -> Result<String, String> {
        self.0.sessions_export(id, format)
    }
}

pub struct SharedStorage(pub Rc<MemoryStorage>);

impl SessionLocalStorage for SharedStorage {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        self.0.get(key)
    }

    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        self.0.set(key, value)
    }

    fn remove(&self, key: &str) -> Result<(), String> {
        self.0.remove(key)
    }
}

/// A store wired to the doubles: fixed now (`NOW_ISO` / `now_ms` cell) and a
/// `gen-N` id counter.
pub struct TestEnv {
    pub store: AiSessionStore,
    pub backend: Rc<MockBackend>,
    pub storage: Rc<MemoryStorage>,
    pub now_ms: Rc<Cell<f64>>,
    pub id_counter: Rc<Cell<u32>>,
}

/// The React suite's `detail` fixture (sessions.backend.test.ts), with the
/// summary projection used to program `sessionsList`.
pub fn fixture_summary() -> AiSessionSummary {
    AiSessionSummary {
        id: "s1".to_string(),
        session_key: "conn-1::db".to_string(),
        connection_id: Some("conn-1".to_string()),
        database: Some("db".to_string()),
        title: "Saved conversation".to_string(),
        metadata: Some(serde_json::json!({ "templateId": "default" })),
        message_count: 2,
        preview: "hello".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:01Z".to_string(),
        last_access_at: "2026-01-01T00:00:01Z".to_string(),
    }
}

pub fn fixture_detail() -> AiSessionDetail {
    AiSessionDetail {
        id: "s1".to_string(),
        session_key: "conn-1::db".to_string(),
        connection_id: Some("conn-1".to_string()),
        database: Some("db".to_string()),
        title: "Saved conversation".to_string(),
        metadata: Some(serde_json::json!({ "templateId": "default" })),
        message_count: 3,
        preview: "hello".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:01Z".to_string(),
        last_access_at: "2026-01-01T00:00:01Z".to_string(),
        messages: vec![
            AiSessionMessage {
                id: "m1".to_string(),
                role: AiMessageRole::User,
                content: "hello".to_string(),
                status: AiMessageStatus::Complete,
                metadata: Some(serde_json::json!({ "memoryUsed": [{ "id": "mem-1" }] })),
                created_at: "2026-01-01T00:00:00Z".to_string(),
            },
            AiSessionMessage {
                id: "m2".to_string(),
                role: AiMessageRole::Assistant,
                content: "stopped".to_string(),
                status: AiMessageStatus::Cancelled,
                metadata: Some(serde_json::json!({ "reasoning": "thinking", "cancelled": true })),
                created_at: "2026-01-01T00:00:01Z".to_string(),
            },
            AiSessionMessage {
                id: "m3".to_string(),
                role: AiMessageRole::Assistant,
                content: "Error: boom".to_string(),
                status: AiMessageStatus::Failed,
                metadata: None,
                created_at: "2026-01-01T00:00:02Z".to_string(),
            },
        ],
    }
}

/// A `chatSessionFromDetail(fixtureDetail())` equivalent, precomputed for
/// tests that need the live-session shape.
pub fn fixture_session() -> super::model::AiChatSession {
    super::mapping::chat_session_from_detail(&fixture_detail())
}

/// A stored-entry JSON blob, as the React tests seed it.
pub fn stored_json(entries_json: Value) -> String {
    serde_json::to_string(&entries_json).unwrap_or_default()
}

/// The legacy seed the rebind suite uses: `db-sess::db` with one user
/// message (`old question`) and an optional cancelled assistant reply.
pub fn legacy_local_json(cancelled_reply: bool) -> String {
    let messages: Vec<Value> = if cancelled_reply {
        vec![
            serde_json::json!({ "role": "user", "content": "old question" }),
            serde_json::json!({ "role": "assistant", "content": "old answer", "cancelled": true }),
        ]
    } else {
        vec![serde_json::json!({ "role": "user", "content": "old question" })]
    };
    serde_json::to_string(&serde_json::json!({
        "db-sess::db": { "messages": messages, "lastAccess": 1000 }
    }))
    .unwrap_or_default()
}

impl TestEnv {
    /// Seeds localStorage's session blob (React's `localStorage.setItem`
    /// in beforeEach).
    pub fn seed_storage(&self, json: &str) {
        self.storage
            .cells
            .borrow_mut()
            .insert(super::local::STORAGE_KEY.to_string(), json.to_string());
    }

    pub fn raw_storage(&self) -> Option<String> {
        self.storage.raw(super::local::STORAGE_KEY)
    }

    pub fn marker(&self) -> Option<String> {
        self.storage.raw(super::local::MIGRATION_MARKER_KEY)
    }

    /// The clock's current ms reading.
    pub fn now(&self) -> f64 {
        self.now_ms.get()
    }

    pub fn set_now(&self, ms: f64) {
        self.now_ms.set(ms);
    }
}

/// Builds a fresh `TestEnv`.
pub fn env() -> TestEnv {
    let backend = Rc::new(MockBackend::default());
    let storage = Rc::new(MemoryStorage::default());
    let now_ms = Rc::new(Cell::new(1_000.0));
    let id_counter = Rc::new(Cell::new(0u32));
    let clock = SessionClock {
        now_ms: Box::new({
            let now_ms = now_ms.clone();
            move || now_ms.get()
        }),
        now_iso: Box::new(|| NOW_ISO.to_string()),
    };
    let new_id = Box::new({
        let id_counter = id_counter.clone();
        move || {
            let next = id_counter.get() + 1;
            id_counter.set(next);
            format!("gen-{next}")
        }
    });
    TestEnv {
        store: AiSessionStore::new(
            Box::new(SharedBackend(backend.clone())),
            Box::new(SharedStorage(storage.clone())),
            clock,
            new_id,
        ),
        backend,
        storage,
        now_ms,
        id_counter,
    }
}
