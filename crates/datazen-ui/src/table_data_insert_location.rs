//! T233 (a): where the row inserted by a commit ended up — port of
//! `packages/frontend/src/stores/tableData/insertLocation.ts` (158 lines).
//!
//! This batch does **not** implement the RETURNING backfill (04 §5 Step 6,
//! decision 7), so the rule is React's own three-step:
//!
//! 1. an insert statement that *does* carry a row identity is located
//!    exactly;
//! 2. otherwise a **candidate** is taken — the row with the largest value
//!    of a single comparable primary key — accepted only when that value
//!    grew compared to the page before the commit;
//! 3. otherwise the row is **not** located: the UI says so and must never
//!    claim it can point at the new row.
//!
//! Rule 2 is a candidate, not a guarantee — a concurrent insert can
//! produce the same maximum — which is exactly why the UI wording stays
//! neutral.
//!
//! ## What maps where
//!
//! | React | here |
//! |---|---|
//! | `locateInsertedRow` | [`locate_inserted_row`] |
//! | `InsertLocationResult` / `InsertLocation` | [`InsertLocationResult`] / [`InsertStatus`] |
//! | `CommitStatementResult` (`lib/tableChanges.ts:150`) | [`CommitStatementResult`] |
//! | `buildRowIdentity` / `rowIdentityKey` | `table_data_dirty_index`'s same-name ports |
//!
//! ## Deliberate deviations from the React source
//!
//! * **The identity match compares structurally, not by
//!   `JSON.stringify`.** React stringifies both sides before comparing;
//!   identity values are primary-key scalars (numbers, strings,
//!   booleans), where structural `Value` equality is exactly the
//!   stringified equality (the object-key-order quirk `JSON.stringify`
//!   carries only affects compound values, which a primary key never
//!   produces through [`build_row_identity`]'s stable-value rule).
//! * **`CommitStatementResult` rides this module** — its lib owner
//!   (`lib/tableChanges.ts`) is ported function-by-function into
//!   `table_data_dirty_index.rs` as its tasks land, and the statement
//!   result shape belongs to the consumer that reads it (the same rule
//!   the dirty index's own docs state). `operation` mirrors as the enum
//!   [`CommitOperation`], `rowIdentity` as the ported [`RowIdentity`].
//! * **A missing column is "not comparable", like `undefined`.** React's
//!   `row[column] === undefined` fails `isComparableValue`; the port's
//!   `HashMap` miss answers the same (`None` from `get`), and a `null`
//!   JSON value fails it too (React's `null` also `!isComparableValue`).

use std::collections::HashMap;

use serde_json::Value;

use crate::table_data_connection_state::ColumnSchema;
use crate::table_data_dirty_index::{build_row_identity, row_identity_key, RowIdentity};

/// React `CommitStatementResult['operation']` (`tableChanges.ts:150`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommitOperation {
    Insert,
    Update,
    Delete,
}

/// React `CommitStatementResult` (`tableChanges.ts:150`): one statement
/// the commit reported.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CommitStatementResult {
    pub operation: CommitOperation,
    pub row_identity: RowIdentity,
    pub affected_rows: u64,
}

/// React `InsertLocation` (`tableChanges.ts:176`): where the inserted
/// row ended up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InsertStatus {
    /// The new row was found on the refreshed page.
    Located,
    /// The commit succeeded but the new row could not be located; the
    /// UI must **not** claim the row was highlighted (04 §4.1 — the
    /// regular path, not an error).
    Unavailable,
    /// The commit contained no insert at all.
    NotApplicable,
}

/// React `InsertLocationResult`: the status plus the located row's
/// identity key (`None` when the row could not be located).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InsertLocationResult {
    pub status: InsertStatus,
    pub key: Option<String>,
}

const UNAVAILABLE: InsertLocationResult =
    InsertLocationResult { status: InsertStatus::Unavailable, key: None };
const NOT_APPLICABLE: InsertLocationResult =
    InsertLocationResult { status: InsertStatus::NotApplicable, key: None };

/// Identity key of a row, or `None` when the row has no stable identity
/// (React's `identityKeyOf` — the try/catch is the ported
/// `row_identity_key`'s `None`).
fn identity_key_of(row: &HashMap<String, Value>, primary_key_columns: &[ColumnSchema]) -> Option<String> {
    row_identity_key(&build_row_identity(row, primary_key_columns)?)
}

/// Does the row carry exactly the values of the given identity?
/// (React's `identityMatches`: every identity column present, non-null,
/// and equal.)
fn identity_matches(row: &HashMap<String, Value>, identity: &RowIdentity) -> bool {
    if identity.is_empty() {
        return false;
    }
    identity
        .iter()
        .all(|(column, expected)| match row.get(column) {
            // `undefined` (a miss) and `null` both refuse the match.
            None | Some(Value::Null) => false,
            Some(actual) => actual == expected,
        })
}

/// React's `compareValues`: numbers subtract, strings order, anything
/// else is `0` (equal) — the caller only reads the sign.
fn compare_values(left: &Value, right: &Value) -> f64 {
    if let (Value::Number(left), Value::Number(right)) = (left, right) {
        return left.as_f64().unwrap_or(0.0) - right.as_f64().unwrap_or(0.0);
    }
    if let (Value::String(left), Value::String(right)) = (left, right) {
        return match left.cmp(right) {
            std::cmp::Ordering::Less => -1.0,
            std::cmp::Ordering::Greater => 1.0,
            std::cmp::Ordering::Equal => 0.0,
        };
    }
    0.0
}

/// The comparable maximum's carrier: the row and its winning value.
#[derive(Clone, Debug)]
struct ComparableMax {
    row: HashMap<String, Value>,
    value: Value,
}

/// Is the value a number or a string (React's `isComparableValue`)?
fn is_comparable_value(value: &Value) -> bool {
    matches!(value, Value::Number(_) | Value::String(_))
}

/// The largest value of a column, when every row carries a comparable
/// one of a uniform kind. `comparable: false` means "the values cannot
/// be ordered" (a boolean, a compound value, a null, a miss, or a mix
/// of numbers and strings) — such a column can never locate a row.
fn comparable_max(
    rows: &[HashMap<String, Value>],
    column: &str,
) -> (Option<ComparableMax>, bool) {
    if rows.is_empty() {
        return (None, true);
    }
    let mut max: Option<ComparableMax> = None;
    let mut number_kind = None;
    for row in rows {
        let Some(raw) = row.get(column) else {
            return (None, false); // a miss: React's `undefined`
        };
        if !is_comparable_value(raw) {
            return (None, false);
        }
        let row_kind = matches!(raw, Value::Number(_));
        match number_kind {
            None => number_kind = Some(row_kind),
            Some(kind) if kind != row_kind => return (None, false),
            _ => {}
        }
        let exceeds = match &max {
            None => true,
            Some(current) => compare_values(raw, &current.value) > 0.0,
        };
        if exceeds {
            max = Some(ComparableMax { row: row.clone(), value: raw.clone() });
        }
    }
    (max, true)
}

/// Locate the row inserted by a commit (React's `locateInsertedRow`).
///
/// * `previous_rows` — the rows as they were before the commit (the
///   candidate rule needs the previous maximum to tell "the new row"
///   from "a row that was already there");
/// * `refreshed_rows` — the rows of the page after the refresh;
/// * `primary_key_columns` — the table's primary key columns;
/// * `statements` — the statements the commit reported.
pub(crate) fn locate_inserted_row(
    previous_rows: &[HashMap<String, Value>],
    refreshed_rows: &[HashMap<String, Value>],
    primary_key_columns: &[ColumnSchema],
    statements: &[CommitStatementResult],
) -> InsertLocationResult {
    let inserts: Vec<&CommitStatementResult> =
        statements.iter().filter(|s| s.operation == CommitOperation::Insert).collect();
    if inserts.is_empty() {
        return NOT_APPLICABLE;
    }

    // 1. An insert that carries an identity is located exactly.
    for statement in inserts {
        if statement.row_identity.is_empty() {
            continue;
        }
        let Some(match_row) =
            refreshed_rows.iter().find(|row| identity_matches(row, &statement.row_identity))
        else {
            continue;
        };
        if let Some(key) = identity_key_of(match_row, primary_key_columns) {
            return InsertLocationResult { status: InsertStatus::Located, key: Some(key) };
        }
    }

    // 2. The candidate rule: one comparable primary key, and its maximum
    //    grew.
    if primary_key_columns.len() != 1 {
        return UNAVAILABLE;
    }
    let column = primary_key_columns[0].name.as_str();
    let (refreshed_max, refreshed_comparable) = comparable_max(refreshed_rows, column);
    if !refreshed_comparable {
        return UNAVAILABLE;
    }
    let Some(refreshed_max) = refreshed_max else {
        return UNAVAILABLE;
    };
    let (previous_max, _) = comparable_max(previous_rows, column);
    match previous_max {
        None => {
            // Nothing comparable before the commit: only a page that
            // holds exactly one row can be attributed to the insert.
            if refreshed_rows.len() != 1 {
                return UNAVAILABLE;
            }
        }
        Some(previous_max) => {
            // The maximum did not grow, so the candidate is a row that
            // was already there: highlighting it would point at the
            // wrong row.
            if compare_values(&refreshed_max.value, &previous_max.value) <= 0.0 {
                return UNAVAILABLE;
            }
        }
    }

    match identity_key_of(&refreshed_max.row, primary_key_columns) {
        Some(key) => InsertLocationResult { status: InsertStatus::Located, key: Some(key) },
        None => UNAVAILABLE,
    }
}
