//! Table-data connection-state tools, ported from React
//! `packages/frontend/src/stores/tableData/connectionState.ts` (T041).
//!
//! The pure helpers the table-data store builds on: turning read rows into
//! records, normalising a cell value, pulling a message out of a caught
//! error, the empty panel state, and the panel's immutable load target.
//! Nothing here renders, stores, or calls IPC — the store that owns a panel
//! calls these and keeps the results.
//!
//! # Dependency status at port time (T041)
//!
//! None of the four types this file imports from React had a Rune version
//! yet, so this module carries the minimal mirrors it needs, each marked
//! with the React source and the port that should replace it:
//!
//! * `TableState` — mirror of `stores/tableData/types.ts`. The full port of
//!   that file replaces this struct; until then the fields below are
//!   field-for-field the React ones.
//! * `EMPTY_GRID_SELECTION` — mirror of `stores/tableData/gridSelection.ts`
//!   (`EMPTY_GRID_SELECTION`, the frozen empty selection). The grid-selection
//!   port replaces the `GridSelection` / `CellCoord` / `CellRange` mirrors
//!   and this constant with its own.
//! * `EMPTY_KEYSET_PAGING_STATE` — mirror of `lib/keysetPaging.ts`
//!   (`emptyKeysetPagingState`). The keyset-paging port replaces the
//!   `KeysetPagingState` family (`PagingMode`, `OrderColumn`,
//!   `KeysetCursor`, `CursorValue`, `KeysetUnavailableReason`) and this
//!   constant with its own.
//! * `TableChangeContext` — mirror of `lib/tableChanges.ts`. The
//!   table-changes port replaces it, together with `PendingRowChange`,
//!   `PendingInsert`, `RowChangePlan` and `PendingStatus`.
//!
//! `Value` is **not** re-mirrored here: the established Rune mirror of the
//! frontend `Value` (`string | number | boolean | null | object | array`)
//! is `serde_json::Value`, already used by the T002 `CellRenderer` port, so
//! this module reuses it. `ColumnSchema` is mirrored here (the React
//! `types/connection.ts` shape); a shared schema-types port should replace
//! it. `CountStrategy` mirrors the backend-client `'exact' | 'estimated' |
//! 'unsupported'`.
//!
//! # TypeScript → Rust mapping notes
//!
//! * `Value | null` is `serde_json::Value`: `null` is already a variant, so
//!   a missing cell is `Value::Null`, never a second `Option` layer.
//! * `unknown` has no Rust form. `to_cell_value` takes `Option<Value>` —
//!   `None` is TS `null`/`undefined`, `Some(v)` is any other value — and
//!   `extract_error_message` takes an [`ErrorMessageSource`] enum, one
//!   variant per shape the TS function distinguishes (bare string, `Error`,
//!   object with a `message` field), plus `None` for everything else.
//! * `Map` is `HashMap`, `Set` is `HashSet`; `Record<string, unknown>` is
//!   `HashMap<String, Value>`. The grid renders cells by iterating
//!   `columns` and reading `record[column]`, so record iteration order is
//!   not load-bearing and a hash map is faithful.
//! * React optional booleans (`isPrimaryKey?`, `insertMarked?`) collapse to
//!   `bool`: an absent flag is `false`, exactly as every React reader
//!   treats it.
//! * `?? null` is `Option` passthrough: [`build_table_context`] copies the
//!   target's optional fields straight into the context.
//! * The shared React constants are `const`s here too, but a Rust `const`
//!   is copied on use, not aliased: each panel gets its own maps and
//!   vectors, and mutating one panel's state can never leak into another
//!   panel or into the constant. React relies on the same discipline
//!   (panels replace the object instead of mutating the shared one).
#![allow(dead_code)] // consumers: tableData store port (T041+); TableState lands with the store

use std::collections::{HashMap, HashSet};

use serde_json::Value;

/// One column of a table schema.
///
/// Mirror of `ColumnSchema` (`packages/frontend/src/types/connection.ts`).
/// Only `name` is read by this module (`rows_to_records`); the rest is
/// carried so `TableState.columns` is the whole schema, not a projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ColumnSchema {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub is_primary_key: bool,
    pub is_auto_increment: bool,
    pub comment: Option<String>,
}

/// How much `totalRows` may be trusted.
///
/// Mirror of the backend-client `CountStrategy`: an exact number, an
/// estimate that has to be shown as "about N rows", and "this database
/// cannot say".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CountStrategy {
    #[default]
    Exact,
    Estimated,
    Unsupported,
}

/// The panel's temporary override of the default order.
///
/// `Inherit` follows the global setting; the other values are this panel's
/// own choice and are not persisted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DefaultOrderOverride {
    #[default]
    Inherit,
    PrimaryKeyAsc,
    PrimaryKeyDesc,
    None,
}

/// The request-wide logical operator joining a filter list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FilterLogic {
    #[default]
    And,
    Or,
}

/// Which editor pane the filter panel shows (DBR-04).
///
/// The modes keep their own drafts, so switching between them never
/// discards unapplied work; the applied filter itself is always at most one
/// channel at a time — `filters` for structured/Any Column, `raw_where` for
/// Raw SQL.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FilterMode {
    #[default]
    Structured,
    AnyColumn,
    RawSql,
}

/// One applied filter condition.
///
/// Mirror of `FilterCondition` (`packages/frontend/src/types/filters.ts`).
/// `operator` is the React operator id (`eq`, `between`, …) as a string,
/// following the T002-adjacent `FilterConditionChip` precedent; the
/// `FilterOperator` enum lands with the filters port. `value` is absent
/// for operators that take none (`isEmpty`); `connector` joins this
/// condition to the one before it, and falls back to the request-wide
/// operator when absent.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FilterCondition {
    pub column: String,
    pub operator: String,
    pub value: Option<Value>,
    pub connector: Option<FilterLogic>,
}

/// One sort: the column and its direction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SortCondition {
    pub column: String,
    pub descending: bool,
}

/// How a page was read.
///
/// Mirror of the host's `PagingMode`: by offset (the page starts
/// `page × page_size` rows in) or by seek (the page starts after a value
/// boundary that was read from a real row).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PagingMode {
    #[default]
    Offset,
    Keyset,
}

/// Why a page could not be read by seek.
///
/// Mirror of the host's `KeysetUnavailableReason`. Every one of them is a
/// **degradation**, not an error: the page is read by offset and the reason
/// says why.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeysetUnavailableReason {
    DriverUnsupported,
    NoStableOrder,
    NullableKeyColumn,
    NullBoundary,
    PredicateUnavailable,
    NoPaginationClause,
    StaleCursor,
    NotAdjacent,
}

/// One boundary value, **with its tag**.
///
/// Mirror of `CursorValue` (`packages/frontend/src/types/keyset.ts`). The
/// tag is load-bearing: the host's `Value` is untagged, so `[1, 2, 3]` reads
/// as bytes *or* as JSON, and a boundary travels "host → front end → host"
/// — untagged it would change its variant on the way back, and the seek
/// would compare against the wrong literal with no error anywhere.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CursorValue {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Timestamp(String),
    Json(Value),
}

/// One ordering column and its direction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OrderColumn {
    pub column: String,
    pub descending: bool,
}

/// One visited page and the boundary it started at.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KeysetCursor {
    /// The boundary the page started at; `None` for the first page.
    pub boundary: Option<Vec<CursorValue>>,
    /// The sort that was in force when the boundary was taken.
    pub order_signature: String,
    /// The filters that were in force when the boundary was taken.
    pub filter_signature: String,
}

/// The paging state of one panel.
///
/// Mirror of `KeysetPagingState` (`packages/frontend/src/lib/keysetPaging.ts`).
/// `page` stays the ordinal it has always been — "the Nth page" — and this
/// state says how that page is **read**: by offset, or by seek from a
/// boundary that was really read. There is no second page number.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KeysetPagingState {
    /// The strategy the back end reported.
    pub mode: PagingMode,
    /// The stable total order the back end reported.
    pub order_columns: Vec<OrderColumn>,
    /// The boundaries of the pages that were visited.
    /// `stack[i]` is the boundary of page `anchor_page + i`.
    pub stack: Vec<KeysetCursor>,
    /// The page `stack[0]` belongs to.
    pub anchor_page: usize,
    /// Why the page was not read by seek, when it was not.
    pub unavailable: Option<KeysetUnavailableReason>,
    /// Rows were deleted while the user was paging.
    pub drift_suspected: bool,
    /// The boundary of the page **after** the one on screen. `None` when
    /// the back end gave none — the last page, or a page read by offset.
    pub next_after_key: Option<Vec<CursorValue>>,
}

/// The state of a panel that has never paged by seek.
///
/// Mirror of `emptyKeysetPagingState` (`lib/keysetPaging.ts`).
pub(crate) const EMPTY_KEYSET_PAGING_STATE: KeysetPagingState = KeysetPagingState {
    mode: PagingMode::Offset,
    order_columns: Vec::new(),
    stack: Vec::new(),
    anchor_page: 0,
    unavailable: None,
    drift_suspected: false,
    next_after_key: None,
};

/// The immutable values that identify a row for an UPDATE or DELETE.
///
/// Mirror of `TableChangeContext` (`packages/frontend/src/lib/tableChanges.ts`).
/// A context is only safe for a write once every routing field is known. A
/// null schema is meaningful (drivers without schemas or an explicitly
/// unqualified table), while a null database is not: the current session
/// must be pinned to a concrete database before a pending change can be
/// committed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TableChangeContext {
    pub connection_id: Option<String>,
    pub db_session_id: String,
    pub driver_type: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub table: String,
}

/// The typed write expressions a grid may stage (DBR-05) — the mirror of
/// the back end's closed set (`WriteExpressionId` from the backend-client
/// wire). A staged expression renders the driver's own native SQL at commit
/// time; it is never a literal value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WriteExpressionId {
    CurrentTimestamp,
}

/// One staged change to an existing row.
///
/// Mirror of `PendingRowChange` (`lib/tableChanges.ts`). The write intent
/// of a column lives in the **membership** of `current_values` for drafts,
/// and a column absent from `changed_columns` keeps the database default.
/// `insert_marked` marks an entry as a **new row draft** rather than a
/// change to an existing row: such a row has no identity yet, so
/// `changed_columns` is the exact INSERT column list.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PendingRowChange {
    /// UI-only hint for keeping a PK edit associated with its original row.
    pub row_index: Option<usize>,
    pub row_identity: HashMap<String, Value>,
    pub original_values: HashMap<String, Value>,
    pub current_values: HashMap<String, Value>,
    pub changed_columns: Vec<String>,
    pub delete_marked: bool,
    pub insert_marked: bool,
    /// The changed columns staged as a typed write expression (DBR-05).
    pub expression_values: Option<HashMap<String, WriteExpressionId>>,
}

/// One new-row draft (front-end UI shape; it never goes on the wire).
///
/// Mirror of `PendingInsert` (`lib/tableChanges.ts`). A present key means
/// "written"; `Value::Null` means SQL `NULL`; a key absent from `values`
/// means **not written** and the database keeps its own default
/// (auto-increment, generated column, `DEFAULT now()`, trigger). "Not
/// written" and "written as NULL" are different statements, so clearing a
/// draft cell removes the key instead of storing null.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PendingInsert {
    /// UI association key: React key plus the draft row's `data-draft-id`.
    pub draft_id: String,
    /// Per-column write intent.
    pub values: HashMap<String, Value>,
}

/// One statement of a change plan.
///
/// Mirror of `PlannedStatement` (`lib/tableChanges.ts`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlannedStatement {
    pub row_identity: HashMap<String, Value>,
    pub original_values: HashMap<String, Value>,
    pub current_values: HashMap<String, Value>,
    pub changed_columns: Vec<String>,
    /// The changed columns staged as a typed write expression (DBR-05).
    pub expression_values: Option<HashMap<String, WriteExpressionId>>,
    /// Driver-rendered SQL preview. This is not a wire-level SQL audit record.
    pub sql_template: String,
    /// Human-readable summaries of the values used by the statement.
    pub parameter_summary: Vec<String>,
}

/// One warning attached to a change plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChangeWarning {
    pub code: String,
    pub message: String,
    pub severity: ChangeSeverity,
}

/// The severity of a [`ChangeWarning`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChangeSeverity {
    Info,
    Warning,
    Error,
}

/// A staged change plan: the exact statements a commit will run.
///
/// Mirror of `RowChangePlan` (`lib/tableChanges.ts`). `inserts` is always
/// present — a plan without inserts carries an empty list, never a missing
/// key.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RowChangePlan {
    pub plan_id: String,
    pub fingerprint: String,
    pub config_revision: Option<u64>,
    pub table: TableChangeContext,
    pub updates: Vec<PlannedStatement>,
    pub deletes: Vec<PlannedStatement>,
    /// New-row statements in request order: insertion order decides
    /// auto-increment.
    pub inserts: Vec<PlannedStatement>,
    pub warnings: Vec<ChangeWarning>,
}

/// The staged-change lifecycle of a panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PendingStatus {
    #[default]
    Idle,
    Previewing,
    Committing,
}

/// One cell coordinate: a 0-based row index inside the current page and a
/// column **name** (names stay stable across `visible_columns` changes and
/// column moves, while a column index does not).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CellCoord {
    pub row_index: usize,
    pub column_name: String,
}

/// One cell range; both corners, their order meaningless (normalised by the
/// grid-selection helpers).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CellRange {
    pub start: CellCoord,
    pub end: CellCoord,
}

/// The mode of a [`GridSelection`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum GridSelectionMode {
    #[default]
    None,
    Cell,
    Row,
}

/// Cell/region selection.
///
/// Mirror of `GridSelection` (`stores/tableData/gridSelection.ts`). The
/// store is the only writer; `None` is the default. The selection owns no
/// row set: `mode: Row` reports `TableState.selected_rows` verbatim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GridSelection {
    /// Anchor corner, fixed on pointerdown / when Shift-extension starts.
    pub anchor: Option<CellCoord>,
    /// Moving corner; follows the pointer drag or Shift+arrow.
    pub focus: Option<CellCoord>,
    /// Extra non-contiguous single-cell ranges (Cmd/Ctrl+click).
    pub extra_ranges: Vec<CellRange>,
    pub mode: GridSelectionMode,
}

/// The frozen empty selection shared by every "no selection" site.
///
/// Mirror of `EMPTY_GRID_SELECTION` (`gridSelection.ts`). Never write into
/// it: all transitions build new selections — and as a Rust `const` this
/// one is copied on use, so there is no shared backing to write into.
pub(crate) const EMPTY_GRID_SELECTION: GridSelection = GridSelection {
    anchor: None,
    focus: None,
    extra_ranges: Vec::new(),
    mode: GridSelectionMode::None,
};

/// The cell being edited, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EditingCell {
    pub row: usize,
    pub col: String,
}

/// Table-data state for one panel (tab).
///
/// Mirror of `TableState` (`stores/tableData/types.ts`). The panel is the
/// unit of lifetime and of cache scope: page/filters/selection/staged edits
/// belong to the tab that set them, never to the session, so two tabs on the
/// same table never overwrite each other.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TableState {
    pub context: Option<TableChangeContext>,
    pub columns: Vec<ColumnSchema>,
    pub rows: Vec<HashMap<String, Value>>,
    pub total_rows: usize,
    /// How much `total_rows` may be trusted.
    ///
    /// The three states are the whole point of the count contract: an exact
    /// number, an estimate that has to be shown as "about N rows", and "this
    /// database cannot say". A response that carries no kind keeps the value
    /// this panel already had — that is what a paged re-read sends.
    pub total_rows_kind: CountStrategy,
    /// The panel's temporary override of the default order. `Inherit`
    /// follows the global setting; the other values are this panel's own
    /// choice and are not persisted.
    pub default_order_override: DefaultOrderOverride,
    pub page: usize,
    pub page_size: usize,
    /// How this panel is paged.
    ///
    /// `page` stays the ordinal it has always been — "the Nth page" — and
    /// the keyset state says how that page is **read**: by offset, or by
    /// seek from a boundary that was really read. There is no second page
    /// number.
    pub keyset: KeysetPagingState,
    /// The page whose row count was already recomputed after a drift.
    ///
    /// Rows deleted while the user was paging make the count stale, and the
    /// count is then read again — once per page. This remembers which page
    /// that was, so a table that keeps shrinking cannot make the panel read
    /// itself in a loop.
    pub drift_recount_page: Option<usize>,
    /// Applied filters used by queries.
    pub filters: Vec<FilterCondition>,
    pub filter_logic: FilterLogic,
    pub draft_filters: Vec<FilterCondition>,
    pub draft_filter_logic: FilterLogic,
    pub filter_panel_open: bool,
    /// Which editor pane the filter panel shows (DBR-04). The modes keep
    /// their own drafts, so switching between them never discards unapplied
    /// work; the applied filter itself is always at most one channel at a
    /// time — `filters` for structured/Any Column, `raw_where` for Raw SQL.
    pub filter_mode: FilterMode,
    /// The Any Column draft: the single value, kept across mode switches.
    pub any_column_draft: String,
    /// The Raw SQL draft: the unapplied predicate, kept across mode switches.
    pub raw_where_draft: String,
    /// The applied raw `WHERE` predicate, or `None` when the channel is idle.
    pub raw_where: Option<String>,
    pub sorts: Vec<SortCondition>,
    /// The panel's explicit column **render order** (DB-12): `None` follows
    /// the schema order. It is a permutation hint over the *displayed*
    /// columns — membership stays with `visible_columns`, so the two never
    /// answer the same question twice. Columns missing from the list keep
    /// their schema position at the end; names that no longer exist are
    /// ignored.
    pub column_order: Option<Vec<String>>,
    /// How many leading displayed columns are frozen (DB-12): they stay put
    /// while the grid scrolls horizontally. `0` — the default — freezes
    /// nothing. Panel-scoped on purpose: only widths and order persist.
    pub frozen_column_count: usize,
    /// Changes staged against this table; the map key is the stable PK identity.
    pub pending_changes: HashMap<String, PendingRowChange>,
    /// New-row drafts, keyed by their draft id.
    ///
    /// Deliberately **not** part of `pending_changes`: every consumer of
    /// that map assumes the row already exists and has a primary-key
    /// identity, while a draft has none (and tables without a primary key
    /// may still be inserted into). The two collections only meet at the
    /// wire boundary, where a draft is turned into an `insertMarked` row
    /// change.
    pub draft_inserts: HashMap<String, PendingInsert>,
    /// Creation order of the drafts; it decides the plan order and the
    /// render order.
    pub draft_insert_order: Vec<String>,
    /// Identity key of the row highlighted right after an insert commit;
    /// `None` when none.
    pub highlighted_new_row_key: Option<String>,
    /// Ephemeral row-index anchor; it only points back to an original
    /// identity key.
    pub row_identity_anchors: HashMap<usize, String>,
    pub preview_plan: Option<RowChangePlan>,
    pub pending_status: PendingStatus,
    pub selected_rows: HashSet<usize>,
    pub last_selected_index: Option<usize>,
    /// Cell/region selection. The store is the only writer; `None` is the
    /// default.
    pub grid_selection: GridSelection,
    pub editing_cell: Option<EditingCell>,
    /// The candidate values of the enumeration columns, keyed by column
    /// name (DB-05).
    ///
    /// The cache is filled **only** when the user asks for the candidates:
    /// opening an editor must never query anything. It lives in the panel
    /// because the candidates belong to a table.
    pub enum_candidates: HashMap<String, Vec<String>>,
    /// Row highlighted by the detail drawer.
    pub detail_row_index: Option<usize>,
    pub loading: bool,
    /// Monotonic revision of the latest requested page/filter/sort state.
    pub request_revision: u64,
    /// Revision currently represented by the in-flight request, if any.
    pub loading_revision: Option<u64>,
    pub error: Option<String>,
    pub visible_columns: Option<Vec<String>>,
}

impl Default for TableState {
    /// The empty panel: exactly [`empty_table_state`] with no context.
    fn default() -> Self {
        empty_table_state(None)
    }
}

/// The panel's immutable load target, as carried in every request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TableTarget {
    pub db_session_id: String,
    pub table: String,
    pub connection_id: Option<String>,
    pub driver_type: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
}

/// Build the rows of `columns` into records keyed by column name.
///
/// React `rowsToRecords`: a cell is `row[i]`, so a row shorter than the
/// columns leaves the missing cells `null`, and a row longer than the
/// columns drops the surplus values — the record only ever has the
/// columns' keys, in column order.
pub(crate) fn rows_to_records(
    columns: &[ColumnSchema],
    rows: &[Vec<Value>],
) -> Vec<HashMap<String, Value>> {
    rows.iter()
        .map(|row| {
            columns
                .iter()
                .enumerate()
                .map(|(i, col)| {
                    let value = row.get(i).cloned().unwrap_or(Value::Null);
                    (col.name.clone(), value)
                })
                .collect()
        })
        .collect()
}

/// Normalise an arbitrary value to a cell value.
///
/// React `toCellValue`: `null`/`undefined` become `null`, anything else
/// passes through as a `Value`. The Rust caller holds `None` for the
/// missing cases and `Some(value)` otherwise, so this is the collapse of
/// that distinction into `Value::Null`.
pub(crate) fn to_cell_value(val: Option<Value>) -> Value {
    val.unwrap_or(Value::Null)
}

/// One of the shapes `extract_error_message` distinguishes.
///
/// TypeScript hands the function an `unknown`, and the branch that fires
/// depends on what it actually is: a bare string, an `Error` instance, or
/// an object carrying a string `message` field. Rust has no `unknown`, so
/// the caller says which shape it holds — `None` is everything else.
#[derive(Debug)]
pub(crate) enum ErrorMessageSource<'a> {
    /// A bare string (`typeof e === 'string'`).
    Text(&'a str),
    /// An error value (`e instanceof Error`); its message is its `Display`.
    Error(&'a dyn std::error::Error),
    /// An object carrying a string `message` field.
    MessageField(&'a str),
}

/// Pull a display message out of a caught error.
///
/// React `extractErrorMessage`, branch order preserved: a non-blank string
/// wins first, then an `Error`'s message, then an object's `message`
/// field, and the fallback otherwise. A blank string — empty or
/// whitespace-only — is **not** a message: it falls through to the
/// fallback. A message that passes is returned verbatim, untrimmed; only
/// the blank check looks at the trimmed form.
pub(crate) fn extract_error_message(
    source: Option<ErrorMessageSource<'_>>,
    fallback: &str,
) -> String {
    match source {
        Some(ErrorMessageSource::Text(text)) if !text.trim().is_empty() => text.to_owned(),
        Some(ErrorMessageSource::Error(error)) => {
            let message = error.to_string();
            if message.trim().is_empty() {
                fallback.to_owned()
            } else {
                message
            }
        }
        Some(ErrorMessageSource::MessageField(message)) if !message.trim().is_empty() => {
            message.to_owned()
        }
        _ => fallback.to_owned(),
    }
}

/// The empty state of a panel that has never read a row.
///
/// React `emptyTableState`, field-for-field. The three design reasons from
/// the React comments are kept at their fields: a panel that has never read
/// a row has no count to doubt yet (`total_rows_kind` is `Exact`); a panel
/// that has never paged by seek reads by offset (`keyset` is
/// [`EMPTY_KEYSET_PAGING_STATE`]); and the panel starts in the structured
/// editor with both new-mode drafts empty and the raw channel idle (DBR-04,
/// `filter_mode` is `Structured`).
pub(crate) fn empty_table_state(context: Option<TableChangeContext>) -> TableState {
    TableState {
        context,
        columns: Vec::new(),
        rows: Vec::new(),
        total_rows: 0,
        // A panel that has never read a row has no count to doubt yet: the
        // first response says how much of it may be trusted.
        total_rows_kind: CountStrategy::Exact,
        default_order_override: DefaultOrderOverride::Inherit,
        page: 0,
        page_size: 50,
        // A panel that has never paged by seek reads by offset — which is what
        // every read did before the keyset contract existed.
        keyset: EMPTY_KEYSET_PAGING_STATE,
        drift_recount_page: None,
        filters: Vec::new(),
        filter_logic: FilterLogic::And,
        draft_filters: Vec::new(),
        draft_filter_logic: FilterLogic::And,
        filter_panel_open: false,
        // DBR-04: the panel starts in the structured editor, with both new-mode
        // drafts empty and the raw channel idle.
        filter_mode: FilterMode::Structured,
        any_column_draft: String::new(),
        raw_where_draft: String::new(),
        raw_where: None,
        sorts: Vec::new(),
        column_order: None,
        frozen_column_count: 0,
        pending_changes: HashMap::new(),
        draft_inserts: HashMap::new(),
        draft_insert_order: Vec::new(),
        highlighted_new_row_key: None,
        row_identity_anchors: HashMap::new(),
        preview_plan: None,
        pending_status: PendingStatus::Idle,
        selected_rows: HashSet::new(),
        last_selected_index: None,
        grid_selection: EMPTY_GRID_SELECTION,
        editing_cell: None,
        enum_candidates: HashMap::new(),
        detail_row_index: None,
        loading: false,
        request_revision: 0,
        loading_revision: None,
        error: None,
        visible_columns: None,
    }
}

/// Build a [`TableChangeContext`] from a load target.
///
/// React `buildTableContext`: the five optional fields map with `?? null`,
/// so an absent field is a null in the context, never a default.
pub(crate) fn build_table_context(target: &TableTarget) -> TableChangeContext {
    TableChangeContext {
        connection_id: target.connection_id.clone(),
        db_session_id: target.db_session_id.clone(),
        driver_type: target.driver_type.clone(),
        database: target.database.clone(),
        schema: target.schema.clone(),
        table: target.table.clone(),
    }
}
