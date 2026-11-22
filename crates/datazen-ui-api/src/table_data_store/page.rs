use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::{build_row_identity, row_identity_key, row_index_selection_reset, PendingRowChange, TableState};

use super::patch::apply_selection_patch;
use super::types::FetchedPage;

pub const AMBIGUOUS_ROW_IDENTITY_ERROR: &str =
    "Row identity is NULL, unstable, or ambiguous; UPDATE/DELETE was not staged.";

pub fn rows_to_records(
    columns: &[crate::types::ColumnSchema],
    rows: &[Vec<Value>],
) -> Vec<BTreeMap<String, Value>> {
    rows.iter()
        .map(|row| {
            let mut record = BTreeMap::new();
            for (index, column) in columns.iter().enumerate() {
                record.insert(
                    column.name.clone(),
                    row.get(index).cloned().unwrap_or(Value::Null),
                );
            }
            record
        })
        .collect()
}

pub fn duplicate_identity_keys(
    rows: &[BTreeMap<String, Value>],
    primary_key_columns: &[String],
) -> BTreeSet<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for row in rows {
        if let Some(identity) = build_row_identity(row, primary_key_columns) {
            if let Some(key) = row_identity_key(&identity) {
                *counts.entry(key).or_insert(0) += 1;
            }
        }
    }
    counts.into_iter().filter(|(_, count)| *count > 1).map(|(key, _)| key).collect()
}

pub fn overlay_pending_rows(
    rows: &[BTreeMap<String, Value>],
    anchors: &BTreeMap<u64, String>,
    pending_changes: &BTreeMap<String, PendingRowChange>,
    primary_key_columns: &[String],
) -> Vec<BTreeMap<String, Value>> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let identities: Vec<Option<String>> = rows
        .iter()
        .map(|row| {
            let key = build_row_identity(row, primary_key_columns)
                .and_then(|identity| row_identity_key(&identity));
            if let Some(key) = &key {
                *counts.entry(key.clone()).or_insert(0) += 1;
            }
            key
        })
        .collect();
    rows.iter()
        .enumerate()
        .map(|(index, row)| {
            let Some(direct) = identities.get(index).and_then(|key| key.clone()) else {
                return row.clone();
            };
            if counts.get(&direct).copied().unwrap_or(0) != 1 {
                return row.clone();
            }
            let key = anchors.get(&(index as u64)).cloned().unwrap_or(direct);
            let Some(change) = pending_changes.get(&key) else {
                return row.clone();
            };
            let mut overlaid = row.clone();
            for (column, value) in &change.current_values {
                overlaid.insert(column.clone(), value.clone());
            }
            overlaid
        })
        .collect()
}

pub fn sanitize_visibility(
    visible: Option<&Vec<String>>,
    order: Option<&Vec<String>>,
    valid_names: &BTreeSet<String>,
    column_count: usize,
) -> (Option<Vec<String>>, Option<Vec<String>>) {
    let visible = visible.map(|names| {
        names.iter().filter(|name| valid_names.contains(*name)).cloned().collect::<Vec<_>>()
    });
    let visible = match visible {
        Some(names) if names.len() == column_count => None,
        other => other,
    };
    let order = order
        .map(|names| {
            names.iter().filter(|name| valid_names.contains(*name)).cloned().collect::<Vec<_>>()
        })
        .filter(|names| !names.is_empty());
    (visible, order)
}

pub struct PageCommit {
    pub state: TableState,
    pub drift_recount: bool,
}

pub fn commit_fetched_page(
    current: &TableState,
    request_revision: u64,
    context: Value,
    fetched: &FetchedPage,
) -> Option<PageCommit> {
    if current.request_revision != request_revision
        || current.loading_revision != Some(request_revision)
    {
        return None;
    }
    let records = rows_to_records(&fetched.columns, &fetched.rows);
    let pk_columns: Vec<String> = fetched
        .columns
        .iter()
        .filter(|column| column.is_primary_key == Some(true))
        .map(|column| column.name.clone())
        .collect();
    let mut anchors = current.row_identity_anchors.clone();
    for (index, row) in records.iter().enumerate() {
        match build_row_identity(row, &pk_columns) {
            Some(identity) => {
                if let Some(key) = row_identity_key(&identity) {
                    anchors.insert(index as u64, key);
                    continue;
                }
            }
            None => {}
        }
        anchors.remove(&(index as u64));
    }
    let rows = overlay_pending_rows(&records, &anchors, &current.pending_changes, &pk_columns);
    let valid_names: BTreeSet<String> =
        fetched.columns.iter().map(|column| column.name.clone()).collect();
    let (visible, order) = sanitize_visibility(
        current.visible_columns.as_ref(),
        current.column_order.as_ref(),
        &valid_names,
        fetched.columns.len(),
    );
    let duplicates = duplicate_identity_keys(&records, &pk_columns);
    let drift_recount = fetched.drift_suspected
        && current.drift_recount_page != Some(fetched.page);

    let mut state = current.clone();
    state.columns = fetched.columns.clone();
    state.visible_columns = visible;
    state.column_order = order;
    state.rows = rows;
    state.total_rows = fetched.total_rows.unwrap_or(current.total_rows);
    state.drift_recount_page = if fetched.drift_suspected {
        Some(fetched.page)
    } else {
        current.drift_recount_page
    };
    state.total_rows_kind = fetched.total_rows_kind.unwrap_or(current.total_rows_kind);
    state.page = fetched.page;
    state.page_size = fetched.page_size;
    state.context = Some(context);
    state.row_identity_anchors = anchors;
    state.loading = false;
    state.loading_revision = None;
    apply_selection_patch(&mut state, row_index_selection_reset());
    state.editing_cell = None;
    state.error = if duplicates.is_empty() {
        None
    } else {
        Some(AMBIGUOUS_ROW_IDENTITY_ERROR.to_owned())
    };
    Some(PageCommit { state, drift_recount })
}
