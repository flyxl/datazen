//! New-row (INSERT) normalization and statement building.
//!
//! Statement building for UPDATE/DELETE takes only the identity + written
//! columns, so these tests avoid every wire type; the INSERT branch joins the
//! same plan object and keeps its own rules.

use super::change_set::{PendingRowChange, PlannedStatement};
use crate::table_editing::error::TableEditingError as FeatureError;
use datazen_driver_api::{BoundSqlStatement, DatabaseDriver, Value};
use std::collections::{BTreeMap, HashSet};

// New-row (INSERT) plan construction for the table change path.
//
// A new row travels on [`PendingRowChange`] with `insert_marked = true`:
// `changed_columns` is the exact INSERT column list (a column absent from it
// keeps the database default — auto-increment, generated column,
// `DEFAULT now()`), and `current_values` holds the bound values, where `None`
// means SQL `NULL`. Reusing the change payload instead of a separate draft DTO
// is what lets `changes_fingerprint` cover new-row content and lets
// `plan_changes` rebuild the plan without losing information.
//
// The three intents therefore stay distinguishable on the wire:
// `Value(v)` → written value, `None`/`Value::Null` → SQL `NULL`, and a column
// absent from `changed_columns` → not written at all.

/// Canonicalize one new-row draft and drop anything that is not a write.
///
/// New rows have no identity (the database assigns it), so the primary-key
/// checks applied to UPDATE/DELETE deliberately do not run here. The payload
/// must *prove* that by leaving `row_identity`/`original_values` empty; a row
/// that carries them is rejected instead of having them silently cleared.
/// An all-`Unset` row is legal: it becomes `INSERT INTO t DEFAULT VALUES`.
pub fn canonicalize_insert(change: &PendingRowChange) -> Result<PendingRowChange, FeatureError> {
    if change.delete_marked {
        return Err(FeatureError::Validation(
            "grid.commit.conflictingIntents: a new row cannot also be marked for delete".into(),
        ));
    }
    // The marker replaces the structural `PendingRowInsert` type, so the
    // "carries no existing-row state" guarantee has to be enforced here: a row
    // with an identity or original values describes an existing row and the
    // insert mark would silently discard that intent.
    if !change.row_identity.is_empty() || !change.original_values.is_empty() {
        return Err(FeatureError::Validation(
            "grid.commit.conflictingIntents: a row cannot both change an existing row and be a new row; remove its row identity/original values or clear the insert mark".into(),
        ));
    }
    let mut normalized = change.clone();
    normalized.row_identity = BTreeMap::new();
    normalized.original_values = BTreeMap::new();
    normalized.delete_marked = false;
    normalized.changed_columns.sort();
    normalized.changed_columns.dedup();
    for column in &normalized.changed_columns {
        if !normalized.current_values.contains_key(column) {
            return Err(FeatureError::Validation(format!(
                "Missing current value for changed column '{column}'"
            )));
        }
    }
    // `PlannedStatement` invariant: `current_values` covers exactly the
    // written columns, nothing else. A draft can still carry values for cells
    // the user cleared, but they are not part of this INSERT.
    let written: HashSet<&str> = normalized
        .changed_columns
        .iter()
        .map(String::as_str)
        .collect();
    normalized
        .current_values
        .retain(|column, _| written.contains(column.as_str()));
    Ok(normalized)
}

/// Build the INSERT preview for one new-row draft.
///
/// `sql_template` is display-only: execution rebuilds the bound statement from
/// the immutable plan metadata below.
pub fn planned_insert_statement(
    driver: &dyn DatabaseDriver,
    table: &str,
    change: &PendingRowChange,
) -> Result<PlannedStatement, FeatureError> {
    let statement = driver
        .build_insert_statement(
            table,
            &insert_values(&change.changed_columns, &change.current_values)?,
        )
        .map_err(FeatureError::Driver)?;
    let parameter_summary = change
        .changed_columns
        .iter()
        .filter_map(|column| {
            change.current_values.get(column).map(|value| {
                format!(
                    "INSERT {column}={}",
                    super::change_set::value_summary(value)
                )
            })
        })
        .collect();
    Ok(PlannedStatement {
        row_identity: BTreeMap::new(),
        original_values: BTreeMap::new(),
        current_values: change.current_values.clone(),
        changed_columns: change.changed_columns.clone(),
        sql_template: statement.sql,
        parameter_summary,
    })
}

/// Rebuild the executable INSERT from plan metadata (never from `sql_template`).
pub fn bound_insert_statement_for_plan(
    driver: &dyn DatabaseDriver,
    table: &str,
    planned: &PlannedStatement,
) -> Result<BoundSqlStatement, FeatureError> {
    driver
        .build_insert_statement(
            table,
            &insert_values(&planned.changed_columns, &planned.current_values)?,
        )
        .map_err(FeatureError::Driver)
}

/// Written columns, in the canonical (sorted) order, paired with their values.
///
/// A column present with `None` is SQL `NULL`; a missing column is a broken
/// plan invariant and is rejected rather than silently turned into `NULL`.
fn insert_values<'a>(
    columns: &'a [String],
    values: &'a BTreeMap<String, Option<Value>>,
) -> Result<Vec<(&'a str, Option<Value>)>, FeatureError> {
    columns
        .iter()
        .map(|column| {
            values
                .get(column)
                .cloned()
                .map(|value| (column.as_str(), value))
                .ok_or_else(|| {
                    FeatureError::Validation(format!(
                        "Missing current value for changed column '{column}'"
                    ))
                })
        })
        .collect()
}
