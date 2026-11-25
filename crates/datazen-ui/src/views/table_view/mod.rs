//! `TableView`, ported from `packages/frontend/src/windows/connection/TableView.tsx`.
//!
//! React is a 1445-line host wiring a data grid, cell editors, insert drafts,
//! a dirty index, filter operators, a navigation guard, Quick Look and several
//! banners. The Rune port keeps the *decisions* as pure functions over a
//! host-owned snapshot and emits [`TableViewAction`]s through an injected port.
//! No command, timer or thread lives here.
//!
//! ## Removal stage
//!
//! No production host mounts this view yet; the table host will own the
//! snapshot and apply the actions. Per-item allow annotations carry that stage.

mod logic;
pub mod draft_rows;
mod view;

pub use logic::*;
#[allow(unused_imports)] // host/test seam; consumer: the table host + tests; drop when wired.
pub use view::build;

use std::collections::BTreeMap;
use rune_kit::SortDir;

/// One result column (React `ColumnSchema`).
#[allow(dead_code)] // VM mirror; supplied by the host result feed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ColumnMeta {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

/// A cell as the grid presents it.
#[allow(dead_code)] // VM mirror; supplied by the host result feed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellValue {
    Null,
    Text(String),
    Number(String),
    Bool(bool),
    Json(String),
    Blob(Vec<u8>),
}

impl CellValue {
    #[allow(dead_code)] // pure helper; consumed by the grid and tests.
    pub fn display(&self) -> String {
        match self {
            Self::Null => "NULL".to_owned(),
            Self::Text(value) | Self::Number(value) | Self::Json(value) => value.clone(),
            Self::Bool(value) => value.to_string(),
            Self::Blob(bytes) => bytes.iter().take(8).map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "),
        }
    }
}

/// One staged cell change (`DirtyCell` in React).
#[allow(dead_code)] // VM mirror; consumed by the dirty index and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirtyCell {
    pub column: String,
    pub original: CellValue,
    pub next: CellValue,
}

/// One dirty row entry.
#[allow(dead_code)] // VM mirror; consumed by the grid and pending bar.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirtyRow {
    pub cells: Vec<DirtyCell>,
    pub deleted: bool,
}

/// One insert draft row (React `draftInserts`).
#[allow(dead_code)] // VM mirror; consumed by insert actions and tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DraftRow {
    pub id: String,
    pub values: BTreeMap<String, CellValue>,
}

/// Why a cell cannot be edited (React `CellReadOnlyReason`).
#[allow(dead_code)] // VM mirror; supplied by the mount point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReadOnlyReason {
    #[default]
    ConnectionReadOnly,
    DriverCapability,
    View,
    NoPrimaryKey,
    AmbiguousRowIdentity,
    AutoIncrement,
    Generated,
    BinaryColumn,
    LargeValue,
    TypeNotEditable,
}

/// Host-owned projection of the table panel snapshot.
#[allow(dead_code)] // VM surface; owned by the table host.
#[derive(Clone, Debug, Default)]
pub struct TableViewVm {
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<CellValue>>,
    pub visible_columns: Option<Vec<String>>,
    pub column_order: Option<Vec<String>>,
    pub sort: Option<(usize, SortDir)>,
    pub highlighted_row: Option<usize>,
    pub editing_cell: Option<(usize, String)>,
    /// Whether the editor currently holds an invalid draft (navigation guard).
    pub invalid_draft: bool,
    pub dirty: BTreeMap<usize, DirtyRow>,
    pub drafts: Vec<DraftRow>,
    pub active_draft: Option<String>,
    pub insert_notice: Option<String>,
    pub inserted_row: Option<usize>,
    pub read_only: bool,
    pub read_only_reason: Option<ReadOnlyReason>,
    pub has_primary_key: bool,
    pub safe_mode: bool,
    pub confirm_on_delete: bool,
    pub driver_type: Option<String>,
    /// Operators the driver declares; empty means "shared set" in React.
    pub declared_operators: Vec<String>,
    pub like_case_insensitive: bool,
    pub raw_where_supported: bool,
    pub error: Option<String>,
    pub has_work: bool,
    pub quick_look: Option<(usize, String)>,
    pub quick_look_editable: bool,
    pub show_read_only_tip: Option<ReadOnlyReason>,
    /// The 3000ms read-only-tip deadline, on the host clock (no own timer).
    pub read_only_tip_until: Option<f64>,
    pub now: f64,
}

/// Every host intent this view can emit.
#[allow(dead_code)] // host port; removed with the table host wiring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableViewAction {
    Load,
    Reload,
    Retry,
    SetSort(usize),
    ToggleColumn(String),
    SetPage(usize),
    StageCell { row: usize, column: String, value: CellValue },
    StartEdit { row: usize, column: String },
    CancelEdit,
    DeleteRows(Vec<usize>),
    ConfirmDelete(Vec<usize>),
    AddDraft,
    DuplicateDraft(String),
    DiscardDraft(String),
    CommitPending,
    PreviewPending,
    RollbackPending,
    RestoreRow(usize),
    OpenQuickLook { row: usize, column: String },
    CloseQuickLook,
    CommitQuickLook { row: usize, column: String, value: CellValue },
    ShowReadOnlyTip(ReadOnlyReason),
    PreviewSql,
    ExportFiltered,
    EditInSql,
}

pub type ActionFn<S> = std::rc::Rc<dyn Fn(&mut S, TableViewAction) + 'static>;
