//! Port of `packages/frontend/src/hooks/useQueryExec.ts` (T395) —
//! the per-pane read of the query-execution state.
//!
//! Both faces read the panel store's `queryExec` map through the
//! pane-key rule: the whole-entry face
//! (`usePanelStore(useShallow((s) => s.queryExec.get(key) ??
//! EMPTY_QUERY_EXEC))`) and the single-field face
//! (`usePanelStore((s) => (s.queryExec.get(key) ?? EMPTY)[field])`).
//! `paneId` is optional and defaults to the panel's own (pre-split)
//! pane, whose key is the bare panel id — pre-split callers resolve
//! to exactly the entry they read before the pane wave.
//!
//! ## What maps where
//!
//! - `paneKey` / `DEFAULT_PANE_ID` (`stores/paneKeys.ts`, segment
//!   3's T274) are [`pane_key`] / [`DEFAULT_PANE_ID`], the rules
//!   verbatim: the default pane (`'main'`, or absent) keys by the
//!   bare panel id; a secondary pane by the `panelId::paneId`
//!   composite; the composite always decomposes back to its panel
//!   and pane. The separator is `::` — panel ids are
//!   `panel-<prefix>-<n>`, so they can never contain it.
//! - `QueryExecState` / `EMPTY_QUERY_EXEC`
//!   (`stores/queryExecActions.ts`, re-exported by the panel store —
//!   segment 3's T352) are the [`QueryExecState`] seam type and its
//!   frozen default [`empty_query_exec`]: the map's missing entries
//!   answer the SAME shared default object (React's `Object.freeze`
//!   singleton; the `??` fallback), never a fresh one.
//! - `useShallow` (the whole-entry face) is reference stability for
//!   React's re-render loop — a Rune leaf re-renders off the state
//!   it reads, so the port answers the entry by reference and the
//!   stability is the store's (`map.get` answers the stored value
//!   until the host replaces it).
//! - The single-field face answers ONE field ([`QueryExecField`]) —
//!   the per-field subscription granularity, kept so the port's
//!   callers can take the scalar without the entry.
//!
//! Known deltas: `chartConfig` / `publishedArtifacts` (the optional
//! presentation fields) ride the T352 store port — the chart tile
//! owns its config type. Results carry the UI API's query provenance.

// No production consumer yet (the query panel track wires the
// reads); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::OnceLock;

use datazen_ui_api::types::StatementResult;

/// `DEFAULT_PANE_ID` — the pane a query panel owns before any split.
pub const DEFAULT_PANE_ID: &str = "main";

/// `PANE_SEPARATOR` — between a panel id and a secondary pane id.
/// Panel ids are `panel-<prefix>-<n>`, so they can never contain it.
const PANE_SEPARATOR: &str = "::";

/// `QueryCancelState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryCancelState {
    Idle,
    Requested,
    Failed,
}

impl QueryCancelState {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Requested => "requested",
            Self::Failed => "failed",
        }
    }
}

/// `QueryTerminalState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryTerminalState {
    Succeeded,
    Failed,
    Cancelled,
    Unknown,
}

impl QueryTerminalState {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Unknown => "unknown",
        }
    }
}

/// `resultViewMode?: 'table' | 'chart'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryResultViewMode {
    Table,
    Chart,
}

/// The execution state of one pane.
#[derive(Clone, Debug)]
pub struct QueryExecState {
    pub sql: String,
    pub results: Vec<StatementResult>,
    pub active_result_idx: u32,
    pub error: Option<String>,
    pub running: bool,
    pub execution_time_ms: Option<u64>,
    pub cancel_state: QueryCancelState,
    pub cancel_error: Option<String>,
    pub terminal_state: Option<QueryTerminalState>,
    pub result_view_mode: Option<QueryResultViewMode>,
    pub execution_id: Option<String>,
    pub stream_run_id: Option<u64>,
    pub result_detail_row_index: Option<u32>,
}

impl QueryExecState {
    /// `emptyQueryExecState()` — a fresh mutable copy of the default
    /// (what the store seeds a pane with).
    pub fn empty() -> Self {
        Self {
            sql: String::new(),
            results: Vec::new(),
            active_result_idx: 0,
            error: None,
            running: false,
            execution_time_ms: None,
            cancel_state: QueryCancelState::Idle,
            cancel_error: None,
            terminal_state: None,
            result_view_mode: None,
            execution_id: None,
            stream_run_id: None,
            result_detail_row_index: None,
        }
    }

    /// One field's value (the single-field face's answer).
    pub fn field(&self, field: QueryExecField) -> QueryExecFieldValue {
        match field {
            QueryExecField::Sql => QueryExecFieldValue::Text(self.sql.clone()),
            QueryExecField::ActiveResultIdx => QueryExecFieldValue::Index(self.active_result_idx),
            QueryExecField::Error => QueryExecFieldValue::OptText(self.error.clone()),
            QueryExecField::Running => QueryExecFieldValue::Flag(self.running),
            QueryExecField::ExecutionTimeMs => {
                QueryExecFieldValue::OptNumber(self.execution_time_ms)
            }
            QueryExecField::CancelState => QueryExecFieldValue::Cancel(self.cancel_state),
            QueryExecField::CancelError => QueryExecFieldValue::OptText(self.cancel_error.clone()),
            QueryExecField::TerminalState => QueryExecFieldValue::Terminal(self.terminal_state),
            QueryExecField::ResultViewMode => {
                QueryExecFieldValue::OptViewMode(self.result_view_mode)
            }
            QueryExecField::ExecutionId => QueryExecFieldValue::OptText(self.execution_id.clone()),
            QueryExecField::StreamRunId => QueryExecFieldValue::OptNumber(self.stream_run_id),
            QueryExecField::ResultDetailRowIndex => {
                QueryExecFieldValue::OptIndex(self.result_detail_row_index)
            }
        }
    }
}

/// `EMPTY_QUERY_EXEC` — the shared frozen default the `??` fallback
/// answers (one instance for every caller, React's `Object.freeze`).
pub fn empty_query_exec() -> &'static QueryExecState {
    static EMPTY: OnceLock<QueryExecState> = OnceLock::new();
    EMPTY.get_or_init(QueryExecState::empty)
}

/// `paneKey(panelId, paneId = DEFAULT_PANE_ID)`: the default pane
/// keys by the bare panel id; a secondary pane by the composite.
pub fn pane_key(panel_id: &str, pane_id: Option<&str>) -> String {
    match pane_id {
        None | Some(DEFAULT_PANE_ID) => panel_id.to_owned(),
        Some(pane) => format!("{panel_id}{PANE_SEPARATOR}{pane}"),
    }
}

/// `panelIdOfPaneKey(key)`: the panel that owns `key` (a bare panel
/// id is its own answer).
pub fn panel_id_of_pane_key(key: &str) -> &str {
    match key.find(PANE_SEPARATOR) {
        Some(at) => &key[..at],
        None => key,
    }
}

/// `paneIdOfPaneKey(key)`: the pane `key` names (a bare panel id is
/// the default pane).
pub fn pane_id_of_pane_key(key: &str) -> &str {
    match key.find(PANE_SEPARATOR) {
        Some(at) => &key[at + PANE_SEPARATOR.len()..],
        None => DEFAULT_PANE_ID,
    }
}

/// The single-field face's field selector (`keyof QueryExecState`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryExecField {
    Sql,
    ActiveResultIdx,
    Error,
    Running,
    ExecutionTimeMs,
    CancelState,
    CancelError,
    TerminalState,
    ResultViewMode,
    ExecutionId,
    StreamRunId,
    ResultDetailRowIndex,
}

/// The single-field face's answer (one field's scalar).
#[derive(Clone, Debug, PartialEq)]
pub enum QueryExecFieldValue {
    Text(String),
    Index(u32),
    OptText(Option<String>),
    Flag(bool),
    OptNumber(Option<u64>),
    Cancel(QueryCancelState),
    Terminal(Option<QueryTerminalState>),
    OptViewMode(Option<QueryResultViewMode>),
    OptIndex(Option<u32>),
}

/// `useQueryExec(panelId, paneId?)`: the pane's execution state —
/// the stored entry, or the shared default when the pane has none
/// (a never-run pane, or the map itself not yet mounted).
pub fn use_query_exec<'a>(
    query_exec: Option<&'a HashMap<String, QueryExecState>>,
    panel_id: &str,
    pane_id: Option<&str>,
) -> &'a QueryExecState {
    let key = pane_key(panel_id, pane_id);
    query_exec
        .and_then(|map| map.get(&key))
        .unwrap_or_else(|| empty_query_exec())
}

/// `useQueryExecField(panelId, field, paneId?)`: one field of the
/// pane's execution state (the `?? EMPTY[field]` fallback included).
pub fn use_query_exec_field(
    query_exec: Option<&HashMap<String, QueryExecState>>,
    panel_id: &str,
    field: QueryExecField,
    pane_id: Option<&str>,
) -> QueryExecFieldValue {
    use_query_exec(query_exec, panel_id, pane_id).field(field)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_pane_keys_by_the_bare_panel_id() {
        assert_eq!(pane_key("panel-q-1", None), "panel-q-1");
        // The explicit default pane id is the same key: pre-split
        // callers read exactly the entry they read before.
        assert_eq!(pane_key("panel-q-1", Some("main")), "panel-q-1");
        // A secondary pane composes.
        assert_eq!(pane_key("panel-q-1", Some("right")), "panel-q-1::right");
    }

    #[test]
    fn pane_keys_decompose_back_to_their_panel_and_pane() {
        assert_eq!(panel_id_of_pane_key("panel-q-1"), "panel-q-1");
        assert_eq!(pane_id_of_pane_key("panel-q-1"), "main");
        assert_eq!(panel_id_of_pane_key("panel-q-1::right"), "panel-q-1");
        assert_eq!(pane_id_of_pane_key("panel-q-1::right"), "right");
        // The composite always decomposes (the separator can never
        // appear inside a panel id).
        let key = pane_key("panel-a-2", Some("lower"));
        assert_eq!(panel_id_of_pane_key(&key), "panel-a-2");
        assert_eq!(pane_id_of_pane_key(&key), "lower");
    }

    #[test]
    fn a_missing_entry_answers_the_shared_default() {
        // No map at all (the store not yet mounted): the default.
        let read = use_query_exec(None, "panel-q-1", None);
        assert!(read.sql.is_empty());
        assert!(read.results.is_empty());
        assert!(!read.running);
        assert_eq!(read.active_result_idx, 0);
        assert_eq!(read.cancel_state, QueryCancelState::Idle);
        // A map without the pane's entry: the same shared default.
        let map: HashMap<String, QueryExecState> = HashMap::new();
        let read = use_query_exec(Some(&map), "panel-q-1", Some("right"));
        // The default is the frozen singleton: every fallback answers
        // the SAME instance.
        assert!(std::ptr::eq(read, empty_query_exec()));
        assert_eq!(read.sql, "");
        assert!(read.results.is_empty());
        assert_eq!(read.active_result_idx, 0);
        assert_eq!(read.cancel_state, QueryCancelState::Idle);
        assert_eq!(read.terminal_state, None);
        assert_eq!(read.result_detail_row_index, None);
    }

    #[test]
    fn a_stored_entry_answers_by_its_pane_key() {
        let mut map = HashMap::new();
        let mut entry = QueryExecState::empty();
        entry.sql = "SELECT 1".to_owned();
        entry.running = true;
        entry.execution_id = Some("exec-9".to_owned());
        entry.terminal_state = Some(QueryTerminalState::Succeeded);
        map.insert(pane_key("panel-q-1", None), entry.clone());
        map.insert(pane_key("panel-q-1", Some("right")), {
            let mut split = QueryExecState::empty();
            split.sql = "SELECT 2".to_owned();
            split
        });

        let default_pane = use_query_exec(Some(&map), "panel-q-1", None);
        assert_eq!(default_pane.sql, "SELECT 1");
        assert!(default_pane.running);
        // Reference stability (the useShallow rule): the same stored
        // entry answers while the map is unchanged.
        assert!(std::ptr::eq(
            use_query_exec(Some(&map), "panel-q-1", None),
            default_pane
        ));
        let right = use_query_exec(Some(&map), "panel-q-1", Some("right"));
        assert_eq!(right.sql, "SELECT 2");
        assert!(!right.running);
    }

    #[test]
    fn the_field_face_answers_one_scalar() {
        let mut map = HashMap::new();
        let mut entry = QueryExecState::empty();
        entry.execution_time_ms = Some(120);
        entry.active_result_idx = 2;
        entry.result_view_mode = Some(QueryResultViewMode::Chart);
        entry.cancel_state = QueryCancelState::Requested;
        map.insert(pane_key("panel-q-1", None), entry);

        assert_eq!(
            use_query_exec_field(
                Some(&map),
                "panel-q-1",
                QueryExecField::ExecutionTimeMs,
                None
            ),
            QueryExecFieldValue::OptNumber(Some(120))
        );
        assert_eq!(
            use_query_exec_field(
                Some(&map),
                "panel-q-1",
                QueryExecField::ActiveResultIdx,
                None
            ),
            QueryExecFieldValue::Index(2)
        );
        assert_eq!(
            use_query_exec_field(
                Some(&map),
                "panel-q-1",
                QueryExecField::ResultViewMode,
                None
            ),
            QueryExecFieldValue::OptViewMode(Some(QueryResultViewMode::Chart))
        );
        assert_eq!(
            use_query_exec_field(Some(&map), "panel-q-1", QueryExecField::CancelState, None),
            QueryExecFieldValue::Cancel(QueryCancelState::Requested)
        );
        // The fallback answers the default's field (the `?? EMPTY[field]`).
        assert_eq!(
            use_query_exec_field(None, "panel-q-1", QueryExecField::Sql, Some("right")),
            QueryExecFieldValue::Text(String::new())
        );
        assert_eq!(
            use_query_exec_field(None, "panel-q-1", QueryExecField::Running, None),
            QueryExecFieldValue::Flag(false)
        );
    }

    #[test]
    fn the_wire_spellings_are_the_react_literals() {
        assert_eq!(QueryCancelState::Idle.as_str(), "idle");
        assert_eq!(QueryCancelState::Requested.as_str(), "requested");
        assert_eq!(QueryCancelState::Failed.as_str(), "failed");
        assert_eq!(QueryTerminalState::Succeeded.as_str(), "succeeded");
        assert_eq!(QueryTerminalState::Failed.as_str(), "failed");
        assert_eq!(QueryTerminalState::Cancelled.as_str(), "cancelled");
        assert_eq!(QueryTerminalState::Unknown.as_str(), "unknown");
        assert_eq!(DEFAULT_PANE_ID, "main");
    }
}
