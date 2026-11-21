mod actions;
mod filters;
mod order;
mod page;
mod patch;
mod types;

#[cfg(test)]
mod action_tests;
#[cfg(test)]
mod tests;

pub use actions::{
    add_filter, add_sort, apply_any_column, apply_filters, apply_raw_where, begin_load,
    cancel_edit, clear_filters, clear_highlighted_new_row, clear_pending_state, clear_sorts,
    invalidate_cached_data, load_failed, remove_filter, remove_sort, set_column_order,
    set_default_order_override, set_detail_row, set_filter_logic, set_filter_mode,
    set_filter_panel_open, set_filters, set_frozen_column_count, set_page, set_page_size,
    set_pending_status, set_raw_where_draft, set_any_column_draft, set_sort, set_visible_columns,
    start_edit, update_filter,
};
pub use filters::{apply_filters_decision, filter_draft_equals_applied, is_complete_filter};
pub use order::{order_request, resolve_default_table_sort, target_of, upsert_sort, TableContextData};
pub use page::{
    commit_fetched_page, duplicate_identity_keys, overlay_pending_rows, rows_to_records,
    sanitize_visibility, PageCommit, AMBIGUOUS_ROW_IDENTITY_ERROR,
};
pub use patch::{
    apply_selection_patch, patch_panel, patch_panel_for_reload, remove_panel_state, set_panel_state,
};
pub use types::{
    AnyColumnExpansion, FetchedPage, LoadTableDataParams, OrderRequest,
    PendingStatus, ReloadRequest, TableDataStoreSnapshot, TableTargetData,
    TABLE_DATA_STORE_GAP, TABLE_DATA_STORE_SURFACE,
};
