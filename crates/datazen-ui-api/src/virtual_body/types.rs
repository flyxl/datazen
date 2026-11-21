use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::CellCoord;

pub const OVERSCAN: usize = 12;
pub const DEFAULT_COLUMN_WIDTH: i64 = 160;
pub const GUTTER_WIDTH: i64 = 40;

pub const VIRTUAL_BODY_GAP: [&str; 9] = [
    "row click target.closest([data-dt-row][data-dt-col])",
    "onRowSelect / onRowClick / onRowGutterSelect callbacks",
    "useVirtualTable getVirtualItems / getTotalSize",
    "scrollElement viewport measurement",
    "mouse modifier flags metaKey / ctrlKey / shiftKey",
    "t() labels",
    "DraftRow rendering",
    "FK onJump element argument",
    "onAddDraftRow double click",
];

pub type PresentationPlan = String;
pub type ColumnTypeName = String;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DraftRowViewModel {
    pub draft_id: String,
    pub cells: BTreeMap<String, Option<Value>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnDefData {
    pub id: String,
    pub name: String,
    pub column_type: Option<String>,
    pub nullable: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DirtyCellData {
    pub column_name: String,
    pub original: Option<Value>,
    pub next: Option<Value>,
    pub expression: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirtyRowState {
    Dirty,
    Deleted,
    Inserted,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DirtyRowEntryData {
    pub identity_key: String,
    pub state: DirtyRowState,
    pub changed_columns: Vec<String>,
    pub cells: Vec<DirtyCellData>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FkGroupAffordanceData {
    pub local_columns: Vec<String>,
    pub target_columns: Vec<String>,
    pub target_label: String,
    pub constraint_name: String,
    pub reference_table: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FkNavigationData {
    pub index: BTreeMap<String, FkGroupAffordanceData>,
    pub rows: Vec<BTreeMap<String, Value>>,
    pub visible_columns: Vec<String>,
    pub resolving: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnEditorInfoData {
    pub cell_type: String,
    pub nullable: bool,
    pub integer_only: bool,
    pub read_only_reason: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CellTypeHintsData {
    pub type_overrides: BTreeMap<String, String>,
    pub enum_types: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CellEditingData {
    pub column_editor_info: BTreeMap<String, ColumnEditorInfoData>,
    pub read_only_reason: Option<String>,
    pub panel_id: String,
    pub has_cell_value_handler: bool,
    pub has_load_candidates: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowModifiers {
    pub multi: bool,
    pub range: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowClickAction {
    CellSelection { open_detail: usize },
    SelectRow { index: usize, modifiers: RowModifiers },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKeyAction {
    None,
    SelectRow { index: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GutterClickAction {
    Gutter { index: usize, modifiers: RowModifiers },
    Row { index: usize, modifiers: RowModifiers },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VirtualRowData {
    pub index: usize,
    pub key: String,
    pub start: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VirtualBodyConfig {
    pub columns: Vec<ColumnDefData>,
    pub rows: Vec<Vec<Option<Value>>>,
    pub draft_rows: Vec<DraftRowViewModel>,
    pub row_height: i64,
    pub editing_cell: Option<(usize, String)>,
    pub selected_rows: BTreeSet<usize>,
    pub highlighted_row: Option<usize>,
    pub column_widths: Vec<i64>,
    pub on_cell_double_click: bool,
    pub on_cell_edit: bool,
    pub on_cell_quick_look: bool,
    pub cell_editing: Option<CellEditingData>,
    pub cell_type_hints: Option<CellTypeHintsData>,
    pub on_row_select: bool,
    pub cell_selection_enabled: bool,
    pub is_cell_selected: bool,
    pub active_cell: Option<CellCoord>,
    pub cell_id_prefix: Option<String>,
    pub on_row_gutter_select: bool,
    pub on_row_click: bool,
    pub editing_draft: Option<(String, String)>,
    pub new_row_index: Option<usize>,
    pub dirty_by_row: BTreeMap<usize, DirtyRowEntryData>,
    pub cell_errors: BTreeSet<String>,
    pub foreign_keys: Option<FkNavigationData>,
    pub has_fk_activate: bool,
    pub frozen_columns: usize,
    pub alternating_rows: bool,
    pub show_row_numbers: bool,
    pub presentation_plan_by_column: BTreeMap<String, PresentationPlan>,
    pub on_add_draft_row: bool,
}

impl Default for VirtualBodyConfig {
    fn default() -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
            draft_rows: Vec::new(),
            row_height: 32,
            editing_cell: None,
            selected_rows: BTreeSet::new(),
            highlighted_row: None,
            column_widths: Vec::new(),
            on_cell_double_click: false,
            on_cell_edit: false,
            on_cell_quick_look: false,
            cell_editing: None,
            cell_type_hints: None,
            on_row_select: false,
            cell_selection_enabled: false,
            is_cell_selected: false,
            active_cell: None,
            cell_id_prefix: None,
            on_row_gutter_select: false,
            on_row_click: false,
            editing_draft: None,
            new_row_index: None,
            dirty_by_row: BTreeMap::new(),
            cell_errors: BTreeSet::new(),
            foreign_keys: None,
            has_fk_activate: false,
            frozen_columns: 0,
            alternating_rows: true,
            show_row_numbers: true,
            presentation_plan_by_column: BTreeMap::new(),
            on_add_draft_row: false,
        }
    }
}
