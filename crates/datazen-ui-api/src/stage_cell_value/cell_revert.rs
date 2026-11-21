use super::{values_equal, PendingRowChange, Value};

pub fn restored_cell_value(existing: Option<&PendingRowChange>, column: &str) -> Option<Value> {
    let existing = existing?;
    if !existing.changed_columns.iter().any(|entry| entry == column) {
        return None;
    }
    existing.original_values.get(column).cloned()
}

pub fn has_pending_column_change(existing: Option<&PendingRowChange>, column: &str) -> bool {
    let existing = match existing {
        Some(existing) => existing,
        None => return false,
    };
    if !existing.changed_columns.iter().any(|entry| entry == column) {
        return false;
    }
    let expression = existing
        .expression_values
        .as_ref()
        .map(|values| values.contains_key(column))
        .unwrap_or(false);
    if expression {
        return true;
    }
    !values_equal(
        existing.original_values.get(column),
        existing.current_values.get(column),
    )
}
