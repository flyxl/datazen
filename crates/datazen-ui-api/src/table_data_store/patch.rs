use crate::{row_index_selection_reset, SelectionPatch, TableState};

use super::types::TableDataStoreSnapshot;

pub fn apply_selection_patch(state: &mut TableState, patch: SelectionPatch) {
    if let Some(selection) = patch.grid_selection {
        state.grid_selection = selection;
    }
    if let Some(rows) = patch.selected_rows {
        state.selected_rows = rows.into_iter().map(|index| index as u64).collect();
    }
    if let Some(last) = patch.last_selected_index {
        state.last_selected_index = last.map(|index| index as u64);
    }
}

fn reset_for_reload(state: &mut TableState) {
    apply_selection_patch(state, row_index_selection_reset());
    state.highlighted_new_row_key = None;
    state.request_revision = state.request_revision.saturating_add(1);
    state.drift_recount_page = None;
}

pub fn patch_panel(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    updater: impl FnOnce(&mut TableState),
) -> TableDataStoreSnapshot {
    let Some(current) = store.by_panel.get(panel_id) else {
        return store.clone();
    };
    let mut next_state = current.clone();
    updater(&mut next_state);
    let mut by_panel = store.by_panel.clone();
    by_panel.insert(panel_id.to_owned(), next_state);
    TableDataStoreSnapshot { by_panel }
}

pub fn patch_panel_for_reload(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    updater: impl FnOnce(&mut TableState),
) -> TableDataStoreSnapshot {
    let Some(current) = store.by_panel.get(panel_id) else {
        return store.clone();
    };
    let mut next_state = current.clone();
    updater(&mut next_state);
    reset_for_reload(&mut next_state);
    let mut by_panel = store.by_panel.clone();
    by_panel.insert(panel_id.to_owned(), next_state);
    TableDataStoreSnapshot { by_panel }
}

pub fn set_panel_state(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
    state: TableState,
) -> TableDataStoreSnapshot {
    let mut by_panel = store.by_panel.clone();
    by_panel.insert(panel_id.to_owned(), state);
    TableDataStoreSnapshot { by_panel }
}

pub fn remove_panel_state(
    store: &TableDataStoreSnapshot,
    panel_id: &str,
) -> TableDataStoreSnapshot {
    if !store.by_panel.contains_key(panel_id) {
        return store.clone();
    }
    let mut by_panel = store.by_panel.clone();
    by_panel.remove(panel_id);
    TableDataStoreSnapshot { by_panel }
}
