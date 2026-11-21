mod actions;
mod types;

pub use actions::{
    apply_begin_cell_selection, apply_clear_grid_selection, apply_grid_selection,
    apply_reconcile_selection, apply_row_mode_transition, apply_row_select,
    apply_toggle_select_all, displayed_column_names, panel_column_order, resolve_selection_patch,
    row_index_selection_reset, SelectionAction, SelectionOpts,
};
pub use types::{
    normalize_cell_range, reconcile_grid_selection, selection_from_anchor_focus, CellCoord,
    CellRange, GridSelection, GridSelectionMode, SelectionPatch, TableStateForSelection,
};
