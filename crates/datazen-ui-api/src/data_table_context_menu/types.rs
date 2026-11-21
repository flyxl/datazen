use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const CONTEXT_MENU_DOM_GAP: [&str; 4] = [
    "native contextmenu event",
    "data-dt-row/col DOM lookup",
    "showNativeContextMenu",
    "navigator.clipboard.writeText",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardFormat {
    Tsv,
    Csv,
    Json,
    Plain,
    Html,
    Markdown,
}

impl ClipboardFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            ClipboardFormat::Tsv => "tsv",
            ClipboardFormat::Csv => "csv",
            ClipboardFormat::Json => "json",
            ClipboardFormat::Plain => "plain",
            ClipboardFormat::Html => "html",
            ClipboardFormat::Markdown => "markdown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetValueActionId {
    SetNull,
    SetEmpty,
    SetDefault,
    RevertCell,
    PickDate,
    LoadBinaryFromFile,
    SetCurrentDatetime,
}

impl SetValueActionId {
    pub fn as_str(self) -> &'static str {
        match self {
            SetValueActionId::SetNull => "setNull",
            SetValueActionId::SetEmpty => "setEmpty",
            SetValueActionId::SetDefault => "setDefault",
            SetValueActionId::RevertCell => "revertCell",
            SetValueActionId::PickDate => "pickDate",
            SetValueActionId::LoadBinaryFromFile => "loadBinaryFromFile",
            SetValueActionId::SetCurrentDatetime => "setCurrentDatetime",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SetValueState {
    pub can_null: bool,
    pub can_empty: bool,
    pub row_kind_insert: bool,
    pub has_pending: bool,
    pub can_load_candidates: bool,
    pub can_pick_date: bool,
    pub can_load_binary: bool,
    pub can_set_current_datetime: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowState {
    Deleted,
    Other,
}

pub use crate::SortCondition;

#[derive(Clone, Debug, PartialEq)]
pub enum HitTarget {
    Cell {
        row_index: usize,
        column_name: String,
    },
    Header {
        column_name: String,
    },
    Plain,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MenuEvent {
    pub target: HitTarget,
    pub client_x: i64,
    pub client_y: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuCapabilities {
    pub add_filter: bool,
    pub cell_edit: bool,
    pub delete_rows: bool,
    pub restore_row: bool,
    pub show_referencing_rows: bool,
    pub export: bool,
    pub set_value: bool,
    pub load_candidates: bool,
    pub copy_selected_cells: bool,
    pub copy_selected_cells_as: bool,
    pub paste_selected_cells: bool,
    pub paste_rows: bool,
    pub copy_rows_as_new_rows: bool,
    pub copy_problem: bool,
    pub add_sort: bool,
    pub remove_sort: bool,
    pub clear_sorts: bool,
    pub freeze_through: bool,
    pub unfreeze_columns: bool,
    pub column_stats: bool,
    pub column_presentation: bool,
}

pub struct MenuInput<'a> {
    pub column_names: &'a [String],
    pub rows: &'a [Vec<Value>],
    pub selected_rows: &'a BTreeSet<usize>,
    pub loading: bool,
    pub export_table_name: Option<&'a str>,
    pub export_enabled: bool,
    pub database_type: Option<&'a str>,
    pub primary_key_columns: &'a [String],
    pub enable_set_null: Option<bool>,
    pub context_cell_text: Option<&'a str>,
    pub dirty_by_row: Option<&'a BTreeMap<usize, RowState>>,
    pub has_cell_range: bool,
    pub can_copy_selection: bool,
    pub can_paste_selection: bool,
    pub selection_ignored_ranges: usize,
    pub can_paste_rows: bool,
    pub sorts: Option<&'a [SortCondition]>,
    pub frozen_columns: Option<usize>,
    pub set_value_state: Option<SetValueState>,
    pub capabilities: MenuCapabilities,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeaderSort {
    pub column: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeaderFreeze {
    pub index: isize,
    pub frozen: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeaderColumn {
    pub column: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CopySource {
    Text(String),
    RowsTsv(Vec<Vec<Value>>),
    RowsJson(Vec<String>, Vec<Vec<Value>>),
    RowsCsv(Vec<String>, Vec<Vec<Value>>),
    ColumnValues(usize),
    SqlInsert {
        table: String,
        columns: Vec<String>,
        rows: Vec<Vec<Value>>,
    },
    SqlUpdate {
        table: String,
        columns: Vec<String>,
        rows: Vec<Vec<Value>>,
        primary_keys: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum MenuAction {
    Copy(CopySource),
    FilterByValue {
        column: String,
        value: String,
    },
    SetNull {
        row: usize,
        column: String,
    },
    DeleteRows(Vec<usize>),
    Export,
    RestoreRow(usize),
    ShowReferencingRows(usize),
    SetValue {
        row: usize,
        column: String,
        action: SetValueActionId,
    },
    LoadCandidates {
        row: usize,
        column: String,
    },
    CopySelectedCells,
    CopySelectedCellsAs {
        format: ClipboardFormat,
        include_header: bool,
    },
    PasteSelectedCells,
    PasteRows,
    CopyRowsAsNewRows,
    SortAscending(String),
    SortDescending(String),
    RemoveSort(String),
    ClearSorts,
    FreezeThrough(usize),
    UnfreezeColumns,
    ColumnStats(String),
    ColumnPresentation(String),
    ReportCopyProblem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuEntryId {
    Copy,
    CopyRow,
    CopyAsJson,
    CopyAsSqlInsert,
    CopyAsUpdate,
    CopyAsCsv,
    CopyColumnName,
    CopyColumnData,
    SetNull,
    FilterByValue,
    CopySelectedRows,
    DeleteRow,
    Export,
    RestoreRow,
    ShowReferencingRows,
    SetValue,
    LoadCandidates,
    CopySelectedCells,
    CopySelectedCellsAs,
    PasteSelectedCells,
    PasteRows,
    CopyRowsAsNewRows,
    SortAscending,
    SortDescending,
    RemoveSort,
    ClearSorts,
    FreezeThrough,
    UnfreezeColumns,
    ColumnStats,
    ColumnPresentation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuLabels {
    pub copy: String,
    pub copy_row: String,
    pub more_actions: String,
    pub copy_as_json: String,
    pub copy_as_sql_insert: String,
    pub copy_as_update: String,
    pub copy_as_csv: String,
    pub copy_column_name: String,
    pub copy_column_data: String,
    pub set_null: String,
    pub filter_by_value: String,
    pub copy_selected_rows: String,
    pub delete_row: String,
    pub export: String,
    pub restore_row: String,
    pub referenced_by: String,
    pub set_value: String,
    pub set_empty: String,
    pub set_default: String,
    pub revert_cell: String,
    pub pick_date: String,
    pub load_binary_from_file: String,
    pub set_current_datetime: String,
    pub load_candidates: String,
    pub copy_selected_cells: String,
    pub copy_selected_cells_as: String,
    pub copy_selected_cells_as_tsv: String,
    pub copy_selected_cells_as_tsv_with_header: String,
    pub copy_selected_cells_as_csv: String,
    pub copy_selected_cells_as_json: String,
    pub copy_selected_cells_as_plain: String,
    pub copy_selected_cells_as_html: String,
    pub copy_selected_cells_as_markdown: String,
    pub paste_selected_cells: String,
    pub paste_rows: String,
    pub copy_rows_as_new_rows: String,
    pub sort_multi: String,
    pub sort_ascending: String,
    pub sort_descending: String,
    pub remove_sort_column: String,
    pub clear_sorts: String,
    pub freeze_through: String,
    pub unfreeze_columns: String,
    pub column_stats: String,
    pub column_presentation: String,
}
