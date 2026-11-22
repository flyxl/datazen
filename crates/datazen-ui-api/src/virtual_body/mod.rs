mod events;
mod indexes;
mod rows;
mod types;
mod virtualization;

#[cfg(test)]
mod row_tests;
#[cfg(test)]
mod tests;

pub use events::{
    gutter_click_action, modifiers_from_flags, row_click_action, row_double_click_action,
    row_key_down_action,
};
pub use indexes::{
    build_column_editor_info_from_defs, dirty_cell_aria_label, dirty_cell_tooltip, editor_info,
    error_columns_by_row, fk_navigable_by_row, format_dirty_expression, format_dirty_value,
    is_fk_cell_navigable, is_integer_cell_type, resolve_cell_type,
};
pub use rows::{
    build_cell_input, build_row_cell_views, cell_id, column_names, dirty_cells, error_key,
    fk_affordances, frozen_lefts, row_deleted, row_dirty, row_state, row_tint_class, sticky_info,
    CellRowContext,
};
pub use types::{
    CellEditingData, CellTypeHintsData, ColumnDefData, ColumnEditorInfoData, DraftRowViewModel,
    DirtyCellData, DirtyRowEntryData, DirtyRowState, FkGroupAffordanceData, FkNavigationData,
    GutterClickAction, PresentationPlan, RowClickAction, RowKeyAction, RowModifiers,
    VirtualBodyConfig, VirtualRowData, VIRTUAL_BODY_GAP, DEFAULT_COLUMN_WIDTH, GUTTER_WIDTH,
    OVERSCAN,
};
pub use virtualization::{
    container_height, display_rows, row_kind, virtual_table, DisplayRow, VirtualTableResult,
};
