use crate::{compute_stage_cell_value_result, StageCellValueInput, Value};

use super::identity::{
    build_row_identity, find_pending_for_row, has_pending_identity_collision, row_identity_is_unique,
    row_identity_key,
};
use super::types::{
    CellPasteWrite, GridErrorCode, PasteCellProblem, StagePasteWritesOutcome, StagedTableState,
};

fn to_cell_value(value: Option<&Value>) -> Option<Value> {
    match value {
        Some(Value::Null) | None => None,
        Some(value) => Some(value.clone()),
    }
}

fn problem(write: &CellPasteWrite, code: GridErrorCode) -> PasteCellProblem {
    PasteCellProblem {
        row_index: write.row_index,
        column_name: write.column_name.clone(),
        code,
        detail: None,
    }
}

fn unchanged(
    state: &StagedTableState,
    staged: usize,
    problems: Vec<PasteCellProblem>,
) -> StagePasteWritesOutcome {
    StagePasteWritesOutcome {
        rows: state.rows.clone(),
        pending_changes: state.pending_changes.clone(),
        row_identity_anchors: state.row_identity_anchors.clone(),
        staged,
        problems,
    }
}

pub fn stage_paste_writes_into(
    state: &StagedTableState,
    writes: &[CellPasteWrite],
    primary_key_columns: &[String],
) -> StagePasteWritesOutcome {
    if writes.is_empty() {
        return unchanged(state, 0, Vec::new());
    }
    if primary_key_columns.is_empty() {
        return unchanged(
            state,
            0,
            writes.iter().map(|write| problem(write, GridErrorCode::CellNoRowIdentity)).collect(),
        );
    }

    let mut rows = state.rows.clone();
    let mut pending_changes = state.pending_changes.clone();
    let mut row_identity_anchors = state.row_identity_anchors.clone();
    let mut working = StagedTableState {
        rows: rows.clone(),
        pending_changes: pending_changes.clone(),
        row_identity_anchors: row_identity_anchors.clone(),
    };
    let mut staged = 0usize;
    let mut applied = false;
    let mut problems = Vec::new();

    for write in writes {
        let row_index = write.row_index;
        let row_object = match rows.get(row_index as usize) {
            Some(row) => row.clone(),
            None => {
                problems.push(problem(write, GridErrorCode::PasteOutOfBounds));
                continue;
            }
        };

        let row_identity = match build_row_identity(&row_object, primary_key_columns) {
            Some(identity) if row_identity_is_unique(&working, row_index, &identity, primary_key_columns) => identity,
            _ => {
                problems.push(problem(write, GridErrorCode::CellAmbiguousIdentity));
                continue;
            }
        };

        let matched = find_pending_for_row(&working, row_index, &row_object, primary_key_columns);
        let identity = matched
            .as_ref()
            .map_or_else(|| row_identity.clone(), |entry| entry.1.row_identity.clone());
        let existing = matched.as_ref().map(|entry| entry.1.clone());
        let original_value = existing
            .as_ref()
            .filter(|change| change.original_values.contains_key(&write.column_name))
            .and_then(|change| change.original_values.get(&write.column_name).cloned())
            .or_else(|| to_cell_value(row_object.get(&write.column_name)));

        let mut prospective_row = row_object.clone();
        prospective_row.insert(write.column_name.clone(), write.value.clone());
        let prospective_identity = build_row_identity(&prospective_row, primary_key_columns);
        let key = matched
            .as_ref()
            .map_or_else(|| row_identity_key(&identity).unwrap_or_default(), |entry| entry.0.clone());
        let prospective_state = StagedTableState {
            rows: rows
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    if index as u64 == row_index {
                        prospective_row.clone()
                    } else {
                        row.clone()
                    }
                })
                .collect(),
            pending_changes: pending_changes.clone(),
            row_identity_anchors: row_identity_anchors.clone(),
        };
        let identity_broken = match prospective_identity.as_ref() {
            Some(candidate) => {
                !row_identity_is_unique(&prospective_state, row_index, candidate, primary_key_columns)
                    || has_pending_identity_collision(&working, Some(&key), candidate, primary_key_columns)
            }
            None => true,
        };
        if identity_broken {
            problems.push(problem(write, GridErrorCode::PasteIdentityChanged));
            continue;
        }

        let next = compute_stage_cell_value_result(&StageCellValueInput {
            existing,
            column: write.column_name.clone(),
            value: Some(write.value.clone()),
            original_value,
            row_identity: identity,
            row_index,
        });
        match next.change {
            Some(change) => {
                pending_changes.insert(key.clone(), change);
            }
            None => {
                pending_changes.remove(&key);
            }
        }
        applied = true;
        rows[row_index as usize] = prospective_row;
        row_identity_anchors.insert(row_index, key);
        if next.changed_columns.contains(&write.column_name) {
            staged += 1;
        }
        working = StagedTableState {
            rows: rows.clone(),
            pending_changes: pending_changes.clone(),
            row_identity_anchors: row_identity_anchors.clone(),
        };
    }

    if !applied {
        return unchanged(state, staged, problems);
    }
    StagePasteWritesOutcome { rows, pending_changes, row_identity_anchors, staged, problems }
}
