//! Result-grid write-back rules (DB-14).
//!
//! A cell of a **query result** may be written back only when the driver has
//! proved where the rows came from. This module owns that proof and the
//! verdict, so the desktop host and a web host reach the same answer:
//!
//! * [`evaluate_result_editability`] turns a driver's origin report into a
//!   verdict — writable, or read-only **with a reason**. The reason is what
//!   the user is shown; a grey cell without a reason is not an answer.
//! * [`build_result_cell_change_plan`] turns one cell edit into the ordinary
//!   change plan, after the request has passed the guards that must run
//!   **before any SQL is rendered**.
//!
//! Two rules shape everything here:
//!
//! * **Fail closed.** Anything the driver does not prove is read-only. A
//!   wrong "yes" costs a wrong `UPDATE`; a wrong "no" costs a grey cell.
//! * **The host owns no rules.** The host lends a session, reads the
//!   catalog's primary key and maps the error; everything that decides
//!   anything lives here.

use std::collections::{BTreeMap, BTreeSet};

use datazen_driver_api::{
    DatabaseDriver, ResultObjectKind, ResultOrigin, ResultOriginDetail, ResultShape,
    ResultTableRef, Value,
};
use datazen_runtime::schema_cache::CachedColumns;
use serde::{Deserialize, Serialize};

use crate::data_browsing::{
    build_row_change_plan, PendingRowChange, RowChangePlan, RowChangeTableContext,
};
use crate::error::FeatureError;

/// Why a result cannot be written back.
///
/// The front end renders one message per reason; the codes are the wire
/// contract, the messages are not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GridReadOnlyReason {
    /// The driver cannot say where the result came from.
    UnknownOrigin,
    /// The driver's report contradicts itself.
    AmbiguousOrigin,
    /// The rows come from more than one relation.
    MultiTable,
    /// The rows are aggregated.
    Aggregate,
    /// The rows are de-duplicated.
    Distinct,
    /// The rows come from a set operation (`UNION` and friends).
    SetOperation,
    /// The rows come from a derived source.
    DerivedSource,
    /// The rows come from a view rather than a base table.
    View,
    /// The source table has no primary key.
    NoPrimaryKey,
    /// The primary key is not fully present in the result as a straight copy.
    KeyNotSelected,
    /// The session or connection is read-only.
    SessionReadOnly,
}

/// The `grid.result.*` rejection codes (00-contracts §8.2, rows 33–37).
///
/// These are the only codes this feature raises; the front end classifies on
/// the exact prefix, so the spelling is part of the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultErrorCode {
    /// The driver cannot report where the result came from.
    OriginUnavailable,
    /// The driver's report is self-contradictory.
    OriginAmbiguous,
    /// The row has no unique, complete identity.
    RowNotAddressable,
    /// The request would write more than one row.
    MultiRowNotAllowed,
    /// The target column cannot be written.
    WriteDenied,
}

impl ResultErrorCode {
    /// The wire prefix the front end matches on.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OriginUnavailable => "grid.result.originUnavailable",
            Self::OriginAmbiguous => "grid.result.originAmbiguous",
            Self::RowNotAddressable => "grid.result.rowNotAddressable",
            Self::MultiRowNotAllowed => "grid.result.multiRowNotAllowed",
            Self::WriteDenied => "grid.result.writeDenied",
        }
    }
}

impl std::fmt::Display for ResultErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One identity column, in both names.
///
/// A write-back statement always uses `source_column`; the interface always
/// shows `column`. Mixing them up writes a wrong column or a wrong table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityColumn {
    /// The name the result reports.
    pub column: String,
    /// The real column name on the base table.
    pub source_column: String,
}

/// The verdict on one result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultEditability {
    /// Whether the result can be written back at all.
    pub writable: bool,
    /// The key columns, in both names. Empty when the result is read-only.
    pub identity_columns: Vec<IdentityColumn>,
    /// Every column that is a straight copy of a base-table column, in both
    /// names. A cell whose column is missing from this list is not editable —
    /// the front end asks the server rather than deciding for itself, because
    /// the server is the only side that knows the provenance.
    pub editable_columns: Vec<IdentityColumn>,
    /// The relation the rows come from — the real name, never an alias.
    pub table: Option<ResultTableRef>,
    /// Why the result is read-only. `None` exactly when it is writable.
    pub reason: Option<GridReadOnlyReason>,
}

/// The verdict for a read-only result.
fn read_only(reason: GridReadOnlyReason) -> ResultEditability {
    ResultEditability {
        writable: false,
        identity_columns: Vec::new(),
        editable_columns: Vec::new(),
        table: None,
        reason: Some(reason),
    }
}

/// Decide whether a result may be written back.
///
/// `primary_keys` are the source table's key columns — the **normalised** list
/// from the catalog, never a re-derivation from the column flags, because the
/// two can disagree and the normalised list names the real key.
///
/// `result_column_count` is how many columns the executed result reports. It
/// is what makes a driver report checkable: a report that does not line up
/// with the result cannot be trusted at all.
pub fn evaluate_result_editability(
    origin: &ResultOrigin,
    primary_keys: &[String],
    session_writable: bool,
    result_column_count: usize,
) -> ResultEditability {
    if !session_writable {
        return read_only(GridReadOnlyReason::SessionReadOnly);
    }
    let detail = match origin {
        ResultOrigin::Undeterminable(_) => return read_only(GridReadOnlyReason::UnknownOrigin),
        ResultOrigin::Determined(detail) => detail,
    };
    if !is_self_consistent(detail, result_column_count) {
        return read_only(GridReadOnlyReason::AmbiguousOrigin);
    }
    if detail.set_operation.is_some() {
        return read_only(GridReadOnlyReason::SetOperation);
    }
    if detail.has_derived_source {
        return read_only(GridReadOnlyReason::DerivedSource);
    }
    if detail.aggregated {
        return read_only(GridReadOnlyReason::Aggregate);
    }
    if detail.deduplicated {
        return read_only(GridReadOnlyReason::Distinct);
    }
    match detail.object_kind {
        ResultObjectKind::BaseTable => {}
        ResultObjectKind::View | ResultObjectKind::MaterializedView => {
            return read_only(GridReadOnlyReason::View)
        }
        // The driver could not confirm the relation is a base table. That is
        // not the same as "it is a view", so it does not get the view's
        // message.
        ResultObjectKind::Unknown => return read_only(GridReadOnlyReason::UnknownOrigin),
    }
    if detail.shape == ResultShape::Unknown {
        return read_only(GridReadOnlyReason::UnknownOrigin);
    }
    if detail.shape != ResultShape::SingleBaseTable || detail.referenced_tables != 1 {
        return read_only(GridReadOnlyReason::MultiTable);
    }
    if primary_keys.is_empty() {
        return read_only(GridReadOnlyReason::NoPrimaryKey);
    }
    let Some(identity) = identity_columns(detail, primary_keys) else {
        return read_only(GridReadOnlyReason::KeyNotSelected);
    };
    let editable_columns = detail
        .columns
        .iter()
        .filter(|column| column.passthrough)
        .filter_map(|column| {
            Some(IdentityColumn {
                column: column.column.clone(),
                source_column: column.source_column.clone()?,
            })
        })
        .collect();
    ResultEditability {
        writable: true,
        identity_columns: identity,
        editable_columns,
        table: detail.table.clone(),
        reason: None,
    }
}

/// Whether the driver's report can be believed at all.
///
/// A report that contradicts itself is worse than no report: it would let the
/// host build a write-back statement out of half of it.
fn is_self_consistent(detail: &ResultOriginDetail, result_column_count: usize) -> bool {
    if detail.columns.len() != result_column_count {
        return false;
    }
    let names: BTreeSet<&str> = detail
        .columns
        .iter()
        .map(|column| column.column.as_str())
        .collect();
    if names.len() != detail.columns.len() {
        return false;
    }
    for column in &detail.columns {
        if !column.passthrough {
            continue;
        }
        // A straight copy without a source column cannot be written back to.
        if column.source_column.is_none() {
            return false;
        }
        // A straight copy from a different relation is not this relation's
        // column at all.
        if column.table != detail.table {
            return false;
        }
    }
    // The relation and the shape must agree. An unknown shape is allowed to
    // carry no relation: "the driver cannot classify this" is a verdict of
    // its own, not a contradiction.
    match detail.shape {
        ResultShape::SingleBaseTable => {
            if detail.table.is_none() || detail.referenced_tables != 1 {
                return false;
            }
        }
        ResultShape::MultiTable | ResultShape::Derived => {
            if detail.table.is_some() {
                return false;
            }
        }
        ResultShape::Unknown => {}
    }
    true
}

/// Locate the key columns in the result.
///
/// A key column may appear under its own name (`SELECT id FROM t`) or under an
/// alias (`SELECT id AS order_id FROM t`) — the second one is still writable,
/// because the write-back uses the **source** name. What is not writable is a
/// key column that was wrapped in an expression: its value no longer comes
/// from the base column.
fn identity_columns(
    detail: &ResultOriginDetail,
    primary_keys: &[String],
) -> Option<Vec<IdentityColumn>> {
    let mut identity = Vec::with_capacity(primary_keys.len());
    for key in primary_keys {
        if let Some(column) = detail.columns.iter().find(|column| column.column == *key) {
            if !column.passthrough {
                return None;
            }
            identity.push(IdentityColumn {
                column: column.column.clone(),
                source_column: column.source_column.clone()?,
            });
            continue;
        }
        let column = detail.columns.iter().find(|column| {
            column.passthrough && column.source_column.as_deref() == Some(key.as_str())
        })?;
        identity.push(IdentityColumn {
            column: column.column.clone(),
            source_column: key.clone(),
        });
    }
    Some(identity)
}

/// Which rejection a read-only reason produces.
///
/// The two reasons that are about the **origin** keep their own codes; every
/// other reason is a refusal to write, which is what `writeDenied` says.
pub fn read_only_error_code(reason: GridReadOnlyReason) -> ResultErrorCode {
    match reason {
        GridReadOnlyReason::UnknownOrigin => ResultErrorCode::OriginUnavailable,
        GridReadOnlyReason::AmbiguousOrigin => ResultErrorCode::OriginAmbiguous,
        GridReadOnlyReason::SessionReadOnly
        | GridReadOnlyReason::MultiTable
        | GridReadOnlyReason::Aggregate
        | GridReadOnlyReason::Distinct
        | GridReadOnlyReason::SetOperation
        | GridReadOnlyReason::DerivedSource
        | GridReadOnlyReason::View
        | GridReadOnlyReason::NoPrimaryKey
        | GridReadOnlyReason::KeyNotSelected => ResultErrorCode::WriteDenied,
    }
}

/// One cell edit of a query result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultCellChangeRequest {
    /// The connection the result was read from.
    pub connection_id: String,
    /// The session the result was read from.
    pub db_session_id: String,
    /// The driver that read the result.
    pub driver_type: String,
    /// The database the relation lives in.
    pub database: Option<String>,
    /// The schema the relation lives in.
    pub schema: Option<String>,
    /// The real name of the relation.
    pub table: String,
    /// The key columns, in both names.
    pub identity_columns: Vec<IdentityColumn>,
    /// The row identity: **source** column name → the value the result showed.
    pub row_identity: BTreeMap<String, Option<Value>>,
    /// The result column the user edited.
    pub column: String,
    /// The real column name of the edited result column.
    pub source_column: String,
    /// The value the result showed in that column.
    pub original_value: Option<Value>,
    /// The new value.
    pub value: Option<Value>,
    /// How many rows the request would write. Only `1` is allowed.
    pub row_count: u32,
}

/// The guards that must run **before any SQL is rendered**.
///
/// Every one of them refuses a request that could address more than one row,
/// or no row at all. A statement without a `WHERE` would touch every row of
/// the table, so these checks are not a nicety: they are the difference
/// between a write and a disaster.
pub fn validate_result_cell_request(request: &ResultCellChangeRequest) -> Result<(), FeatureError> {
    if request.row_count != 1 {
        return Err(FeatureError::Validation(format!(
            "{}: editing many query-result rows at once is not supported; the request writes {} row(s)",
            ResultErrorCode::MultiRowNotAllowed,
            request.row_count
        )));
    }
    if request.row_identity.is_empty() {
        return Err(FeatureError::Validation(format!(
            "{}: the request carries no row identity, so no column can locate the row",
            ResultErrorCode::RowNotAddressable
        )));
    }
    let null_columns: Vec<&str> = request
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
            "{}: the row identity has NULL value(s) in column(s) {}; a NULL cannot address one row",
            ResultErrorCode::RowNotAddressable,
            null_columns.join(", ")
        )));
    }
    Ok(())
}

/// Build the change plan for one edited cell of a query result.
///
/// `editability` is the verdict the **server** just computed from a fresh
/// origin report — never one the client sent. The plan is the ordinary
/// change plan, so the existing fingerprint and `affected == 1` guards apply
/// unchanged.
pub fn build_result_cell_change_plan(
    driver: &dyn DatabaseDriver,
    request: &ResultCellChangeRequest,
    editability: &ResultEditability,
    columns: &CachedColumns,
) -> Result<RowChangePlan, FeatureError> {
    validate_result_cell_request(request)?;
    if !editability.writable {
        let reason = editability
            .reason
            .unwrap_or(GridReadOnlyReason::UnknownOrigin);
        let code = read_only_error_code(reason);
        return Err(FeatureError::Validation(format!(
            "{code}: this result cannot be written back ({reason:?})"
        )));
    }
    // The identity the **server** derived must be exactly the identity the
    // request carries. A request that names another key set is refused
    // instead of being "helpfully" completed.
    let expected: BTreeSet<&str> = editability
        .identity_columns
        .iter()
        .map(|column| column.source_column.as_str())
        .collect();
    let submitted: BTreeSet<&str> = request.row_identity.keys().map(String::as_str).collect();
    if expected != submitted {
        return Err(FeatureError::Validation(format!(
            "{}: the request's row identity does not match the result's key columns; expected ({}) but got ({})",
            ResultErrorCode::RowNotAddressable,
            expected.into_iter().collect::<Vec<_>>().join(", "),
            submitted.into_iter().collect::<Vec<_>>().join(", ")
        )));
    }
    // The edited column must be a straight copy of a real base column, and
    // the request must name that real column.
    let Some(editable) = editability
        .editable_columns
        .iter()
        .find(|column| column.column == request.column)
    else {
        return Err(FeatureError::Validation(format!(
            "{}: column '{}' is not a writable column of '{}'",
            ResultErrorCode::WriteDenied,
            request.column,
            request.table
        )));
    };
    if editable.source_column != request.source_column {
        return Err(FeatureError::Validation(format!(
            "{}: column '{}' of '{}' is the base column '{}', not '{}'",
            ResultErrorCode::WriteDenied,
            request.column,
            request.table,
            editable.source_column,
            request.source_column
        )));
    }

    let table = RowChangeTableContext {
        connection_id: request.connection_id.clone(),
        db_session_id: request.db_session_id.clone(),
        driver_type: request.driver_type.clone(),
        database: request.database.clone(),
        schema: request.schema.clone(),
        table: request.table.clone(),
    };
    let mut original_values = BTreeMap::new();
    original_values.insert(
        request.source_column.clone(),
        request.original_value.clone(),
    );
    let mut current_values = BTreeMap::new();
    current_values.insert(request.source_column.clone(), request.value.clone());
    let change = PendingRowChange {
        row_identity: request.row_identity.clone(),
        original_values,
        current_values,
        changed_columns: vec![request.source_column.clone()],
        delete_marked: false,
        insert_marked: false,
    };
    build_row_change_plan(driver, table, std::slice::from_ref(&change), columns)
}

#[cfg(test)]
mod tests;
