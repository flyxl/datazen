//! Keyset (seek) pagination: the shared vocabulary and the shared algorithm.
//!
//! Reading a page by *offset* means the database counts and discards the rows
//! before the page. Reading it by *seek* means the page starts after a value
//! boundary that was read from a real row, so the cost of a page does not
//! grow with the depth of the pagination.
//!
//! The trade is that a seek needs a **stable total order**: two rows must
//! never compare equal, or the engine is free to return them in either order
//! and a page boundary becomes a guess. The host therefore decides whether the
//! order in force is stable (see `datazen_feature_core::services::table_paging`)
//! and the driver decides whether a given order can be expressed as a seek
//! condition (see [`crate::traits::DatabaseDriver::keyset_predicate_sql`]).
//!
//! Two things in here are deliberately **not** the same shape as
//! [`crate::types::Value`]:
//!
//! * [`CursorValue`] is the **wire** shape of a boundary value. `Value` is
//!   `#[serde(untagged)]`, so `[1, 2, 3]` deserializes as `Bytes` *or* `Json`
//!   and a string as `String` *or* `Timestamp`. A cursor travels
//!   "Rust → front end → Rust", and on the second hop the untagged form would
//!   silently change the variant: the predicate would compare against
//!   `X'7b226131…'` instead of `'{"a":1}'` — a wrong boundary with no error
//!   anywhere. [`CursorValue`] carries an explicit `kind` tag for exactly
//!   this reason.
//! * [`PagePagingInfo`] is what the host tells the caller about the page it
//!   just read. The caller believes **this**, not its own guess about which
//!   paging strategy is in force.

use serde::{Deserialize, Serialize};

use crate::traits::DatabaseDriver;
use crate::types::Value;

/// The strategy one page was read with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PagingMode {
    /// `page × pageSize` as an `OFFSET`. The historical shape, and the shape
    /// every driver still uses unless it says otherwise.
    Offset,
    /// A seek: the page starts after a value boundary read from a real row.
    /// The `page` number is then only an ordinal — "the Nth page I walked
    /// to" — and says nothing about where in the table the page starts.
    Keyset,
}

/// One ordering column and its direction.
///
/// The pair is what the host discloses as "the stable total order this page
/// was read in", including the key columns that were appended to make the
/// order total.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderColumn {
    pub column: String,
    #[serde(default)]
    pub descending: bool,
}

/// Why a page could not be read by seek.
///
/// The host picks the user-visible wording; the front end only needs to know
/// that the page fell back to `OFFSET` and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeysetUnavailableReason {
    /// The driver does not implement keyset paging.
    DriverUnsupported,
    /// The order in force is not a total order (no key column at all).
    NoStableOrder,
    /// A column of the order can be `NULL`, and `NULL` does not take part in
    /// comparisons — a seek across the `NULL` block would silently drop rows.
    NullableKeyColumn,
    /// A boundary value is `NULL`.
    NullBoundary,
    /// The driver refused to turn this order into a seek condition.
    PredicateUnavailable,
    /// The dialect has no pagination clause at all.
    NoPaginationClause,
    /// The cursor that came with the request does not belong to the current
    /// order or filter any more, so it was discarded.
    StaleCursor,
    /// The requested page is not reachable from the boundaries that were
    /// visited, so the read has to start over with an `OFFSET`.
    NotAdjacent,
}

impl std::fmt::Display for KeysetUnavailableReason {
    /// A phrase that says **why**, not which variant it was.
    ///
    /// The front end has its own wording for each reason; this one is for a
    /// message that reaches a log or a caller that reads the raw text.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            KeysetUnavailableReason::DriverUnsupported => {
                "this database does not support keyset paging"
            }
            KeysetUnavailableReason::NoStableOrder => {
                "this table has no key, so its rows have no stable order"
            }
            KeysetUnavailableReason::NullableKeyColumn => {
                "the order includes a column that can be NULL"
            }
            KeysetUnavailableReason::NullBoundary => "the page boundary is NULL",
            KeysetUnavailableReason::PredicateUnavailable => {
                "this order cannot be turned into a seek condition"
            }
            KeysetUnavailableReason::NoPaginationClause => {
                "this database cannot paginate this query"
            }
            KeysetUnavailableReason::StaleCursor => "the page boundary belongs to another order",
            KeysetUnavailableReason::NotAdjacent => {
                "the requested page is not reachable from the pages that were visited"
            }
        };
        f.write_str(text)
    }
}

/// What the host did about the paging of one page.
///
/// This travels with every table read. A caller that receives nothing (an
/// older back end) is reading `OFFSET` pages, exactly as before.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PagePagingInfo {
    /// The strategy that was actually used. **The front end believes this
    /// field, not its own guess.**
    pub mode: PagingMode,
    /// The real column order of this page, including the key columns that
    /// were appended to make the order total.
    pub order_columns: Vec<OrderColumn>,
    /// The key values of the last row of this page, to be sent back verbatim
    /// for the next page. `None` means there is no successor (the page is
    /// empty, the driver gave no boundary, or paging is not by seek).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_after_key: Option<Vec<CursorValue>>,
    /// Set when the page was **not** read by seek: why not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<KeysetUnavailableReason>,
    /// The page holds fewer rows than the page size while the row count says
    /// there are more pages: rows were deleted while the user was paging.
    #[serde(default)]
    pub drift_suspected: bool,
}

/// The wire shape of one boundary value.
///
/// Unlike [`Value`] this is **tagged**: `{"kind": "integer", "value": 42}`.
/// The tag is what survives the round trip through the front end — see the
/// module documentation for why that matters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum CursorValue {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Timestamp(String),
    Json(serde_json::Value),
}

impl CursorValue {
    /// The boundary value as the driver contract speaks it.
    ///
    /// The mapping is per variant — nothing is stringified on the way, so a
    /// binary key stays binary and a JSON key stays JSON.
    pub fn to_value(&self) -> Value {
        match self {
            CursorValue::Null => Value::Null,
            CursorValue::Bool(value) => Value::Bool(*value),
            CursorValue::Integer(value) => Value::Integer(*value),
            CursorValue::Float(value) => Value::Float(*value),
            CursorValue::String(value) => Value::String(value.clone()),
            CursorValue::Bytes(value) => Value::Bytes(value.clone()),
            CursorValue::Timestamp(value) => Value::Timestamp(value.clone()),
            CursorValue::Json(value) => Value::Json(value.clone()),
        }
    }

    /// The boundary value as it travels. `Value::Null` stays `Null` — it is a
    /// value here, not an absence.
    pub fn from_value(value: &Value) -> Self {
        match value {
            Value::Null => CursorValue::Null,
            Value::Bool(inner) => CursorValue::Bool(*inner),
            Value::Integer(inner) => CursorValue::Integer(*inner),
            Value::Float(inner) => CursorValue::Float(*inner),
            Value::String(inner) => CursorValue::String(inner.clone()),
            Value::Bytes(inner) => CursorValue::Bytes(inner.clone()),
            Value::Timestamp(inner) => CursorValue::Timestamp(inner.clone()),
            Value::Json(inner) => CursorValue::Json(inner.clone()),
        }
    }
}

/// The portable seek condition for a lexicographic order.
///
/// For `ORDER BY k1 d1, k2 d2, …, kn dn` and the boundary `(b1…bn)` the rows
/// *after* the boundary are
///
/// ```text
/// (k1 ⋈1 b1)
/// OR (k1 = b1 AND k2 ⋈2 b2)
/// OR (k1 = b1 AND k2 = b2 AND k3 ⋈3 b3)
/// …
/// ```
///
/// where `⋈i` is `>` for an ascending column and `<` for a descending one.
/// The equality terms do not depend on the direction.
///
/// This shape is the **portable** one: it is legal in every dialect, and it
/// is the only one legal in T-SQL, which has no row-value comparison. A
/// dialect that *does* support `(a, b) > (x, y)` may render that instead —
/// the choice belongs to the driver, because "does this dialect have row
/// values" is dialect knowledge. This helper is offered so a driver can be
/// written in one line.
///
/// The condition is `None` when it cannot be safe:
///
/// * the order is empty, or the boundary does not have one value per column;
/// * a boundary value is `NULL` — `NULL` does not take part in comparisons,
///   so a condition built from it would silently drop every row.
///
/// Identifiers come from [`DatabaseDriver::quote_ident`] and values from
/// [`DatabaseDriver::format_sql_literal`], so the rendered condition is the
/// dialect's own SQL and never a host-built approximation of it.
pub fn lexicographic_seek_predicate(
    driver: &dyn DatabaseDriver,
    order_columns: &[(String, bool)],
    boundary: &[(String, Value)],
) -> Option<String> {
    if order_columns.is_empty() || order_columns.len() != boundary.len() {
        return None;
    }
    if boundary
        .iter()
        .any(|(_, value)| matches!(value, Value::Null))
    {
        return None;
    }

    let literal = |value: &Value| driver.format_sql_literal(&Some(value.clone()));
    let mut disjuncts = Vec::with_capacity(order_columns.len());
    for index in 0..order_columns.len() {
        let mut conjuncts = Vec::with_capacity(index + 1);
        for previous in 0..index {
            conjuncts.push(format!(
                "{} = {}",
                driver.quote_ident(&order_columns[previous].0),
                literal(&boundary[previous].1),
            ));
        }
        let comparison = if order_columns[index].1 { "<" } else { ">" };
        conjuncts.push(format!(
            "{} {} {}",
            driver.quote_ident(&order_columns[index].0),
            comparison,
            literal(&boundary[index].1),
        ));
        disjuncts.push(format!("({})", conjuncts.join(" AND ")));
    }

    if disjuncts.len() == 1 {
        Some(disjuncts.remove(0))
    } else {
        Some(format!("({})", disjuncts.join(" OR ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_driver::{MockDriver, MockDriverOptions};
    use std::sync::Arc;

    /// A driver whose quoting and literal rendering are the plain ANSI ones,
    /// so the expected SQL in these cases is readable.
    fn driver() -> Arc<dyn DatabaseDriver> {
        MockDriver::new("postgres", MockDriverOptions::default()) as Arc<dyn DatabaseDriver>
    }

    #[test]
    fn lexicographic_seek_predicate_single_ascending() {
        let predicate = lexicographic_seek_predicate(
            driver().as_ref(),
            &[("id".to_string(), false)],
            &[("id".to_string(), Value::Integer(42))],
        );
        assert_eq!(predicate.as_deref(), Some("(\"id\" > 42)"));
    }

    #[test]
    fn lexicographic_seek_predicate_single_descending() {
        let predicate = lexicographic_seek_predicate(
            driver().as_ref(),
            &[("id".to_string(), true)],
            &[("id".to_string(), Value::Integer(42))],
        );
        assert_eq!(predicate.as_deref(), Some("(\"id\" < 42)"));
    }

    #[test]
    fn lexicographic_seek_predicate_composite_uniform() {
        let predicate = lexicographic_seek_predicate(
            driver().as_ref(),
            &[("a".to_string(), false), ("b".to_string(), false)],
            &[
                ("a".to_string(), Value::Integer(1)),
                ("b".to_string(), Value::Integer(2)),
            ],
        );
        assert_eq!(
            predicate.as_deref(),
            Some("((\"a\" > 1) OR (\"a\" = 1 AND \"b\" > 2))")
        );
    }

    #[test]
    fn lexicographic_seek_predicate_mixed_directions() {
        let predicate = lexicographic_seek_predicate(
            driver().as_ref(),
            &[("a".to_string(), false), ("b".to_string(), true)],
            &[
                ("a".to_string(), Value::Integer(1)),
                ("b".to_string(), Value::Integer(2)),
            ],
        );
        assert_eq!(
            predicate.as_deref(),
            Some("((\"a\" > 1) OR (\"a\" = 1 AND \"b\" < 2))")
        );
    }

    #[test]
    fn lexicographic_seek_predicate_rejects_null_and_length_mismatch() {
        let driver = driver();
        assert!(
            lexicographic_seek_predicate(driver.as_ref(), &[], &[],).is_none(),
            "an empty order has no boundary to seek from"
        );
        assert!(
            lexicographic_seek_predicate(
                driver.as_ref(),
                &[("a".to_string(), false), ("b".to_string(), false)],
                &[("a".to_string(), Value::Integer(1))],
            )
            .is_none(),
            "a boundary that does not match the order cannot be located"
        );
        assert!(
            lexicographic_seek_predicate(
                driver.as_ref(),
                &[("a".to_string(), false)],
                &[("a".to_string(), Value::Null)],
            )
            .is_none(),
            "NULL does not take part in comparisons, so it cannot be a boundary"
        );
    }

    #[test]
    fn cursor_value_round_trip_is_lossless_for_array_shaped_values() {
        // The untagged `Value` reads `[1, 2, 3]` as `Bytes` and a JSON array
        // as `Json` — whichever variant happens to be tried first. The tagged
        // `CursorValue` keeps them apart across the wire.
        let json = CursorValue::Json(serde_json::json!([1, 2, 3]));
        let bytes = CursorValue::Bytes(vec![1, 2, 3]);
        assert_ne!(json, bytes);

        let json_wire = serde_json::to_string(&json).expect("cursor value serializes");
        let bytes_wire = serde_json::to_string(&bytes).expect("cursor value serializes");
        assert_ne!(json_wire, bytes_wire);

        let json_back: CursorValue =
            serde_json::from_str(&json_wire).expect("cursor value deserializes");
        let bytes_back: CursorValue =
            serde_json::from_str(&bytes_wire).expect("cursor value deserializes");
        assert_eq!(json_back, json);
        assert_eq!(bytes_back, bytes);
    }

    #[test]
    fn cursor_value_maps_to_and_from_the_driver_value() {
        // `Value` has no `PartialEq` of its own, so the round trip is checked
        // on the wire form — which is the form that actually travels.
        let value = Value::Json(serde_json::json!({"a": 1}));
        let wire = serde_json::to_string(&value).expect("value serializes");
        let cursor = CursorValue::from_value(&value);
        let back = cursor.to_value();
        assert_eq!(
            serde_json::to_string(&back).expect("value serializes"),
            wire,
            "the boundary value must survive the mapping unchanged"
        );
        assert_eq!(
            CursorValue::from_value(&Value::Null),
            CursorValue::Null,
            "a SQL NULL is a value on the wire, not an absence"
        );
    }

    /// The compatibility gate: a driver that says nothing about keyset
    /// paging must read exactly as it did before the contract existed —
    /// no capability, and no condition.
    #[test]
    fn default_keyset_capability_is_false_and_predicate_is_none() {
        let driver = driver();
        assert!(
            !driver.supports_keyset_pagination(),
            "the capability must be off by default: a driver that did not opt in must keep reading pages by OFFSET"
        );
        assert!(
            driver
                .keyset_predicate_sql(
                    &[("id".to_string(), false)],
                    &[("id".to_string(), Value::Integer(42))],
                )
                .is_none(),
            "a driver that did not opt in must never be asked for a condition"
        );
    }
}
