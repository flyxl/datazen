use serde_json::Value;

use crate::{
    DefaultOrderOverride, FilterCondition, FilterLogic, FilterMode, SortCondition, TableState,
};

use super::filters::apply_filters_decision;
use super::order::upsert_sort;
use super::patch::{patch_panel, patch_panel_for_reload};
use super::order::TableContextData;
use super::types::{AnyColumnExpansion, PendingStatus, ReloadRequest, TableDataStoreSnapshot};

fn invalidate_keyset(state: &mut TableState) {
    state.keyset = Value::Null;
}

fn context_field<'a>(state: &'a TableState, field: &str) -> Option<&'a str> {
    state.context.as_ref()?.get(field)?.as_str()
}

pub fn begin_load(
    current: &TableState,
    context: &TableContextData,
    settings_page_size: u64,
    driver_page_size: Option<u64>,
    sorts: Vec<SortCondition>,
    column_order: Option<Vec<String>>,
) -> Option<TableState> {
    if current.loading && current.loading_revision == Some(current.request_revision) {
        return None;
    }
    let resolved_size = if settings_page_size > 0 {
        settings_page_size
    } else {
        driver_page_size.unwrap_or(0)
    };
    let page_size = if !current.columns.is_empty() {
        current.page_size
    } else if resolved_size > 0 {
        resolved_size
    } else {
        current.page_size
    };
    let mut next = current.clone();
    next.context = Some(serde_json::json!({
        "dbSessionId": context.db_session_id,
        "table": context.table,
        "connectionId": context.connection_id,
        "driverType": context.driver_type,
        "database": context.database,
        "schema": context.schema,
    }));
    next.page_size = page_size;
    next.sorts = sorts;
    next.column_order = column_order;
    next.loading = true;
    next.loading_revision = Some(current.request_revision);
    next.error = None;
    Some(next)
}

pub fn load_failed(current: &TableState, request_revision: u64, message: &str) -> Option<TableState> {
    if current.request_revision != request_revision
        || current.loading_revision != Some(request_revision)
    {
        return None;
    }
    let mut next = current.clone();
    next.loading = false;
    next.loading_revision = None;
    next.error = Some(message.to_owned());
    super::patch::apply_selection_patch(&mut next, crate::row_index_selection_reset());
    Some(next)
}

pub fn set_page(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    page: u64,
) -> (TableDataStoreSnapshot, Option<ReloadRequest>) {
    let next = patch_panel_for_reload(store, panel_id, |state| state.page = page);
    (next, Some(ReloadRequest { skip_count: Some(true) }))
}

pub fn set_page_size(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    size: u64,
) -> (TableDataStoreSnapshot, Option<ReloadRequest>) {
    let next = patch_panel_for_reload(store, panel_id, |state| {
        state.page_size = size;
        state.page = 0;
        invalidate_keyset(state);
    });
    (next, Some(ReloadRequest { skip_count: Some(true) }))
}

pub fn add_filter(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    filter: FilterCondition,
) -> TableDataStoreSnapshot {
    patch_panel(store, panel_id, |state| {
        state.draft_filters.push(filter);
        state.filter_panel_open = true;
    })
}

pub fn set_filters(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    filters: Vec<FilterCondition>,
    logic: FilterLogic,
) -> (TableDataStoreSnapshot, Option<ReloadRequest>) {
    let next = patch_panel_for_reload(store, panel_id, |state| {
        state.filters = filters.clone();
        state.draft_filters = filters;
        state.filter_logic = logic;
        state.draft_filter_logic = logic;
        state.raw_where = None;
        state.page = 0;
        state.filter_panel_open = true;
        invalidate_keyset(state);
    });
    (next, Some(ReloadRequest::default()))
}

pub fn update_filter(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    index: usize,
    filter: FilterCondition,
) -> TableDataStoreSnapshot {
    patch_panel(store, panel_id, |state| {
        if let Some(slot) = state.draft_filters.get_mut(index) {
            *slot = filter;
        }
    })
}

pub fn set_filter_logic(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    logic: FilterLogic,
) -> TableDataStoreSnapshot {
    patch_panel(store, panel_id, |state| state.draft_filter_logic = logic)
}

pub fn remove_filter(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    index: usize,
) -> TableDataStoreSnapshot {
    patch_panel(store, panel_id, |state| {
        if index < state.draft_filters.len() {
            state.draft_filters.remove(index);
        }
    })
}

fn clear_filter_state(state: &mut TableState) {
    state.filters.clear();
    state.draft_filters.clear();
    state.filter_logic = FilterLogic::And;
    state.draft_filter_logic = FilterLogic::And;
    state.raw_where = None;
    state.raw_where_draft.clear();
    state.any_column_draft.clear();
    state.filter_mode = FilterMode::Structured;
    state.page = 0;
    invalidate_keyset(state);
}

pub fn clear_filters(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
) -> (TableDataStoreSnapshot, Option<ReloadRequest>) {
    let next = patch_panel_for_reload(store, panel_id, clear_filter_state);
    (next, Some(ReloadRequest::default()))
}

pub fn apply_filters(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
) -> (TableDataStoreSnapshot, Option<ReloadRequest>) {
    let Some(state) = store.by_panel.get(panel_id) else {
        return (store.clone(), None);
    };
    if !apply_filters_decision(state) {
        return (store.clone(), None);
    }
    let next = patch_panel_for_reload(store, panel_id, |state| {
        state.filters = state.draft_filters.clone();
        state.filter_logic = state.draft_filter_logic;
        state.raw_where = None;
        state.page = 0;
        invalidate_keyset(state);
    });
    (next, Some(ReloadRequest::default()))
}

pub fn set_filter_mode(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    mode: FilterMode,
) -> TableDataStoreSnapshot {
    patch_panel(store, panel_id, |state| state.filter_mode = mode)
}

pub fn set_any_column_draft(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    value: String,
) -> TableDataStoreSnapshot {
    patch_panel(store, panel_id, |state| state.any_column_draft = value)
}

pub fn set_raw_where_draft(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    value: String,
) -> TableDataStoreSnapshot {
    patch_panel(store, panel_id, |state| state.raw_where_draft = value)
}

pub fn apply_any_column(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    expansion: &AnyColumnExpansion,
) -> (TableDataStoreSnapshot, Option<ReloadRequest>) {
    if expansion.conditions.is_empty() {
        return clear_filters(store, panel_id);
    }
    let conditions = expansion.conditions.clone();
    let next = patch_panel_for_reload(store, panel_id, |state| {
        state.filters = conditions.clone();
        state.draft_filters = conditions;
        state.filter_logic = FilterLogic::Or;
        state.draft_filter_logic = FilterLogic::Or;
        state.raw_where = None;
        state.page = 0;
        state.filter_panel_open = true;
        invalidate_keyset(state);
    });
    (next, Some(ReloadRequest::default()))
}

pub fn apply_raw_where(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    predicate: &str,
) -> (TableDataStoreSnapshot, Option<ReloadRequest>) {
    let trimmed = predicate.trim();
    if trimmed.is_empty() {
        return clear_filters(store, panel_id);
    }
    let next = patch_panel_for_reload(store, panel_id, |state| {
        state.raw_where = Some(trimmed.to_owned());
        state.filters.clear();
        state.draft_filters.clear();
        state.filter_logic = FilterLogic::And;
        state.draft_filter_logic = FilterLogic::And;
        state.page = 0;
        state.filter_panel_open = true;
        invalidate_keyset(state);
    });
    (next, Some(ReloadRequest::default()))
}

pub fn set_filter_panel_open(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    open: bool,
) -> TableDataStoreSnapshot {
    patch_panel(store, panel_id, |state| state.filter_panel_open = open)
}

pub fn set_visible_columns(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    columns: Option<Vec<String>>,
) -> TableDataStoreSnapshot {
    patch_panel(store, panel_id, |state| state.visible_columns = columns)
}

pub fn set_sort(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    sort: SortCondition,
) -> (TableDataStoreSnapshot, Option<ReloadRequest>) {
    let next = patch_panel_for_reload(store, panel_id, |state| {
        state.sorts = vec![sort];
        state.page = 0;
        invalidate_keyset(state);
    });
    (next, Some(ReloadRequest { skip_count: Some(true) }))
}

pub fn add_sort(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    sort: SortCondition,
) -> (TableDataStoreSnapshot, Option<ReloadRequest>) {
    let next = patch_panel_for_reload(store, panel_id, |state| {
        state.sorts = upsert_sort(&state.sorts, sort);
        state.page = 0;
        invalidate_keyset(state);
    });
    (next, Some(ReloadRequest { skip_count: Some(true) }))
}

pub fn remove_sort(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    column: &str,
) -> (TableDataStoreSnapshot, Option<ReloadRequest>) {
    let next = patch_panel_for_reload(store, panel_id, |state| {
        state.sorts.retain(|entry| entry.column != column);
        state.page = 0;
        invalidate_keyset(state);
    });
    (next, Some(ReloadRequest { skip_count: Some(true) }))
}

pub fn clear_sorts(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
) -> (TableDataStoreSnapshot, Option<ReloadRequest>) {
    let next = patch_panel_for_reload(store, panel_id, |state| {
        state.sorts.clear();
        state.page = 0;
        invalidate_keyset(state);
    });
    (next, Some(ReloadRequest { skip_count: Some(true) }))
}

pub fn set_column_order(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    order: Option<Vec<String>>,
) -> TableDataStoreSnapshot {
    patch_panel(store, panel_id, |state| state.column_order = order)
}

pub fn set_frozen_column_count(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    count: u64,
) -> TableDataStoreSnapshot {
    patch_panel(store, panel_id, |state| state.frozen_column_count = count)
}

pub fn set_default_order_override(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    override_value: DefaultOrderOverride,
) -> (TableDataStoreSnapshot, Option<ReloadRequest>) {
    let next = patch_panel_for_reload(store, panel_id, |state| {
        state.default_order_override = override_value;
        state.sorts.clear();
        state.page = 0;
        invalidate_keyset(state);
    });
    (next, Some(ReloadRequest { skip_count: Some(true) }))
}

pub fn set_detail_row(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    index: Option<u64>,
) -> TableDataStoreSnapshot {
    patch_panel(store, panel_id, |state| state.detail_row_index = index)
}

pub fn start_edit(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    row: u64,
    col: String,
) -> TableDataStoreSnapshot {
    patch_panel(store, panel_id, |state| {
        state.editing_cell = Some(crate::CellEditLocation { row, col });
    })
}

pub fn cancel_edit(store: &TableDataStoreSnapshot, panel_id: &str) -> TableDataStoreSnapshot {
    patch_panel(store, panel_id, |state| state.editing_cell = None)
}

pub fn clear_highlighted_new_row(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
) -> TableDataStoreSnapshot {
    patch_panel(store, panel_id, |state| {
        if state.highlighted_new_row_key.is_some() {
            state.highlighted_new_row_key = None;
        }
    })
}

pub fn invalidate_cached_data(
    store: &TableDataStoreSnapshot,
    db_session_id: &str,
    table_name: Option<&str>,
) -> TableDataStoreSnapshot {
    let mut changed = false;
    let mut by_panel = store.by_panel.clone();
    for state in by_panel.values_mut() {
        if context_field(state, "dbSessionId") != Some(db_session_id) {
            continue;
        }
        if let Some(name) = table_name {
            if context_field(state, "table") != Some(name) {
                continue;
            }
        }
        if state.has_pending_edits() || state.editing_cell.is_some() {
            continue;
        }
        if state.columns.is_empty() && state.rows.is_empty() {
            continue;
        }
        state.columns.clear();
        state.rows.clear();
        state.total_rows = 0;
        super::patch::apply_selection_patch(state, crate::row_index_selection_reset());
        state.editing_cell = None;
        state.request_revision = state.request_revision.saturating_add(1);
        state.loading_revision = None;
        changed = true;
    }
    if changed {
        TableDataStoreSnapshot { by_panel }
    } else {
        store.clone()
    }
}

pub fn set_pending_status(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    status: PendingStatus,
) -> TableDataStoreSnapshot {
    let value = serde_json::json!(status.as_str());
    patch_panel(store, panel_id, |state| state.pending_status = value)
}

pub fn clear_pending_state(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
) -> TableDataStoreSnapshot {
    patch_panel(store, panel_id, |state| {
        state.pending_changes.clear();
        state.draft_inserts.clear();
        state.draft_insert_order.clear();
        state.preview_plan = None;
        state.pending_status = serde_json::json!(PendingStatus::Idle.as_str());
        state.editing_cell = None;
        state.highlighted_new_row_key = None;
        state.error = None;
    })
}
