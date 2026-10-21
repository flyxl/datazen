//! Primary-key identity and column guards for the table change path.
//!
//! `affected == 1` is not an identity check: a WHERE clause built from the
//! wrong column set can still match exactly one row, and an UPDATE built from
//! a partial key silently edits whatever rows the missing column would have
//! excluded. The plan therefore compares the submitted identity against the
//! table's real primary key **before** any SQL is generated, and refuses the
//! plan outright when it cannot read that key — a silently skipped guard is
//! indistinguishable from a guard that passed.

use super::change_set::{PendingRowChange, RowChangeTableContext};
use crate::error::FeatureError;
use datazen_driver_api::Value;
use datazen_runtime::schema_cache::CachedColumns;

/// Reject a row change that cannot address exactly one existing row.
///
/// `build_update_statement` / `build_delete_statement` happily render an empty
/// `WHERE`, which would touch every row in the table. `identity_key` also
/// rejects these, but it reports one generic message; this guard names the
/// operation and the offending columns so the refusal is diagnosable.
pub fn validate_identity_shape(
    operation: &str,
    change: &PendingRowChange,
) -> Result<(), FeatureError> {
    if change.row_identity.is_empty() {
        return Err(FeatureError::Validation(format!(
            "{operation} requires the table's primary-key identity; the row identity is empty, so no column can locate the row"
        )));
    }
    let null_columns: Vec<&str> = change
        .row_identity
        .iter()
        .filter(|(_, value)| {
            value
                .as_ref()
                .map_or(true, |value| matches!(value, Value::Null))
        })
        .map(|(column, _)| column.as_str())
        .collect();
    if !null_columns.is_empty() {
        return Err(FeatureError::Validation(format!(
            "{operation} row identity has NULL primary-key values: {}; a NULL cannot address one row",
            null_columns.join(", ")
        )));
    }
    Ok(())
}

/// Cheap shape checks that need no catalog: empty or NULL identities.
///
/// Runs before the schema read so an empty identity is reported as such even
/// when the catalog itself is unreachable.
pub fn validate_identity_shapes(changes: &[PendingRowChange]) -> Result<(), FeatureError> {
    for change in changes {
        if change.insert_marked {
            // A new row has no identity; a contradictory identity is handled
            // when the draft is canonicalized.
            continue;
        }
        validate_identity_shape(
            if change.delete_marked {
                "DELETE"
            } else {
                "UPDATE"
            },
            change,
        )?;
    }
    Ok(())
}

/// Require `row_identity` to name exactly the table's primary key.
///
/// Both directions matter: a missing column matches more rows than the user
/// selected, and an extra column (usually a stale or renamed key) matches rows
/// the predicate never meant to address.
pub fn validate_identity_against_primary_key(
    operation: &str,
    change: &PendingRowChange,
    table: &RowChangeTableContext,
    columns: &CachedColumns,
) -> Result<(), FeatureError> {
    if columns.primary_keys.is_empty() {
        return Err(FeatureError::Validation(format!(
            "{operation} requires the table's primary key, but '{}' reports none; a grid change cannot address one row",
            table.table
        )));
    }
    let expected: std::collections::BTreeSet<&str> =
        columns.primary_keys.iter().map(String::as_str).collect();
    let submitted: std::collections::BTreeSet<&str> =
        change.row_identity.keys().map(String::as_str).collect();
    let missing: Vec<&str> = expected.difference(&submitted).copied().collect();
    if !missing.is_empty() {
        return Err(FeatureError::Validation(format!(
            "{operation} row identity is missing primary-key column(s) of '{}': {}; the primary key is ({})",
            table.table,
            missing.join(", "),
            columns.primary_keys.join(", ")
        )));
    }
    let unexpected: Vec<&str> = submitted.difference(&expected).copied().collect();
    if !unexpected.is_empty() {
        return Err(FeatureError::Validation(format!(
            "{operation} row identity contains column(s) that are not part of '{}' primary key: {}; the primary key is ({})",
            table.table,
            unexpected.join(", "),
            columns.primary_keys.join(", ")
        )));
    }
    Ok(())
}

/// Reject writes to columns the table does not have.
///
/// Without this the database raises an opaque syntax/unknown-column error
/// *after* the batch started, and for inserts the user has no way to tell a
/// typo in a draft cell from a stale schema.
pub fn validate_written_columns(
    operation: &str,
    change: &PendingRowChange,
    table: &RowChangeTableContext,
    columns: &CachedColumns,
) -> Result<(), FeatureError> {
    if change.changed_columns.is_empty() {
        return Ok(());
    }
    let known: std::collections::BTreeSet<&str> = columns
        .columns
        .iter()
        .map(|column| column.name.as_str())
        .collect();
    let unknown: Vec<&str> = change
        .changed_columns
        .iter()
        .map(String::as_str)
        .filter(|column| !known.contains(column))
        .collect();
    if unknown.is_empty() {
        return Ok(());
    }
    Err(FeatureError::Validation(format!(
        "grid.insert.unknownColumn: {operation} writes column(s) that do not exist on '{}': {}",
        table.table,
        unknown.join(", ")
    )))
}
