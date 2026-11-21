mod paging;
mod selection;
mod types;

#[cfg(test)]
mod tests;

pub use paging::{
    diff_rows, footer_visible, is_empty_state, max_cols, next_disabled, page_count, page_rows,
    prev_disabled, safe_page, server_paged,
};
pub use selection::{
    cell_view, clear_all_op, is_changed, key_label, row_selectable, select_all_op, toggle_row,
    CellView,
};
pub use types::{
    format_cell, operation_allowed, operation_badge_class, row_key_string, DataSyncOperation,
    DataSyncRowChange, DataSyncTableResult, SyncOptions, DIFF_DETAIL_DOM_GAP, PAGE_SIZE,
};
