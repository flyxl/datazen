use std::collections::{BTreeMap, BTreeSet};

use super::host::{CommitCellRequest, PanelStoreHost};
use super::types::{ExecTable, PanelExecState, ResultEdit};

#[derive(Clone, Debug, PartialEq)]
pub struct BatchTarget {
    pub binding: String,
    pub statement_index: u32,
}

pub fn validate_batch(
    exec: &PanelExecState,
    result_idx: usize,
    edits: &[ResultEdit],
    in_flight: bool,
) -> Result<BatchTarget, String> {
    if edits.is_empty() {
        return Err("no edits".to_owned());
    }
    if in_flight {
        return Err("A result edit is already in progress".to_owned());
    }
    let original = exec
        .results
        .get(result_idx)
        .ok_or_else(|| "Result is read-only; execute the query again".to_owned())?;
    let binding = original
        .runtime_binding
        .clone()
        .ok_or_else(|| "Result is read-only; execute the query again".to_owned())?;
    if !original.writable_mapping_verified {
        return Err("Result is read-only; execute the query again".to_owned());
    }
    let statement_index = original
        .statement_index
        .ok_or_else(|| "Result is read-only; execute the query again".to_owned())?;
    Ok(BatchTarget {
        binding,
        statement_index,
    })
}

fn find_column(exec: &PanelExecState, result_idx: usize, column: &str) -> Option<usize> {
    exec.results
        .get(result_idx)?
        .columns
        .iter()
        .position(|entry| entry.name == column)
}

pub fn apply_result_edits(
    exec_table: &mut ExecTable,
    host: &dyn PanelStoreHost,
    key: &str,
    result_idx: usize,
    edits: &[ResultEdit],
    in_flight: &mut BTreeSet<String>,
) -> Result<(), String> {
    let current = exec_table
        .get(key)
        .ok_or_else(|| "Result is read-only; execute the query again".to_owned())?;
    let target = validate_batch(current, result_idx, edits, in_flight.contains(key))?;
    in_flight.insert(key.to_owned());
    let outcome = (|| {
        for edit in edits {
            let fresh = exec_table
                .get(key)
                .ok_or_else(|| "Result is read-only; execute the query again".to_owned())?;
            let result = fresh
                .results
                .get(result_idx)
                .ok_or_else(|| "Result is read-only; execute the query again".to_owned())?;
            if result.runtime_binding.as_deref() != Some(target.binding.as_str()) {
                return Err("Result is read-only; execute the query again".to_owned());
            }
            let column_index = find_column(fresh, result_idx, &edit.col)
                .ok_or_else(|| "Result cell unavailable".to_owned())?;
            let row = result
                .rows
                .get(edit.row as usize)
                .ok_or_else(|| "Result cell unavailable".to_owned())?;
            let existing = row.get(column_index);
            if crate::values_equal(existing, Some(&edit.value)) {
                continue;
            }
            host.commit_cell(&CommitCellRequest {
                binding: target.binding.clone(),
                statement_index: target.statement_index,
                row_index: edit.row,
                column: edit.col.clone(),
                value: edit.value.clone(),
            })?;
            if let Some(entry) = exec_table.get_mut(key) {
                if let Some(result) = entry.results.get_mut(result_idx) {
                    if let Some(row) = result.rows.get_mut(edit.row as usize) {
                        if let Some(cell) = row.get_mut(column_index) {
                            *cell = edit.value.clone();
                        }
                    }
                }
            }
        }
        Ok(())
    })();
    in_flight.remove(key);
    outcome
}

pub fn mark_error(exec_table: &mut BTreeMap<String, PanelExecState>, key: &str, message: String) {
    if let Some(entry) = exec_table.get_mut(key) {
        entry.error = Some(message);
    }
}
