use crate::types::SortCondition;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CopyFormat {
    Tsv,
    Csv,
    Json,
    Plain,
    Html,
    Markdown,
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
            Self::SetNull => "setNull",
            Self::SetEmpty => "setEmpty",
            Self::SetDefault => "setDefault",
            Self::RevertCell => "revertCell",
            Self::PickDate => "pickDate",
            Self::LoadBinaryFromFile => "loadBinaryFromFile",
            Self::SetCurrentDatetime => "setCurrentDatetime",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetValueAction {
    pub id: SetValueActionId,
    pub label_key: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    Update,
    Insert,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetValueState {
    pub can_null: bool,
    pub can_empty: bool,
    pub row_kind: RowKind,
    pub has_pending: bool,
    pub can_load_candidates: bool,
    pub can_pick_date: bool,
    pub can_load_binary: bool,
    pub can_set_current_datetime: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuAction {
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
    SetValue(SetValueActionId),
    LoadCandidates,
    CopySelectedCells,
    CopySelectedCellsAs {
        format: CopyFormat,
        include_header: bool,
    },
    PasteSelectedCells,
    PasteRows,
    CopyRowsAsNewRows,
    SortAscending(String),
    SortDescending(String),
    RemoveSort(String),
    ClearSorts,
    FreezeThrough(i64),
    UnfreezeColumns,
    ColumnStats(String),
    ColumnPresentation(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeMenuItemDef {
    Item {
        id: String,
        label: String,
        action: MenuAction,
        enabled: bool,
    },
    Separator,
    Submenu {
        id: String,
        label: String,
        items: Vec<NativeMenuItemDef>,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataTableContextMenuLabels {
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
    pub referenced_by: Option<String>,
    pub set_value: Option<String>,
    pub set_empty: Option<String>,
    pub set_default: Option<String>,
    pub revert_cell: Option<String>,
    pub pick_date: Option<String>,
    pub load_binary_from_file: Option<String>,
    pub set_current_datetime: Option<String>,
    pub load_candidates: Option<String>,
    pub copy_selected_cells: Option<String>,
    pub copy_selected_cells_as: Option<String>,
    pub copy_selected_cells_as_tsv: Option<String>,
    pub copy_selected_cells_as_tsv_with_header: Option<String>,
    pub copy_selected_cells_as_csv: Option<String>,
    pub copy_selected_cells_as_json: Option<String>,
    pub copy_selected_cells_as_plain: Option<String>,
    pub copy_selected_cells_as_html: Option<String>,
    pub copy_selected_cells_as_markdown: Option<String>,
    pub paste_selected_cells: Option<String>,
    pub paste_rows: Option<String>,
    pub copy_rows_as_new_rows: Option<String>,
    pub sort_multi: Option<String>,
    pub sort_ascending: Option<String>,
    pub sort_descending: Option<String>,
    pub remove_sort_column: Option<String>,
    pub clear_sorts: Option<String>,
    pub freeze_through: Option<String>,
    pub unfreeze_columns: Option<String>,
    pub column_stats: Option<String>,
    pub column_presentation: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DataTableContextMenuHandlers {
    pub on_copy: bool,
    pub on_copy_row: bool,
    pub on_copy_as_json: bool,
    pub on_copy_as_sql_insert: bool,
    pub on_copy_as_update: bool,
    pub on_copy_as_csv: bool,
    pub on_copy_column_name: bool,
    pub on_copy_column_data: bool,
    pub on_set_null: bool,
    pub on_filter_by_value: bool,
    pub on_copy_selected_rows: bool,
    pub on_delete_row: bool,
    pub on_export: bool,
    pub on_restore_row: bool,
    pub on_show_referencing_rows: bool,
    pub on_set_value: bool,
    pub on_load_candidates: bool,
    pub on_copy_selected_cells: bool,
    pub on_copy_selected_cells_as: bool,
    pub on_paste_selected_cells: bool,
    pub on_paste_rows: bool,
    pub on_copy_rows_as_new_rows: bool,
    pub on_sort_ascending: bool,
    pub on_sort_descending: bool,
    pub on_remove_sort: bool,
    pub on_clear_sorts: bool,
    pub on_freeze_through: bool,
    pub on_unfreeze_columns: bool,
    pub on_column_stats: bool,
    pub on_column_presentation: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeaderSort {
    pub column: String,
    pub sorts: Vec<SortCondition>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeaderFreeze {
    pub index: i64,
    pub frozen: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeaderStats {
    pub column: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeaderPresentation {
    pub column: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildDataTableContextMenuArgs {
    pub labels: DataTableContextMenuLabels,
    pub handlers: DataTableContextMenuHandlers,
    pub has_cell_context: bool,
    pub has_header_context: bool,
    pub header_sort: Option<HeaderSort>,
    pub header_freeze: Option<HeaderFreeze>,
    pub header_stats: Option<HeaderStats>,
    pub header_presentation: Option<HeaderPresentation>,
    pub has_selected_rows: bool,
    pub multi_row_selection: bool,
    pub export_enabled: bool,
    pub can_filter_by_value: bool,
    pub can_set_null: bool,
    pub can_delete: bool,
    pub can_restore_row: bool,
    pub set_value: Option<SetValueState>,
    pub has_cell_range: bool,
    pub can_copy_selection: bool,
    pub can_paste_selection: bool,
    pub can_paste_rows: bool,
    pub selection_ignored_ranges: i64,
}

impl Default for BuildDataTableContextMenuArgs {
    fn default() -> Self {
        Self {
            labels: DataTableContextMenuLabels::default(),
            handlers: DataTableContextMenuHandlers::default(),
            has_cell_context: false,
            has_header_context: false,
            header_sort: None,
            header_freeze: None,
            header_stats: None,
            header_presentation: None,
            has_selected_rows: false,
            multi_row_selection: false,
            export_enabled: false,
            can_filter_by_value: false,
            can_set_null: false,
            can_delete: false,
            can_restore_row: false,
            set_value: None,
            has_cell_range: false,
            can_copy_selection: false,
            can_paste_selection: false,
            can_paste_rows: true,
            selection_ignored_ranges: 0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CellContext {
    pub row_index: i64,
    pub column_name: Option<String>,
}
