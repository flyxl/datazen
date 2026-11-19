//! Pure value logic for the content-view drawers, ported from the derivations
//! in `packages/frontend/src/windows/connection/ContentViewDrawers.tsx` and
//! `packages/frontend/src/lib/resultEditability.ts`.
//!
//! No Rune tree, no state and no host calls live here: the view module feeds
//! these functions a snapshot and renders their answers. The React `bigint`/
//! store subscriptions become plain values; all strings are cloned where the
//! host would have handed an immutable snapshot.

use std::collections::HashMap;

use serde_json::Value;

/// React `DEFAULT_PANE_ID`. A panel's default pane is keyed by its bare id.
pub(crate) const DEFAULT_PANE_ID: &str = "main";
const PANE_SEPARATOR: &str = "::";

/// React `paneKey(panelId, paneId)`.
pub(crate) fn pane_key(panel_id: &str, pane_id: &str) -> String {
    if pane_id == DEFAULT_PANE_ID {
        panel_id.to_owned()
    } else {
        format!("{panel_id}{PANE_SEPARATOR}{pane_id}")
    }
}

/// React `resolveFocusedPaneId`: `null` means the default pane.
pub(crate) fn resolve_focused_pane_id(focused: Option<&str>) -> &str {
    focused.unwrap_or(DEFAULT_PANE_ID)
}

/// A result column's name and driver type declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ColumnMeta {
    pub name: String,
    pub data_type: String,
}

/// The narrow write-back fact shipped by the backend for one result.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ResultSource {
    /// `source.writableMapping === 'verified'`.
    pub writable_mapping_verified: bool,
}

/// React `StatementResult` (only the fields the drawer reads).
#[derive(Clone, Debug, Default)]
pub(crate) struct StatementResult {
    pub sql: String,
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<Value>>,
    pub execution_time_ms: Option<i64>,
    pub source: Option<ResultSource>,
    pub has_runtime_binding: bool,
}

/// React `QueryExecState` (only the fields the drawer reads).
#[derive(Clone, Debug, Default)]
pub(crate) struct QueryExec {
    pub sql: String,
    pub results: Vec<StatementResult>,
    pub active_result_idx: usize,
    pub result_detail_row_index: Option<usize>,
    pub execution_time_ms: Option<i64>,
    pub error: String,
}

impl QueryExec {
    /// React `activeQueryResult`.
    pub(crate) fn active_result(&self) -> Option<&StatementResult> {
        self.results.get(self.active_result_idx)
    }
}

/// Which panel kind is active — the drawer's derivations branch on this.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PanelKind {
    Table,
    View,
    Query,
    /// KV panel or anything else: no row-detail derivation.
    Other,
}

/// Why a result cannot be written back (`GridReadOnlyReason`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReadOnlyReason {
    UnknownOrigin,
    AmbiguousOrigin,
    MultiTable,
    Aggregate,
    Distinct,
    SetOperation,
    DerivedSource,
    View,
    NoPrimaryKey,
    KeyNotSelected,
    SessionReadOnly,
    KeyValueNull,
    DuplicateIdentity,
    RowNotFound,
}

impl ReadOnlyReason {
    /// The `query.gridReadOnly.<reason>` i18n suffix.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::UnknownOrigin => "unknownOrigin",
            Self::AmbiguousOrigin => "ambiguousOrigin",
            Self::MultiTable => "multiTable",
            Self::Aggregate => "aggregate",
            Self::Distinct => "distinct",
            Self::SetOperation => "setOperation",
            Self::DerivedSource => "derivedSource",
            Self::View => "view",
            Self::NoPrimaryKey => "noPrimaryKey",
            Self::KeyNotSelected => "keyNotSelected",
            Self::SessionReadOnly => "sessionReadOnly",
            Self::KeyValueNull => "keyValueNull",
            Self::DuplicateIdentity => "duplicateIdentity",
            Self::RowNotFound => "rowNotFound",
        }
    }

    /// React `gridReadOnlyMessageKey(reason)`.
    pub(crate) fn message_key(self) -> String {
        format!("query.gridReadOnly.{}", self.as_str())
    }
}

/// Parse one reason from the backend's snake/camel wire word. Unknown words
/// fall back to `unknownOrigin`, exactly the default the drawer uses while a
/// verdict is still unresolved.
pub(crate) fn parse_reason(raw: Option<&str>) -> Option<ReadOnlyReason> {
    let raw = raw?;
    let reason = match raw {
        "unknownOrigin" | "unknown_origin" => ReadOnlyReason::UnknownOrigin,
        "ambiguousOrigin" | "ambiguous_origin" => ReadOnlyReason::AmbiguousOrigin,
        "multiTable" | "multi_table" => ReadOnlyReason::MultiTable,
        "aggregate" => ReadOnlyReason::Aggregate,
        "distinct" => ReadOnlyReason::Distinct,
        "setOperation" | "set_operation" => ReadOnlyReason::SetOperation,
        "derivedSource" | "derived_source" => ReadOnlyReason::DerivedSource,
        "view" => ReadOnlyReason::View,
        "noPrimaryKey" | "no_primary_key" => ReadOnlyReason::NoPrimaryKey,
        "keyNotSelected" | "key_not_selected" => ReadOnlyReason::KeyNotSelected,
        "sessionReadOnly" | "session_read_only" => ReadOnlyReason::SessionReadOnly,
        "keyValueNull" | "key_value_null" => ReadOnlyReason::KeyValueNull,
        "duplicateIdentity" | "duplicate_identity" => ReadOnlyReason::DuplicateIdentity,
        "rowNotFound" | "row_not_found" => ReadOnlyReason::RowNotFound,
        _ => ReadOnlyReason::UnknownOrigin,
    };
    Some(reason)
}

/// React `UNJUDGED`: a result nobody judged yet is read-only.
pub(crate) fn unjudged() -> ServerEditability {
    ServerEditability {
        writable: false,
        identity_columns: Vec::new(),
        editable_columns: Vec::new(),
        reason: None,
    }
}

/// React `ServerEditability` (identity/editable columns reduced to names).
#[derive(Clone, Debug, Default)]
pub(crate) struct ServerEditability {
    pub writable: bool,
    pub identity_columns: Vec<String>,
    pub editable_columns: Vec<String>,
    pub reason: Option<ReadOnlyReason>,
}

/// React `resolveResultWriteBack`. The origin channel is closed in this build,
/// so only the narrow `verified` mapping or an explicit session-read-only
/// refusal can decide the verdict.
pub(crate) fn resolve_result_write_back(
    legacy_verified: bool,
    server: &ServerEditability,
) -> bool {
    if !server.writable && server.reason == Some(ReadOnlyReason::SessionReadOnly) {
        return false;
    }
    if legacy_verified {
        return true;
    }
    false // originChannelWritable is permanently false (release channel closed)
}

/// React `resolveRowAddressability`: a NULL key or a duplicate identity on the
/// page makes the row unaddressable, whatever channel granted the write.
pub(crate) fn resolve_row_addressability(
    identity_columns: &[String],
    rows: &[Vec<Value>],
    columns: &[ColumnMeta],
    row_index: usize,
) -> Option<ReadOnlyReason> {
    if row_index >= rows.len() {
        return Some(ReadOnlyReason::RowNotFound);
    }
    if identity_columns.is_empty() {
        return None;
    }
    let row = &rows[row_index];
    let mut values: Vec<&Value> = Vec::with_capacity(identity_columns.len());
    for identity in identity_columns {
        let index = columns.iter().position(|column| column.name == *identity)?;
        let value = row.get(index)?;
        if value.is_null() {
            return Some(ReadOnlyReason::KeyValueNull);
        }
        values.push(value);
    }
    for (other_index, other) in rows.iter().enumerate() {
        if other_index == row_index {
            continue;
        }
        let same = identity_columns.iter().enumerate().all(|(position, identity)| {
            columns
                .iter()
                .position(|column| column.name == *identity)
                .and_then(|index| other.get(index))
                .is_some_and(|value| *value == *values[position])
        });
        if same {
            return Some(ReadOnlyReason::DuplicateIdentity);
        }
    }
    None
}

/// React `queryResultEditable`: non-query panels are always editable here; a
/// query result needs a granted write-back verdict.
pub(crate) fn query_result_editable(
    kind: PanelKind,
    result: Option<&StatementResult>,
    verdict: &ServerEditability,
) -> bool {
    if kind != PanelKind::Query {
        return true;
    }
    let Some(result) = result else {
        return false;
    };
    resolve_result_write_back(
        result.source.is_some_and(|source| source.writable_mapping_verified),
        verdict,
    )
}

/// React `queryReadOnlyReason` — the reason belongs to the drawer on this row.
pub(crate) fn query_read_only_reason(
    kind: PanelKind,
    result: Option<&StatementResult>,
    verdict: &ServerEditability,
    row_index: Option<usize>,
) -> Option<ReadOnlyReason> {
    if kind != PanelKind::Query {
        return None;
    }
    if !query_result_editable(kind, result, verdict) {
        return Some(verdict.reason.unwrap_or(ReadOnlyReason::UnknownOrigin));
    }
    let result = result?;
    let row_index = row_index?;
    resolve_row_addressability(&verdict.identity_columns, &result.rows, &result.columns, row_index)
}

/// A cell of the row-capped AI preview. Strings are cut to 500 chars; complex
/// values go through `JSON.stringify` and are cut as well.
pub(crate) fn preview_cell(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(text.chars().take(500).collect()),
        Value::Number(_) | Value::Bool(_) | Value::Null => value.clone(),
        other => Value::String(
            serde_json::to_string(other)
                .unwrap_or_default()
                .chars()
                .take(500)
                .collect(),
        ),
    }
}

/// The row-capped result preview handed to the assistant.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ResultPreview {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    pub row_count: usize,
    pub elapsed_ms: Option<i64>,
}

/// React `autoContext`: the active query's current facts as a frozen snapshot.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct AutoContext {
    pub editor_sql: Option<String>,
    pub selection: Option<String>,
    pub last_error: Option<String>,
    pub result_preview: Option<ResultPreview>,
}

impl AutoContext {
    fn is_empty(&self) -> bool {
        self.editor_sql.is_none()
            && self.selection.is_none()
            && self.last_error.is_none()
            && self.result_preview.is_none()
    }
}

/// Build the assistant's automatic context. Non-query panels get no snapshot.
pub(crate) fn build_auto_context(
    kind: PanelKind,
    exec: Option<&QueryExec>,
    editor_selection: &str,
) -> Option<AutoContext> {
    if kind != PanelKind::Query {
        return None;
    }
    let exec = exec?;
    let result = exec.active_result();
    let editor_sql = {
        let trimmed = exec.sql.trim();
        if !trimmed.is_empty() {
            Some(trimmed.to_owned())
        } else {
            result.map(|r| r.sql.clone()).filter(|sql| !sql.is_empty())
        }
    };
    let selection = {
        let trimmed = editor_selection.trim();
        if trimmed.is_empty() { None } else { Some(trimmed.to_owned()) }
    };
    let last_error = if exec.error.is_empty() { None } else { Some(exec.error.clone()) };
    let result_preview = result.and_then(|result| {
        if result.columns.is_empty() && result.rows.is_empty() {
            return None;
        }
        Some(ResultPreview {
            columns: result
                .columns
                .iter()
                .take(20)
                .map(|column| column.name.clone())
                .collect(),
            rows: result
                .rows
                .iter()
                .take(5)
                .map(|row| row.iter().take(20).map(preview_cell).collect())
                .collect(),
            row_count: result.rows.len(),
            elapsed_ms: exec.execution_time_ms.or(result.execution_time_ms),
        })
    });
    let context = AutoContext { editor_sql, selection, last_error, result_preview };
    if context.is_empty() { None } else { Some(context) }
}

/// Resolve the exec the drawer must judge: the focused pane, falling back to
/// the panel's default pane entry. This is the split-pane source rule — reading
/// the default pane instead made the drawer judge a different result set.
pub(crate) fn resolve_active_exec<'a>(
    panel_id: &str,
    focused_pane: Option<&str>,
    panes: &'a HashMap<String, QueryExec>,
) -> Option<&'a QueryExec> {
    let key = pane_key(panel_id, resolve_focused_pane_id(focused_pane));
    panes.get(&key).or_else(|| panes.get(panel_id))
}

/// React `useResizable` bounds for the AI sidebar.
pub(crate) fn clamp_sidebar_width(width: f32) -> f32 {
    width.clamp(240.0, 600.0)
}
