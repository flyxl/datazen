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
mod view;

pub(crate) use logic::*;
#[allow(unused_imports)] // host/test seam; consumer: the table host + tests; drop when wired.
pub(crate) use view::build;

use std::collections::BTreeMap;
use rune_kit::SortDir;

/// One result column (React `ColumnSchema`).
#[allow(dead_code)] // VM mirror; supplied by the host result feed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ColumnMeta {
    pub(crate) name: String,
    pub(crate) data_type: String,
    pub(crate) nullable: bool,
}

/// A cell as the grid presents it.
#[allow(dead_code)] // VM mirror; supplied by the host result feed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CellValue {
    Null,
    Text(String),
    Number(String),
    Bool(bool),
    Json(String),
    Blob(Vec<u8>),
}

impl CellValue {
    #[allow(dead_code)] // pure helper; consumed by the grid and tests.
    pub(crate) fn display(&self) -> String {
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
pub(crate) struct DirtyCell {
    pub(crate) column: String,
    pub(crate) original: CellValue,
    pub(crate) next: CellValue,
}

/// One dirty row entry.
#[allow(dead_code)] // VM mirror; consumed by the grid and pending bar.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DirtyRow {
    pub(crate) cells: Vec<DirtyCell>,
    pub(crate) deleted: bool,
}

/// One insert draft row (React `draftInserts`).
#[allow(dead_code)] // VM mirror; consumed by insert actions and tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DraftRow {
    pub(crate) id: String,
    pub(crate) values: BTreeMap<String, CellValue>,
}

/// Why a cell cannot be edited (React `CellReadOnlyReason`).
#[allow(dead_code)] // VM mirror; supplied by the mount point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ReadOnlyReason {
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
pub(crate) struct TableViewVm {
    pub(crate) columns: Vec<ColumnMeta>,
    pub(crate) rows: Vec<Vec<CellValue>>,
    pub(crate) visible_columns: Option<Vec<String>>,
    pub(crate) column_order: Option<Vec<String>>,
    pub(crate) sort: Option<(usize, SortDir)>,
    pub(crate) highlighted_row: Option<usize>,
    pub(crate) editing_cell: Option<(usize, String)>,
    /// Whether the editor currently holds an invalid draft (navigation guard).
    pub(crate) invalid_draft: bool,
    pub(crate) dirty: BTreeMap<usize, DirtyRow>,
    pub(crate) drafts: Vec<DraftRow>,
    pub(crate) active_draft: Option<String>,
    pub(crate) insert_notice: Option<String>,
    pub(crate) inserted_row: Option<usize>,
    pub(crate) read_only: bool,
    pub(crate) read_only_reason: Option<ReadOnlyReason>,
    pub(crate) has_primary_key: bool,
    pub(crate) safe_mode: bool,
    pub(crate) confirm_on_delete: bool,
    pub(crate) driver_type: Option<String>,
    /// Operators the driver declares; empty means "shared set" in React.
    pub(crate) declared_operators: Vec<String>,
    pub(crate) like_case_insensitive: bool,
    pub(crate) raw_where_supported: bool,
    pub(crate) error: Option<String>,
    pub(crate) has_work: bool,
    pub(crate) quick_look: Option<(usize, String)>,
    pub(crate) quick_look_editable: bool,
    pub(crate) show_read_only_tip: Option<ReadOnlyReason>,
    /// The 3000ms read-only-tip deadline, on the host clock (no own timer).
    pub(crate) read_only_tip_until: Option<f64>,
    pub(crate) now: f64,
}

/// Every host intent this view can emit.
#[allow(dead_code)] // host port; removed with the table host wiring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TableViewAction {
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

pub(crate) type ActionFn<S> = std::rc::Rc<dyn Fn(&mut S, TableViewAction) + 'static>;
