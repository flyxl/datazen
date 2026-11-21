use super::types::{
    format_cell, operation_allowed, row_key_string, DataSyncOperation, DataSyncRowChange,
    DataSyncTableResult, SyncOptions,
};

pub fn toggle_row(
    rows: &[DataSyncRowChange],
    page_rows: &[DataSyncRowChange],
    index: usize,
    checked: bool,
) -> Option<Vec<DataSyncRowChange>> {
    let target = page_rows.get(index)?;
    let wanted = row_key_string(&target.key);
    let mut next = rows.to_vec();
    let position = next.iter().position(|row| row_key_string(&row.key) == wanted)?;
    if let Some(row) = next.get_mut(position) {
        row.selected = checked;
    }
    Some(next)
}

pub fn select_all_op(
    rows: &[DataSyncRowChange],
    operation: DataSyncOperation,
    options: &SyncOptions,
) -> Vec<DataSyncRowChange> {
    let allowed = operation_allowed(operation, options);
    rows.iter()
        .map(|row| {
            if row.operation != operation {
                return row.clone();
            }
            let mut next = row.clone();
            next.selected = allowed;
            next
        })
        .collect()
}

pub fn clear_all_op(rows: &[DataSyncRowChange], operation: DataSyncOperation) -> Vec<DataSyncRowChange> {
    rows.iter()
        .map(|row| {
            if row.operation != operation {
                return row.clone();
            }
            let mut next = row.clone();
            next.selected = false;
            next
        })
        .collect()
}

pub fn row_selectable(row: &DataSyncRowChange, options: &SyncOptions) -> bool {
    row.operation != DataSyncOperation::Unchanged
        && (row.operation != DataSyncOperation::Delete || options.delete)
        && operation_allowed(row.operation, options)
}

pub fn key_label(row: &DataSyncRowChange) -> String {
    row.key
        .iter()
        .map(|value| format_cell(Some(value)))
        .collect::<Vec<_>>()
        .join(" · ")
}

fn changed_column_name(table: &DataSyncTableResult, column_index: usize) -> &str {
    table
        .columns
        .as_ref()
        .and_then(|columns| columns.get(column_index))
        .map(String::as_str)
        .unwrap_or("")
}

pub fn is_changed(row: &DataSyncRowChange, table: &DataSyncTableResult, column_index: usize) -> bool {
    row.operation == DataSyncOperation::Update
        && row.changed_columns.iter().any(|name| name == changed_column_name(table, column_index))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellView {
    Insert { text: String },
    Delete { text: String },
    Update { source: String, target: String, changed: bool },
    Unchanged,
}

pub fn cell_view(row: &DataSyncRowChange, table: &DataSyncTableResult, column_index: usize) -> CellView {
    let source = row
        .source_row
        .as_ref()
        .and_then(|values| values.get(column_index))
        .and_then(|cell| cell.as_ref());
    let target = row
        .target_row
        .as_ref()
        .and_then(|values| values.get(column_index))
        .and_then(|cell| cell.as_ref());
    match row.operation {
        DataSyncOperation::Insert => CellView::Insert { text: format_cell(source) },
        DataSyncOperation::Delete => CellView::Delete { text: format_cell(target) },
        DataSyncOperation::Update => CellView::Update {
            source: format_cell(source),
            target: format_cell(target),
            changed: is_changed(row, table, column_index),
        },
        DataSyncOperation::Unchanged => CellView::Unchanged,
    }
}
