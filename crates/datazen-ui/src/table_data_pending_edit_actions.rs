//! The staged-edit actions of one table panel, ported from React
//! `packages/frontend/src/stores/tableData/pendingEditActions.ts`
//! (T294).
//!
//! The React doc comment, verbatim in intent: these actions are the
//! ones the dirty-cell chapter (DB-06) rewrites, moved verbatim out of
//! `tableDataStore.ts` — same order, same reads, same writes, same
//! error branches. The store keeps only the wiring: `patch` writes
//! one panel's slice, `read` reads the current slice, and `reload`
//! re-reads the page.
//!
//! ## What maps where
//!
//! | React | here |
//! |---|---|
//! | `createPendingEditSlice(patch, read, reload)` | [`PendingEditActions`] — the actions as pure functions over a [`TableState`], each returning [`PendingEditOutcome`] (the patch the host applies, or the early-out `None`); the rollback's `reload` rides the outcome's [`PendingEditOutcome::reload`] flag |
//! | `StagePasteWritesResult` | [`StagePasteWritesResult`] |
//! | the guard branches writing `error: t(...)`/`AMBIGUOUS_ROW_IDENTITY_ERROR` | the outcome's `error` field (the patch shape carries it) |
//! | `computeStageCellValueResult` &c. | the T328 placeholder section below (real logic, module-local until T328 lands — the transfer_mapping_view precedent) |
//! | `computeStageResult` / `computeUnstageDeleteResult` | the T341 placeholder section |
//! | `stagePasteWritesInto` / `PasteCellProblem` / `CellPasteWrite` | the T353 placeholder section |
//!
//! ## The patch shape
//!
//! React's `patch(panelId, () => ({ … }))` writes a partial of
//! `TableState`; the port's [`PendingEditPatch`] mirrors exactly the
//! fields the actions write (`rows`, `pending_changes`,
//! `row_identity_anchors`, `preview_plan`, `pending_status`,
//! `editing_cell`, `error`, and the rollback's three extra:
//! `draft_inserts`, `draft_insert_order`, `highlighted_new_row_key`)
//! — `None` fields leave the panel's value untouched, exactly the
//! spread's own semantics.

use std::collections::HashMap;

use serde_json::Value;

use crate::i18n::t;
use crate::table_data_connection_state::{
    to_cell_value, ColumnSchema, PendingRowChange, TableState, WriteExpressionId,
};
use crate::table_data_dirty_index::{
    build_row_identity, row_identity_key, values_equal, RowIdentity,
};
use crate::table_data_pending_changes::{
    find_pending_for_row, has_pending_identity_collision, row_identity_is_unique,
    AMBIGUOUS_ROW_IDENTITY_ERROR,
};

// ---------------------------------------------------------------------------
// T353 placeholder: stagePasteWrites.ts — the paste batch's pure core.
// T353 replaces this section with the real port (its own module); the
// signatures are the TS ones, transliterated.
// ---------------------------------------------------------------------------

/// React `CellPasteWrite` (`lib/tableChanges.ts:369-376`): one paste
/// write — the 0-based row index (render-only), the column NAME (not
/// index), and the value (`null` = SQL `NULL`).
#[derive(Clone, Debug, PartialEq)]
pub struct CellPasteWrite {
    pub row_index: usize,
    pub column_name: String,
    /// The value to write; `Value::Null` means SQL `NULL`.
    pub value: Value,
}

/// React `PasteCellProblem` (`stagePasteWrites.ts:29-34`): one skipped
/// cell and its reason.
#[derive(Clone, Debug, PartialEq)]
pub struct PasteCellProblem {
    pub row_index: usize,
    pub column_name: String,
    /// The grid error code (`GridErrorCode`, a closed string set —
    /// carried as the code itself).
    pub code: &'static str,
    pub detail: Option<String>,
}

/// React `StagePasteWritesOutcome` (`stagePasteWrites.ts:36-44`): the
/// proposal the store applies in one `set()`.
#[derive(Clone, Debug)]
pub struct StagePasteWritesOutcome {
    pub rows: Vec<HashMap<String, Value>>,
    pub pending_changes: HashMap<String, PendingRowChange>,
    pub row_identity_anchors: HashMap<usize, String>,
    /// Cells that really entered `pendingChanges`.
    pub staged: usize,
    /// Cells that were skipped, with the reason.
    pub problems: Vec<PasteCellProblem>,
}

/// `stagePasteWritesInto` (`stagePasteWrites.ts:53-194`): write N cells
/// in one pass — same checks, same order, same branches as one
/// `stageCellValue` call, repeated over the block; a cell whose row
/// identity is ambiguous is skipped **and the rest still lands**.
pub fn stage_paste_writes_into(
    ts: &TableState,
    writes: &[CellPasteWrite],
    primary_key_columns: &[ColumnSchema],
) -> StagePasteWritesOutcome {
    if writes.is_empty() {
        // Reference-equal in React: an empty paste must not produce a
        // new state object. The port returns clones of the same
        // content — the caller's `landed` check compares by the
        // staged count and problem count instead (documented below).
        return StagePasteWritesOutcome {
            rows: ts.rows.clone(),
            pending_changes: ts.pending_changes.clone(),
            row_identity_anchors: ts.row_identity_anchors.clone(),
            staged: 0,
            problems: Vec::new(),
        };
    }

    if primary_key_columns.is_empty() {
        return StagePasteWritesOutcome {
            rows: ts.rows.clone(),
            pending_changes: ts.pending_changes.clone(),
            row_identity_anchors: ts.row_identity_anchors.clone(),
            staged: 0,
            problems: writes
                .iter()
                .map(|write| PasteCellProblem {
                    row_index: write.row_index,
                    column_name: write.column_name.clone(),
                    code: "grid.cell.noRowIdentity",
                    detail: None,
                })
                .collect(),
        };
    }

    let pk_cols: Vec<ColumnSchema> = primary_key_columns.to_vec();
    let mut rows = ts.rows.clone();
    let mut pending_changes = ts.pending_changes.clone();
    let mut row_identity_anchors = ts.row_identity_anchors.clone();
    // The working state: every iteration sees the writes of the
    // previous ones, exactly as the single-cell path would after its
    // own `set()`.
    let mut working = ts.clone();
    working.rows = rows.clone();
    working.pending_changes = pending_changes.clone();
    working.row_identity_anchors = row_identity_anchors.clone();

    let mut staged = 0usize;
    let mut applied = false;
    let mut problems: Vec<PasteCellProblem> = Vec::new();

    for write in writes {
        let (row_index, column_name, current_value) =
            (&write.row_index, write.column_name.as_str(), &write.value);
        if *row_index >= rows.len() {
            problems.push(PasteCellProblem {
                row_index: *row_index,
                column_name: column_name.to_owned(),
                code: "grid.paste.outOfBounds",
                detail: None,
            });
            continue;
        }
        let row_object = &rows[*row_index];

        // The row identity is resolved from the row's own primary-key
        // values, never from the row number (00-contracts §4.3).
        let Some(row_identity) = build_row_identity(row_object, &pk_cols) else {
            problems.push(PasteCellProblem {
                row_index: *row_index,
                column_name: column_name.to_owned(),
                code: "grid.cell.ambiguousIdentity",
                detail: None,
            });
            continue;
        };
        if !row_identity_is_unique(&working, *row_index, &row_identity, &pk_cols) {
            problems.push(PasteCellProblem {
                row_index: *row_index,
                column_name: column_name.to_owned(),
                code: "grid.cell.ambiguousIdentity",
                detail: None,
            });
            continue;
        }
        let match_ = find_pending_for_row(&working, *row_index, row_object, &pk_cols);
        let identity = match_.as_ref().map(|(_, change)| change.row_identity.clone());
        let identity = match identity {
            Some(identity) => identity,
            None => row_identity,
        };
        if identity.is_empty() {
            problems.push(PasteCellProblem {
                row_index: *row_index,
                column_name: column_name.to_owned(),
                code: "grid.cell.ambiguousIdentity",
                detail: None,
            });
            continue;
        }
        let existing = match_.as_ref().map(|(_, change)| change.clone());
        let has_original_value = existing
            .as_ref()
            .is_some_and(|change| change.original_values.contains_key(column_name));
        let original_value = match &existing {
            Some(change) if has_original_value => change
                .original_values
                .get(column_name)
                .cloned()
                .unwrap_or(Value::Null),
            _ => to_cell_value(row_object.get(column_name).cloned()),
        };
        let prospective_row = {
            let mut row = row_object.clone();
            row.insert(column_name.to_owned(), current_value.clone());
            row
        };
        let prospective_identity = build_row_identity(&prospective_row, &pk_cols);
        let Some(key) = match_
            .as_ref()
            .map(|(key, _)| key.clone())
            .or_else(|| row_identity_key(&identity)) else {
            problems.push(PasteCellProblem {
                row_index: *row_index,
                column_name: column_name.to_owned(),
                code: "grid.cell.ambiguousIdentity",
                detail: None,
            });
            continue;
        };
        let identity_changed = match &prospective_identity {
            None => true,
            Some(prospective) => {
                let mut prospective_rows = rows.clone();
                prospective_rows[*row_index] = prospective_row.clone();
                let mut prospective_ts = working.clone();
                prospective_ts.rows = prospective_rows;
                !row_identity_is_unique(
                    &prospective_ts,
                    *row_index,
                    prospective,
                    &pk_cols,
                )
            }
        };
        let effective_prospective = prospective_identity.as_ref().unwrap_or(&identity);
        if identity_changed
            || has_pending_identity_collision(
                &working,
                Some(key.as_str()),
                effective_prospective,
                &pk_cols,
            )
        {
            // Paste-specific code: the user did not aim at the primary
            // key, so "the row would no longer be locatable" is told
            // with the code the copy chapter owns (03 §3.3).
            problems.push(PasteCellProblem {
                row_index: *row_index,
                column_name: column_name.to_owned(),
                code: "grid.paste.identityChanged",
                detail: None,
            });
            continue;
        }

        let next = compute_stage_cell_value_result(StageCellValueInput {
            existing: existing.clone(),
            column: column_name.to_owned(),
            value: Some(current_value.clone()),
            original_value,
            row_identity: identity.clone(),
            row_index: *row_index,
        });
        match next.change {
            None => {
                pending_changes.remove(&key);
            }
            Some(change) => {
                if change.changed_columns.iter().any(|column| column == column_name) {
                    staged += 1;
                }
                pending_changes.insert(key.clone(), change);
            }
        }
        applied = true;
        rows[*row_index] = prospective_row;
        row_identity_anchors.insert(*row_index, key);
        // Keep the working state current for the next write.
        working.rows = rows.clone();
        working.pending_changes = pending_changes.clone();
        working.row_identity_anchors = row_identity_anchors.clone();
    }

    // Every cell was skipped (`applied === false`): hand the original
    // content back — the caller decides whether to publish (the
    // staged count and the problem list carry the decision).
    if !applied {
        return StagePasteWritesOutcome {
            rows: ts.rows.clone(),
            pending_changes: ts.pending_changes.clone(),
            row_identity_anchors: ts.row_identity_anchors.clone(),
            staged,
            problems,
        };
    }
    StagePasteWritesOutcome {
        rows,
        pending_changes,
        row_identity_anchors,
        staged,
        problems,
    }
}

// ---------------------------------------------------------------------------
// T341 placeholder: stageEdit.ts — the next version of a staged change.
// T341 replaces this section with the real port.
// ---------------------------------------------------------------------------

/// React `StageEditInput` (`stageEdit.ts:16-29`).
#[derive(Clone, Debug)]
pub struct StageEditInput {
    pub existing: Option<PendingRowChange>,
    pub column: String,
    /// The value before the edit; `Value::Null` = React's `null`.
    pub original_value: Value,
    /// The value the user entered; `Value::Null` = React's `null`.
    pub current_value: Value,
    pub row_identity: RowIdentity,
    pub row_index: usize,
}

/// React `StageEditResult` (`stageEdit.ts:31-40`): the next version of
/// the change, or `None` when the row has nothing staged left.
#[derive(Clone, Debug)]
pub struct StageEditResult {
    pub change: Option<PendingRowChange>,
    pub changed_columns: Vec<String>,
}

/// `computeStageResult` (`stageEdit.ts:54-84`): the next version of a
/// row's staged change after one cell was edited — editing a cell to a
/// value that differs from the original stages it, editing it **back**
/// to the original unstages it (and if that was the only changed
/// column and the row is not marked for deletion, the row's key leaves
/// `pendingChanges` entirely), and a row marked for deletion keeps its
/// entry even when every column was reverted.
pub fn compute_stage_result(input: StageEditInput) -> StageEditResult {
    let mut original_values = input
        .existing
        .as_ref()
        .map(|change| change.original_values.clone())
        .unwrap_or_default();
    let mut current_values = input
        .existing
        .as_ref()
        .map(|change| change.current_values.clone())
        .unwrap_or_default();
    let mut changed_columns = input
        .existing
        .as_ref()
        .map(|change| change.changed_columns.clone())
        .unwrap_or_default();

    if values_equal(&input.original_value, &input.current_value) {
        original_values.remove(&input.column);
        current_values.remove(&input.column);
        changed_columns.retain(|column| column != &input.column);
    } else {
        original_values.insert(input.column.clone(), input.original_value.clone());
        current_values.insert(input.column.clone(), input.current_value.clone());
        if !changed_columns.iter().any(|column| column == &input.column) {
            changed_columns.push(input.column.clone());
        }
    }

    let delete_marked = input.existing.as_ref().is_some_and(|change| change.delete_marked);
    if changed_columns.is_empty() && !delete_marked {
        return StageEditResult {
            change: None,
            changed_columns,
        };
    }

    StageEditResult {
        change: Some(PendingRowChange {
            row_index: Some(input.row_index),
            row_identity: input.row_identity.clone(),
            original_values,
            current_values,
            changed_columns: changed_columns.clone(),
            delete_marked,
            insert_marked: false,
            expression_values: None,
        }),
        changed_columns,
    }
}

/// `computeUnstageDeleteResult` (`stageEdit.ts:93-96`): the next
/// version after the delete mark was undone — a row that was only
/// marked has nothing left staged (its key leaves the map); a row that
/// also had column changes keeps them and only loses the mark.
pub fn compute_unstage_delete_result(change: &PendingRowChange) -> Option<PendingRowChange> {
    if change.changed_columns.is_empty() {
        return None;
    }
    let mut next = change.clone();
    next.delete_marked = false;
    Some(next)
}

// ---------------------------------------------------------------------------
// T328 placeholder: stageCellValue.ts — the three states of a write.
// T328 replaces this section with the real port.
// ---------------------------------------------------------------------------

/// React `StageCellValueInput` (`stageCellValue.ts:22-40`). The write
/// intent: `Some(value)` = write it (`Value::Null` = SQL `NULL`),
/// `None` = do not write this column (the revert).
#[derive(Clone, Debug)]
pub struct StageCellValueInput {
    pub existing: Option<PendingRowChange>,
    pub column: String,
    pub value: Option<Value>,
    pub original_value: Value,
    pub row_identity: RowIdentity,
    pub row_index: usize,
}

/// React `StageExpressionValueInput` (`stageCellValue.ts:42-53`).
#[derive(Clone, Debug)]
pub struct StageExpressionValueInput {
    pub existing: Option<PendingRowChange>,
    pub column: String,
    pub original_value: Value,
    pub row_identity: RowIdentity,
    pub row_index: usize,
}

/// React `StageCellValueResult` (`stageCellValue.ts:99-108`).
#[derive(Clone, Debug)]
pub struct StageCellValueResult {
    pub change: Option<PendingRowChange>,
    pub changed_columns: Vec<String>,
}

/// `computeStageExpressionValueResult` (`stageCellValue.ts:65-97`):
/// stage one cell as a typed write expression (DBR-05) — the column
/// enters `changedColumns` like any write, and `currentValues` carries
/// the original as the channel's placeholder; a literal staged on the
/// same column afterwards replaces the expression, and the revert
/// clears both.
pub fn compute_stage_expression_value_result(
    input: StageExpressionValueInput,
) -> StageCellValueResult {
    let existing = input.existing.clone();
    let mut expression_values = existing
        .as_ref()
        .and_then(|change| change.expression_values.clone())
        .unwrap_or_default();
    expression_values.insert(input.column.clone(), WriteExpressionId::CurrentTimestamp);
    let mut original_values = existing
        .as_ref()
        .map(|change| change.original_values.clone())
        .unwrap_or_default();
    let mut current_values = existing
        .as_ref()
        .map(|change| change.current_values.clone())
        .unwrap_or_default();
    // `Array.from(new Set([...existing.changedColumns, column]))` —
    // the existing order then the new column, deduped.
    let mut changed_columns = existing
        .as_ref()
        .map(|change| change.changed_columns.clone())
        .unwrap_or_default();
    if !changed_columns.iter().any(|column| column == &input.column) {
        changed_columns.push(input.column.clone());
    }
    original_values
        .entry(input.column.clone())
        .or_insert_with(|| input.original_value.clone());
    // The placeholder keeps the plan invariant ("every changed column
    // has a current value") while the expression owns what the SQL
    // renders.
    let placeholder = current_values
        .get(&input.column)
        .cloned()
        .unwrap_or_else(|| input.original_value.clone());
    current_values.insert(input.column.clone(), placeholder);

    let change = PendingRowChange {
        row_index: existing
            .as_ref()
            .and_then(|change| change.row_index)
            .or(Some(input.row_index)),
        row_identity: existing
            .as_ref()
            .map(|change| change.row_identity.clone())
            .unwrap_or_else(|| input.row_identity.clone()),
        original_values,
        current_values,
        changed_columns: changed_columns.clone(),
        delete_marked: existing
            .as_ref()
            .is_some_and(|change| change.delete_marked),
        insert_marked: false,
        expression_values: Some(expression_values),
    };
    StageCellValueResult {
        change: Some(change),
        changed_columns,
    }
}

/// `computeStageCellValueResult` (`stageCellValue.ts:116-168`): the
/// next version of a row's staged change after one cell was written —
/// a value is written when it differs from the original (the
/// stage-edit rule, including "reverting the only changed column
/// unstages the row"), `null` is a value like any other, and `None`
/// (do not write) is the revert.
pub fn compute_stage_cell_value_result(
    input: StageCellValueInput,
) -> StageCellValueResult {
    if let Some(value) = input.value.clone() {
        let mut staged = compute_stage_result(StageEditInput {
            existing: input.existing.clone(),
            column: input.column.clone(),
            original_value: input.original_value.clone(),
            current_value: value,
            row_identity: input.row_identity.clone(),
            row_index: input.row_index,
        });
        // One write intent per column: a literal (or a NULL) replaces
        // a staged expression — the newest intent is the only one the
        // commit renders.
        if let Some(change) = &mut staged.change {
            if change
                .expression_values
                .as_ref()
                .is_some_and(|values| values.contains_key(&input.column))
            {
                let mut next = change.expression_values.clone().unwrap_or_default();
                next.remove(&input.column);
                change.expression_values = (!next.is_empty()).then_some(next);
            }
        }
        return StageCellValueResult {
            change: staged.change,
            changed_columns: staged.changed_columns,
        };
    }

    // The `undefined` case: do not write this column.
    let Some(existing) = input.existing.clone() else {
        return StageCellValueResult {
            change: None,
            changed_columns: Vec::new(),
        };
    };

    let mut original_values = existing.original_values.clone();
    let mut current_values = existing.current_values.clone();
    original_values.remove(&input.column);
    current_values.remove(&input.column);
    let changed_columns = existing
        .changed_columns
        .iter()
        .filter(|column| column.as_str() != input.column.as_str())
        .cloned()
        .collect::<Vec<_>>();
    // The revert clears a staged expression with the write it belongs
    // to.
    let mut expression_values = existing.expression_values.clone();
    if let Some(values) = expression_values.clone() {
        if values.contains_key(&input.column) {
            let mut next = values;
            next.remove(&input.column);
            expression_values = (!next.is_empty()).then_some(next);
        }
    }

    // A row that is marked for deletion keeps its entry: the delete
    // mark is a fact of its own.
    if changed_columns.is_empty() && !existing.delete_marked {
        return StageCellValueResult {
            change: None,
            changed_columns,
        };
    }

    StageCellValueResult {
        change: Some(PendingRowChange {
            row_index: existing.row_index,
            row_identity: existing.row_identity.clone(),
            original_values,
            current_values,
            changed_columns: changed_columns.clone(),
            delete_marked: existing.delete_marked,
            insert_marked: false,
            expression_values,
        }),
        changed_columns,
    }
}

/// `restoredCellValue` (`stageCellValue.ts:176-184`): the value a cell
/// goes back to when its change is undone — `Value::Null` when the
/// column was never staged (the cell keeps whatever it holds).
pub fn restored_cell_value(
    existing: Option<&PendingRowChange>,
    column: &str,
) -> Value {
    let Some(existing) = existing else {
        return Value::Null;
    };
    if !existing.changed_columns.iter().any(|changed| changed == column) {
        return Value::Null;
    }
    existing
        .original_values
        .get(column)
        .cloned()
        .unwrap_or(Value::Null)
}

// ---------------------------------------------------------------------------
// The actions (React 95-462).
// ---------------------------------------------------------------------------

/// React's `patch(panelId, () => ({ … }))` payload — exactly the
/// fields the actions write; `None` leaves the panel's field
/// untouched (the partial's own semantics).
#[derive(Clone, Debug, Default)]
pub struct PendingEditPatch {
    pub rows: Option<Vec<HashMap<String, Value>>>,
    pub pending_changes: Option<HashMap<String, PendingRowChange>>,
    pub row_identity_anchors: Option<HashMap<usize, String>>,
    pub preview_plan: Option<()>,
    pub pending_status: Option<crate::table_data_connection_state::PendingStatus>,
    pub editing_cell: Option<Option<crate::table_data_connection_state::EditingCell>>,
    pub error: Option<Option<String>>,
    pub draft_inserts: Option<HashMap<String, crate::table_data_connection_state::PendingInsert>>,
    pub draft_insert_order: Option<Vec<String>>,
    pub highlighted_new_row_key: Option<Option<String>>,
}

/// What one action call produced: the patch the host applies — plus
/// the rollback's reload flag.
#[derive(Clone, Debug)]
pub struct PendingEditOutcome {
    pub patch: PendingEditPatch,
    /// `rollbackPendingChanges`'s `reload(panelId)` — the host re-reads
    /// the page after applying the patch.
    pub reload: bool,
}

impl PendingEditOutcome {
    fn write(patch: PendingEditPatch) -> Self {
        Self {
            patch,
            reload: false,
        }
    }
}

/// React `StagePasteWritesResult` (`pendingEditActions.ts:41-47`): what
/// one paste did, for the caller's summary line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StagePasteWritesResult {
    /// Cells that really entered `pendingChanges`.
    pub staged: usize,
    /// Cells that were skipped (the reason is in `problems`).
    pub skipped: usize,
    pub problems: Vec<PasteCellProblem>,
}

/// The staged-edit actions, moved verbatim out of the store — the
/// bodies are the React source's, same order, same reads, same writes,
/// same error branches; each action reads the panel's whole state and
/// returns the patch (or `None` for the early outs).
pub struct PendingEditActions;

impl PendingEditActions {
    /// The shared patch of a landed staging (React 141-149 &c): the
    /// staged change, the anchors, the reset preview/status/editing,
    /// and the cleared error.
    fn landed_patch() -> PendingEditPatch {
        PendingEditPatch {
            preview_plan: Some(()),
            pending_status: Some(crate::table_data_connection_state::PendingStatus::Idle),
            editing_cell: Some(None),
            error: Some(None),
            ..Default::default()
        }
    }

    /// The no-primary-key refusal.
    fn no_primary_key() -> Option<PendingEditOutcome> {
        Some(PendingEditOutcome::write(PendingEditPatch {
            error: Some(Some(t("tableData.noPrimaryKey").to_owned())),
            ..Default::default()
        }))
    }

    /// The ambiguous-identity refusal.
    fn ambiguous() -> Option<PendingEditOutcome> {
        Some(PendingEditOutcome::write(PendingEditPatch {
            error: Some(Some(AMBIGUOUS_ROW_IDENTITY_ERROR.to_owned())),
            ..Default::default()
        }))
    }

    /// `revertCellValue` (React 100-150): undo the staged change of one
    /// column ("revert this cell") — the column leaves the change list
    /// and the cell goes back to the value it had before the change.
    fn revert_cell_value(
        ts: &TableState,
        row: usize,
        col: &str,
    ) -> Option<PendingEditOutcome> {
        let row_obj = ts.rows.get(row)?;
        let pk_cols: Vec<ColumnSchema> = ts
            .columns
            .iter()
            .filter(|c| c.is_primary_key)
            .cloned()
            .collect();
        if pk_cols.is_empty() {
            return Self::no_primary_key();
        }
        let match_ = find_pending_for_row(ts, row, row_obj, &pk_cols)?;
        let (key, change) = match_;
        if !change.changed_columns.iter().any(|column| column == col) {
            return None;
        }

        let staged = compute_stage_cell_value_result(StageCellValueInput {
            existing: Some(change.clone()),
            column: col.to_owned(),
            value: None,
            original_value: change
                .original_values
                .get(col)
                .cloned()
                .unwrap_or(Value::Null),
            row_identity: change.row_identity.clone(),
            row_index: row,
        });

        let mut pending_changes = ts.pending_changes.clone();
        match staged.change {
            None => {
                pending_changes.remove(&key);
            }
            Some(next) => {
                pending_changes.insert(key.clone(), next);
            }
        }

        let mut next_rows = ts.rows.clone();
        let mut reverted = row_obj.clone();
        reverted.insert(col.to_owned(), restored_cell_value(Some(&change), col));
        next_rows[row] = reverted;
        let mut row_identity_anchors = ts.row_identity_anchors.clone();
        row_identity_anchors.insert(row, key);

        let mut patch = Self::landed_patch();
        patch.rows = Some(next_rows);
        patch.pending_changes = Some(pending_changes);
        patch.row_identity_anchors = Some(row_identity_anchors);
        Some(PendingEditOutcome::write(patch))
    }

    /// `stageCellExpression` (React 152-206): stage one cell as a typed
    /// write expression (DBR-05) — the cell keeps displaying its
    /// current value; the dirty mark and its tooltip tell the story.
    pub fn stage_cell_expression(
        ts: &TableState,
        row: usize,
        col: &str,
    ) -> Option<PendingEditOutcome> {
        let row_obj = ts.rows.get(row)?;
        let pk_cols: Vec<ColumnSchema> = ts
            .columns
            .iter()
            .filter(|c| c.is_primary_key)
            .cloned()
            .collect();
        if pk_cols.is_empty() {
            return Self::no_primary_key();
        }
        let row_identity = build_row_identity(row_obj, &pk_cols);
        match row_identity.as_ref().map(|identity| {
            row_identity_is_unique(ts, row, identity, &pk_cols)
        }) {
            None | Some(false) => return Self::ambiguous(),
            Some(true) => {}
        }
        let match_ = find_pending_for_row(ts, row, row_obj, &pk_cols);
        let identity = match_
            .as_ref()
            .and_then(|(_, change)| Some(change.row_identity.clone()))
            .or(row_identity);
        let identity = identity.filter(|identity| !identity.is_empty())?;
        let existing = match_.as_ref().map(|(_, change)| change.clone());
        let key = match_
            .as_ref()
            .map(|(key, _)| key.clone())
            .or_else(|| row_identity_key(&identity))?;
        if has_pending_identity_collision(ts, Some(key.as_str()), &identity, &pk_cols) {
            return Self::ambiguous();
        }
        let original_value = match &existing {
            Some(change) if change.original_values.contains_key(col) => {
                change.original_values.get(col).cloned().unwrap_or(Value::Null)
            }
            _ => to_cell_value(row_obj.get(col).cloned()),
        };
        let staged = compute_stage_expression_value_result(StageExpressionValueInput {
            existing: existing.clone(),
            column: col.to_owned(),
            original_value,
            row_identity: identity.clone(),
            row_index: row,
        });
        let mut pending_changes = ts.pending_changes.clone();
        if let Some(change) = staged.change {
            pending_changes.insert(key.clone(), change);
        }
        let mut row_identity_anchors = ts.row_identity_anchors.clone();
        row_identity_anchors.insert(row, key);
        let mut patch = Self::landed_patch();
        patch.pending_changes = Some(pending_changes);
        patch.row_identity_anchors = Some(row_identity_anchors);
        Some(PendingEditOutcome::write(patch))
    }

    /// `stageCellValue` (React 208-302): stage one cell edit with an
    /// explicit write intent (DB-05) — `None` = do not write this
    /// column, `Some(Value::Null)` = write SQL `NULL`, anything else =
    /// write that value.
    pub fn stage_cell_value(
        ts: &TableState,
        row: usize,
        col: &str,
        value: Option<Value>,
    ) -> Option<PendingEditOutcome> {
        let row_obj = ts.rows.get(row)?;
        let pk_cols: Vec<ColumnSchema> = ts
            .columns
            .iter()
            .filter(|c| c.is_primary_key)
            .cloned()
            .collect();
        if pk_cols.is_empty() {
            return Self::no_primary_key();
        }
        // "Do not write this column" is the undo of a staged change, and
        // it has its own path: there is no value to check against the row
        // identity.
        if value.is_none() {
            return Self::revert_cell_value(ts, row, col);
        }
        let value = value.unwrap_or(Value::Null);

        let row_identity = build_row_identity(row_obj, &pk_cols);
        match row_identity.as_ref().map(|identity| {
            row_identity_is_unique(ts, row, identity, &pk_cols)
        }) {
            None | Some(false) => return Self::ambiguous(),
            Some(true) => {}
        }
        let match_ = find_pending_for_row(ts, row, row_obj, &pk_cols);
        let identity = match_
            .as_ref()
            .and_then(|(_, change)| Some(change.row_identity.clone()))
            .or(row_identity);
        let identity = identity.filter(|identity| !identity.is_empty())?;
        let existing = match_.as_ref().map(|(_, change)| change.clone());
        let original_value = match &existing {
            Some(change) if change.original_values.contains_key(col) => {
                change.original_values.get(col).cloned().unwrap_or(Value::Null)
            }
            _ => to_cell_value(row_obj.get(col).cloned()),
        };
        let current_value = value;
        let mut prospective_row = row_obj.clone();
        prospective_row.insert(col.to_owned(), current_value.clone());
        let prospective_identity = build_row_identity(&prospective_row, &pk_cols);
        let key = match_
            .as_ref()
            .map(|(key, _)| key.clone())
            .or_else(|| row_identity_key(&identity))?;

        // The prospective-row uniqueness: the row as it would be after
        // the write, the caller's own index excluded.
        let prospective_ok = prospective_identity.as_ref().is_some_and(|prospective| {
            let mut prospective_rows = ts.rows.clone();
            prospective_rows[row] = prospective_row.clone();
            let mut prospective_ts = ts.clone();
            prospective_ts.rows = prospective_rows;
            row_identity_is_unique(&prospective_ts, row, prospective, &pk_cols)
        });
        if !prospective_ok
            || has_pending_identity_collision(
                ts,
                Some(key.as_str()),
                prospective_identity.as_ref().unwrap_or(&identity),
                &pk_cols,
            )
        {
            return Self::ambiguous();
        }
        let staged = compute_stage_cell_value_result(StageCellValueInput {
            existing: existing.clone(),
            column: col.to_owned(),
            value: Some(current_value.clone()),
            original_value,
            row_identity: identity.clone(),
            row_index: row,
        });

        let mut pending_changes = ts.pending_changes.clone();
        match staged.change {
            None => {
                pending_changes.remove(&key);
            }
            Some(next) => {
                pending_changes.insert(key.clone(), next);
            }
        }

        let mut next_rows = ts.rows.clone();
        next_rows[row] = prospective_row;
        let mut row_identity_anchors = ts.row_identity_anchors.clone();
        row_identity_anchors.insert(row, key);

        let mut patch = Self::landed_patch();
        patch.rows = Some(next_rows);
        patch.pending_changes = Some(pending_changes);
        patch.row_identity_anchors = Some(row_identity_anchors);
        Some(PendingEditOutcome::write(patch))
    }

    /// `stageCellChange` (React 304-308): the old entry point keeps its
    /// behaviour byte for byte — an absent value was a `NULL`, and it
    /// still is.
    pub fn stage_cell_change(
        ts: &TableState,
        row: usize,
        col: &str,
        value: Option<Value>,
    ) -> Option<PendingEditOutcome> {
        Self::stage_cell_value(ts, row, col, Some(to_cell_value(value)))
    }

    /// `stageRowDelete` (React 310-365): mark rows for deletion.
    pub fn stage_row_delete(
        ts: &TableState,
        row_indices: &[usize],
    ) -> Option<PendingEditOutcome> {
        let pk_cols: Vec<ColumnSchema> = ts
            .columns
            .iter()
            .filter(|c| c.is_primary_key)
            .cloned()
            .collect();
        if pk_cols.is_empty() {
            return Self::no_primary_key();
        }

        let mut indices: Vec<usize> = row_indices.to_vec();
        indices.sort_unstable();
        indices.dedup();
        if indices.is_empty() {
            return None;
        }

        let mut pending_changes = ts.pending_changes.clone();
        let mut row_identity_anchors = ts.row_identity_anchors.clone();
        for row_index in indices {
            let Some(row) = ts.rows.get(row_index) else {
                continue;
            };
            let row_identity = build_row_identity(row, &pk_cols);
            match row_identity.as_ref().map(|identity| {
                row_identity_is_unique(ts, row_index, identity, &pk_cols)
            }) {
                None | Some(false) => return Self::ambiguous(),
                Some(true) => {}
            }
            let match_ = find_pending_for_row(ts, row_index, row, &pk_cols);
            let identity = match_
                .as_ref()
                .and_then(|(_, change)| Some(change.row_identity.clone()))
                .or(row_identity);
            let key = match_
                .as_ref()
                .map(|(key, _)| key.clone())
                .or_else(|| identity.as_ref().and_then(row_identity_key))?;
            if has_pending_identity_collision(
                ts,
                Some(key.as_str()),
                identity.as_ref().unwrap_or(&RowIdentity::default()),
                &pk_cols,
            ) {
                return Self::ambiguous();
            }
            let existing = match_.as_ref().map(|(_, change)| change.clone());
            let next = PendingRowChange {
                row_index: Some(row_index),
                row_identity: identity.clone().unwrap_or_default(),
                original_values: existing
                    .as_ref()
                    .map(|change| change.original_values.clone())
                    .unwrap_or_default(),
                current_values: existing
                    .as_ref()
                    .map(|change| change.current_values.clone())
                    .unwrap_or_default(),
                changed_columns: existing
                    .as_ref()
                    .map(|change| change.changed_columns.clone())
                    .unwrap_or_default(),
                delete_marked: true,
                insert_marked: false,
                expression_values: existing.as_ref().and_then(|change| change.expression_values.clone()),
            };
            pending_changes.insert(key.clone(), next);
            row_identity_anchors.insert(row_index, key);
        }

        let mut patch = Self::landed_patch();
        patch.pending_changes = Some(pending_changes);
        patch.row_identity_anchors = Some(row_identity_anchors);
        Some(PendingEditOutcome::write(patch))
    }

    /// `stagePasteWrites` (React 372-396): stage many cells in ONE state
    /// write (DB-03); returns what happened instead of writing
    /// `ts.error`.
    pub fn stage_paste_writes(
        ts: &TableState,
        writes: &[CellPasteWrite],
    ) -> (Option<PendingEditOutcome>, StagePasteWritesResult) {
        if writes.is_empty() {
            return (
                None,
                StagePasteWritesResult {
                    staged: 0,
                    skipped: 0,
                    problems: Vec::new(),
                },
            );
        }
        let pk_cols: Vec<ColumnSchema> = ts
            .columns
            .iter()
            .filter(|column| column.is_primary_key)
            .cloned()
            .collect();
        let outcome = stage_paste_writes_into(ts, writes, &pk_cols);
        // A paste where every cell was skipped leaves the state
        // untouched: writing it anyway would re-render the whole grid
        // and clear the preview for nothing.
        let landed = outcome.staged > 0 || outcome.problems.len() < writes.len();
        let patch = if landed {
            let mut patch = Self::landed_patch();
            patch.rows = Some(outcome.rows.clone());
            patch.pending_changes = Some(outcome.pending_changes.clone());
            patch.row_identity_anchors = Some(outcome.row_identity_anchors.clone());
            Some(PendingEditOutcome::write(patch))
        } else {
            None
        };
        (
            patch,
            StagePasteWritesResult {
                staged: outcome.staged,
                skipped: outcome.problems.len(),
                problems: outcome.problems,
            },
        )
    }

    /// `applyColumnToRows` (React 398-400): stage the same value in one
    /// column of many rows (one call per cell — the loop the host runs,
    /// each iteration reading the previous write's state).
    pub fn apply_column_to_rows(
        ts: &TableState,
        col: &str,
        value: Option<Value>,
        rows: &[usize],
    ) -> Vec<PendingEditOutcome> {
        let mut ts = ts.clone();
        let mut outcomes = Vec::new();
        for row in rows {
            if let Some(outcome) = Self::stage_cell_change(&ts, *row, col, value.clone()) {
                apply_patch(&mut ts, &outcome.patch);
                outcomes.push(outcome);
            }
        }
        outcomes
    }

    /// `unstageRowDelete` (React 404-431): undo the delete mark of one
    /// row.
    pub fn unstage_row_delete(
        ts: &TableState,
        row_index: usize,
    ) -> Option<PendingEditOutcome> {
        let pk_cols: Vec<ColumnSchema> = ts
            .columns
            .iter()
            .filter(|c| c.is_primary_key)
            .cloned()
            .collect();
        if pk_cols.is_empty() {
            return Self::no_primary_key();
        }
        let Some(row) = ts.rows.get(row_index) else {
            return None;
        };
        // The row identity is resolved by the existing lookup, never by
        // the row number (00-contracts §4.3): the row index is only a
        // hint.
        let match_ = find_pending_for_row(ts, row_index, row, &pk_cols)?;
        let (key, change) = match_;

        let next = compute_unstage_delete_result(&change);
        let mut pending_changes = ts.pending_changes.clone();
        match next {
            None => {
                pending_changes.remove(&key);
            }
            Some(next) => {
                pending_changes.insert(key, next);
            }
        }

        let mut patch = Self::landed_patch();
        patch.pending_changes = Some(pending_changes);
        Some(PendingEditOutcome::write(patch))
    }

    /// `deleteRows` (React 433-444): delete rows — the selection
    /// semantics first (the unique filter), then the delete mark.
    pub fn delete_rows(
        ts: &TableState,
        row_indices: &[usize],
    ) -> Option<PendingEditOutcome> {
        let mut unique = row_indices.to_vec();
        unique.sort_unstable();
        unique.dedup();
        if unique.is_empty() {
            return None;
        }
        // Deleting does not change the selection semantics
        // (00-contracts §4, GS2-D2): the row set is owned by `mode`, so
        // a `cell` selection must stay `cell` + empty rows while the
        // delete scope lives in the derived selector, and row identity
        // is carried by `rowIdentityAnchors` inside `stageRowDelete`.
        Self::stage_row_delete(ts, &unique)
    }

    /// `rollbackPendingChanges` (React 446-460): drop every staged
    /// change and re-read the page.
    pub fn rollback_pending_changes(ts: &TableState) -> Option<PendingEditOutcome> {
        let mut patch = Self::landed_patch();
        patch.pending_changes = Some(HashMap::new());
        patch.draft_inserts = Some(HashMap::new());
        patch.draft_insert_order = Some(Vec::new());
        patch.highlighted_new_row_key = Some(None);
        let mut outcome = PendingEditOutcome::write(patch);
        outcome.reload = true;
        let _ = ts;
        Some(outcome)
    }
}

/// The host's patch application — the `applyColumnToRows` loop's own
/// working-state mirror (React's `read` re-reads the panel fresh on
/// every call, so the loop sees the previous write).
pub fn apply_patch(ts: &mut TableState, patch: &PendingEditPatch) {
    if let Some(rows) = &patch.rows {
        ts.rows = rows.clone();
    }
    if let Some(pending_changes) = &patch.pending_changes {
        ts.pending_changes = pending_changes.clone();
    }
    if let Some(row_identity_anchors) = &patch.row_identity_anchors {
        ts.row_identity_anchors = row_identity_anchors.clone();
    }
    if patch.preview_plan.is_some() {
        ts.preview_plan = None;
    }
    if let Some(pending_status) = patch.pending_status {
        ts.pending_status = pending_status;
    }
    if let Some(editing_cell) = &patch.editing_cell {
        ts.editing_cell = editing_cell.clone();
    }
    if let Some(error) = &patch.error {
        ts.error = error.clone();
    }
    if let Some(draft_inserts) = &patch.draft_inserts {
        ts.draft_inserts = draft_inserts.clone();
    }
    if let Some(draft_insert_order) = &patch.draft_insert_order {
        ts.draft_insert_order = draft_insert_order.clone();
    }
    if let Some(highlighted_new_row_key) = &patch.highlighted_new_row_key {
        ts.highlighted_new_row_key = highlighted_new_row_key.clone();
    }
}
