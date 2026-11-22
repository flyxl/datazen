use crate::{PendingRowChange, RowIdentity, Value, ValueMap};

use super::types::StagedTableState;

fn stable_serialize(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::Number(number) if !number.as_f64().is_some_and(|number| number.is_finite()) => None,
        _ => Some(value.to_string()),
    }
}

pub fn build_row_identity(row: &ValueMap, primary_key_columns: &[String]) -> Option<RowIdentity> {
    if primary_key_columns.is_empty() {
        return None;
    }
    let mut identity = RowIdentity::new();
    for column in primary_key_columns {
        let value = row.get(column)?;
        stable_serialize(value)?;
        identity.insert(column.clone(), value.clone());
    }
    Some(identity)
}

pub fn row_identity_key(identity: &RowIdentity) -> Option<String> {
    if identity.is_empty() {
        return None;
    }
    let mut entries = Vec::new();
    for (column, value) in identity.iter() {
        let serialized = stable_serialize(value)?;
        let quoted = serde_json::to_string(column).unwrap_or_default();
        entries.push(format!("{quoted}:{serialized}"));
    }
    Some(format!("[{}]", entries.join(",")))
}

pub fn row_identity_is_unique(
    state: &StagedTableState,
    row_index: u64,
    identity: &RowIdentity,
    primary_key_columns: &[String],
) -> bool {
    let key = match row_identity_key(identity) {
        Some(key) => key,
        None => return false,
    };
    for (index, row) in state.rows.iter().enumerate() {
        if index as u64 == row_index {
            continue;
        }
        if let Some(other) = build_row_identity(row, primary_key_columns) {
            if row_identity_key(&other).as_deref() == Some(key.as_str()) {
                return false;
            }
        }
    }
    true
}

pub fn effective_pending_identity(
    change: &PendingRowChange,
    primary_key_columns: &[String],
) -> Option<RowIdentity> {
    let mut values = change.row_identity.clone();
    for column in primary_key_columns {
        if let Some(value) = change.current_values.get(column) {
            values.insert(column.clone(), value.clone());
        }
    }
    build_row_identity(&values, primary_key_columns)
}

pub fn has_pending_identity_collision(
    state: &StagedTableState,
    own_key: Option<&str>,
    identity: &RowIdentity,
    primary_key_columns: &[String],
) -> bool {
    let key = match row_identity_key(identity) {
        Some(key) => key,
        None => return true,
    };
    for (pending_key, change) in &state.pending_changes {
        if own_key == Some(pending_key.as_str()) {
            continue;
        }
        match effective_pending_identity(change, primary_key_columns) {
            Some(current) => {
                if row_identity_key(&current).as_deref() == Some(key.as_str()) {
                    return true;
                }
            }
            None => return true,
        }
    }
    false
}

pub fn find_pending_for_row(
    state: &StagedTableState,
    row_index: u64,
    row: &ValueMap,
    primary_key_columns: &[String],
) -> Option<(String, PendingRowChange)> {
    let identity = build_row_identity(row, primary_key_columns)?;
    if !row_identity_is_unique(state, row_index, &identity, primary_key_columns) {
        return None;
    }
    if let Some(anchored_key) = state.row_identity_anchors.get(&row_index) {
        if let Some(change) = state.pending_changes.get(anchored_key) {
            return Some((anchored_key.clone(), change.clone()));
        }
    }
    let direct_key = row_identity_key(&identity)?;
    if let Some(change) = state.pending_changes.get(&direct_key) {
        return Some((direct_key, change.clone()));
    }
    None
}
