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

use crate::schema::CachedColumns;
use datazen_driver_api::{
    DatabaseDriver, ResultObjectKind, ResultOrigin, ResultOriginDetail, ResultShape,
    ResultTableRef, Value,
};
use serde::{Deserialize, Serialize};

use super::error::TableEditingError;
use super::{build_row_change_plan, PendingRowChange, RowChangePlan, RowChangeTableContext};

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
    /// The **locating** key columns, in both names.
    ///
    /// They are reported even when the result is read-only: the front end
    /// needs them to decide whether *a row* can be addressed (the key value
    /// must not be NULL, and no other row on the page may share the identity),
    /// and those are page-level facts that do not depend on the result being
    /// writable.
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
///
/// The locating key columns are **kept**: they are a fact about the table, not
/// about the verdict, and the page-level guards need them.
fn read_only(
    reason: GridReadOnlyReason,
    identity_columns: Vec<IdentityColumn>,
) -> ResultEditability {
    ResultEditability {
        writable: false,
        identity_columns,
        editable_columns: Vec::new(),
        table: None,
        reason: Some(reason),
    }
}

/// The locating key columns of a result, as far as they are known.
///
/// They come from the **catalog**, not from the origin verdict: the catalog's
/// normalised key list is what the narrow write path re-verifies the identity
/// against, so it is the authority. The origin verdict can only add the
/// *aliased* case (a key under another name), which the catalog cannot know.
fn locating_key_columns(
    origin: &ResultOrigin,
    primary_keys: &[String],
    result_columns: &[String],
) -> Vec<IdentityColumn> {
    let mapped = match origin {
        ResultOrigin::Determined(detail) if is_self_consistent(detail, result_columns.len()) => {
            Some(detail)
        }
        _ => None,
    };
    let mut identity = Vec::with_capacity(primary_keys.len());
    for key in primary_keys {
        if let Some(detail) = mapped {
            if let Some(column) = detail.columns.iter().find(|column| {
                column.passthrough && column.source_column.as_deref() == Some(key.as_str())
            }) {
                identity.push(IdentityColumn {
                    column: column.column.clone(),
                    source_column: key.clone(),
                });
                continue;
            }
        }
        // The plain case: the key appears in the result under its own name.
        // `SELECT *` cannot rename a column, so this is the whole story on the
        // narrow channel.
        if result_columns.iter().any(|name| name == key) {
            identity.push(IdentityColumn {
                column: key.clone(),
                source_column: key.clone(),
            });
        }
    }
    identity
}

/// Decide whether a result may be written back.
///
/// `primary_keys` are the source table's key columns — the **normalised** list
/// from the catalog, never a re-derivation from the column flags, because the
/// two can disagree and the normalised list names the real key.
///
/// `result_columns` are the column names the executed result reports. They
/// are what makes a driver report checkable (a report that does not line up
/// with the result cannot be trusted at all) and what lets the locating key
/// columns be found on the narrow channel.
pub fn evaluate_result_editability(
    origin: &ResultOrigin,
    primary_keys: &[String],
    result_columns: &[String],
    session_writable: bool,
) -> ResultEditability {
    let result_column_count = result_columns.len();
    // The locating key columns are computed **before** the verdict: a refusal
    // must not take them away, or the page-level guards would have nothing to
    // work with.
    let locating = locating_key_columns(origin, primary_keys, result_columns);
    if !session_writable {
        return read_only(GridReadOnlyReason::SessionReadOnly, locating);
    }
    let detail = match origin {
        ResultOrigin::Undeterminable(_) => {
            return read_only(GridReadOnlyReason::UnknownOrigin, locating)
        }
        ResultOrigin::Determined(detail) => detail,
    };
    if !is_self_consistent(detail, result_column_count) {
        return read_only(GridReadOnlyReason::AmbiguousOrigin, locating);
    }
    if detail.set_operation.is_some() {
        return read_only(GridReadOnlyReason::SetOperation, locating);
    }
    if detail.has_derived_source {
        return read_only(GridReadOnlyReason::DerivedSource, locating);
    }
    if detail.aggregated {
        return read_only(GridReadOnlyReason::Aggregate, locating);
    }
    if detail.deduplicated {
        return read_only(GridReadOnlyReason::Distinct, locating);
    }
    match detail.object_kind {
        ResultObjectKind::BaseTable => {}
        ResultObjectKind::View | ResultObjectKind::MaterializedView => {
            return read_only(GridReadOnlyReason::View, locating)
        }
        // The driver could not confirm the relation is a base table. That is
        // not the same as "it is a view", so it does not get the view's
        // message.
        ResultObjectKind::Unknown => return read_only(GridReadOnlyReason::UnknownOrigin, locating),
    }
    if detail.shape == ResultShape::Unknown {
        return read_only(GridReadOnlyReason::UnknownOrigin, locating);
    }
    if detail.shape != ResultShape::SingleBaseTable || detail.referenced_tables != 1 {
        return read_only(GridReadOnlyReason::MultiTable, locating);
    }
    if primary_keys.is_empty() {
        return read_only(GridReadOnlyReason::NoPrimaryKey, locating);
    }
    let Some(identity) = identity_columns(detail, primary_keys) else {
        return read_only(GridReadOnlyReason::KeyNotSelected, locating);
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

/// The identity a result-cell write is aimed at.
///
/// A session context mixes two kinds of facts, and only one of them is about
/// the write target:
///
/// * **where the write goes and as whom** — the namespace and the effective
///   identity. These say whether a retained result still names its target.
/// * **how the session happens to be running** — transaction state, autocommit
///   and the search path. These change *while a statement runs* (`BEGIN`, an
///   implicit transaction, `SET search_path`) and say nothing about the target.
///
/// The narrow write gate compares two contexts captured at different moments
/// and produced by different projections, so comparing the contexts themselves
/// was the same defect as comparing two shapes that merely look alike. This
/// type is the **single representation of "the same target"**: both sides go
/// through [`WriteTarget::of`], and only these fields can make the gate refuse.
/// Mode fields are not here, so they cannot make a result permanently
/// unwritable by changing on their own.
///
/// `confidence` is deliberately not part of the target either: it answers "how
/// much we know", not "where the write points". Note that a degraded
/// confidence **does not** imply an incomplete namespace — a driver can report
/// `Partial` while still naming the full namespace (a PostgreSQL transaction
/// that aborted keeps its last observed namespace; SQLite reads the pragmas
/// but marks the whole observation `Partial`). So this type does **not** refuse
/// those writes, and it is not what keeps a redirected write from landing.
///
/// That job belongs to the relation re-check performed immediately before the
/// write: the table/database/schema used for the `UPDATE` (and for qualifying
/// it) all come from the frozen relation, and they are compared against a
/// fresh `get_table_schema` answer. Removing that re-check makes a redirected
/// write land — verified by mutation — so do not treat this type as the guard
/// against a moved target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteTarget {
    pub namespace: crate::connection::domain::NamespaceTarget,
    /// The identity the write executes as. `None` and an empty string are the
    /// same fact — "the backend reported no identity" — and normalize to
    /// `None`, because two projections spelled that fact differently.
    pub identity: Option<String>,
}

impl WriteTarget {
    /// The target `context` names.
    ///
    /// Empty strings are treated as "not reported": an absent level and a
    /// level that projected to `""` are the same namespace, not two.
    pub fn of(context: &crate::execution::SessionContext) -> Self {
        fn value(value: Option<&String>) -> Option<String> {
            value.filter(|value| !value.is_empty()).cloned()
        }
        Self {
            namespace: crate::connection::domain::NamespaceTarget::new(
                value(context.namespace.database.as_ref()),
                value(context.namespace.catalog.as_ref()),
                value(context.namespace.schema.as_ref()),
                context.namespace.path.clone(),
            ),
            identity: value(context.effective_identity.as_ref()),
        }
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
    /// The statement the result came from. The server re-reads the origin from
    /// it before it writes anything — a cached verdict may be stale.
    pub sql: String,
    /// The column names the executed result reports. The server needs them to
    /// re-derive the origin: `SELECT *` names no column itself.
    pub result_columns: Vec<String>,
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
pub fn validate_result_cell_request(
    request: &ResultCellChangeRequest,
) -> Result<(), TableEditingError> {
    if request.row_count != 1 {
        return Err(TableEditingError::Validation(format!(
            "{}: editing many query-result rows at once is not supported; the request writes {} row(s)",
            ResultErrorCode::MultiRowNotAllowed,
            request.row_count
        )));
    }
    if request.row_identity.is_empty() {
        return Err(TableEditingError::Validation(format!(
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
        return Err(TableEditingError::Validation(format!(
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
) -> Result<RowChangePlan, TableEditingError> {
    validate_result_cell_request(request)?;
    if !editability.writable {
        let reason = editability
            .reason
            .unwrap_or(GridReadOnlyReason::UnknownOrigin);
        let code = read_only_error_code(reason);
        return Err(TableEditingError::Validation(format!(
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
        return Err(TableEditingError::Validation(format!(
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
        return Err(TableEditingError::Validation(format!(
            "{}: column '{}' is not a writable column of '{}'",
            ResultErrorCode::WriteDenied,
            request.column,
            request.table
        )));
    };
    if editable.source_column != request.source_column {
        return Err(TableEditingError::Validation(format!(
            "{}: column '{}' of '{}' is the base column '{}', not '{}'",
            ResultErrorCode::WriteDenied,
            request.column,
            request.table,
            editable.source_column,
            request.source_column
        )));
    }

    // The relation the write targets is the one the **driver** named, not the
    // one the request claims.
    let table_name = editability
        .table
        .as_ref()
        .map(|table| table.name.clone())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| request.table.clone());
    let table = RowChangeTableContext {
        connection_id: request.connection_id.clone(),
        db_session_id: request.db_session_id.clone(),
        driver_type: request.driver_type.clone(),
        database: request.database.clone(),
        schema: request.schema.clone(),
        table: table_name,
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
