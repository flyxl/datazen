//! The table-read and key-value shapes the aggregate declares — ported from
//! React `packages/frontend/src/types/index.ts` (tasklist T211).
//!
//! Four of this file's own interface declarations live here:
//!
//! * [`SortCondition`] — one `column`/`direction` order leg (the TS file
//!   declares it itself; backend-client's `types/tableData.ts` declares a
//!   `SortCondition` of its own for the read request — same name, different
//!   domain, whose port lands with that module; this one is the aggregate's
//!   own, exactly the split React has);
//! * [`TableDataResult`] — the raw backend response of a table read: rows
//!   are 2D arrays, with the two-state row-count (`totalRows` +
//!   `totalRowsKind`) and the paging decision (`paging`);
//! * [`KeyEntry`] / [`KeyScanResult`] / [`KeyDetail`] — the key-value
//!   (Redis) browser's scan/detail shapes.
//!
//! Every leg type is a re-export of an existing owner: `ColumnSchema` and
//! `Value` from `connection_types` (T128), `PagePagingInfo` from
//! `keyset_types` (T234), `CountStrategy` from
//! [`crate::types::extension_points`] (this port's own new module).
//!
//! Pure contracts — no IO, no state — so the module compiles for wasm32
//! exactly like its `chart_types` sibling.

use serde::{Deserialize, Serialize};

use crate::connection_types::{ColumnSchema, Value};
use crate::keyset_types::PagePagingInfo;
use crate::types::extension_points::CountStrategy;

/// React `SortCondition` — one column's sort leg: the column and whether
/// it descends.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SortCondition {
    pub column: String,
    pub descending: bool,
}

/// React `TableDataResult` — the raw backend response of a table read.
/// Rows are 2D arrays.
///
/// `totalRows` is `?: number | null` (null on the wire when the backend
/// could not count — no skip, the key stays present) and
/// `totalRowsKind` is `?: CountStrategy | null` the same way: a missing key
/// means "this response carries no fresh row count" — the caller asked to
/// reuse the one it had, or the backend is older than the field — and that
/// is not the same as `'unsupported'`, which says the count was attempted
/// and this database cannot provide one. `paging` is `?: PagePagingInfo |
/// null`: a missing key (or a `null` value) says the page was read by
/// offset, which is what every read did before the field existed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableDataResult {
    pub columns: Vec<ColumnSchema>,
    /// Each cell a [`Value`] or `null`.
    pub rows: Vec<Vec<Option<Value>>>,
    /// The row count, or `null` when the backend could not count.
    pub total_rows: Option<i64>,
    /// How much `totalRows` may be trusted (see the struct docs' two-state
    /// story). Null on the wire when set to `None`; absent also accepted.
    pub total_rows_kind: Option<CountStrategy>,
    pub page: u32,
    pub page_size: u32,
    /// How the page was paged (see the struct docs' offset-default story).
    /// Null on the wire when set to `None`; absent also accepted.
    pub paging: Option<PagePagingInfo>,
}

/// React `KeyEntry` — one Redis key as the key-value browser's scan lists
/// it.
///
/// Widths per the family's number law: `ttl` is seconds, **signed**
/// (`i64`: Redis answers −1 for no-expiry and −2 for a missing key), and
/// `size` is bytes (`u64`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyEntry {
    pub key: String,
    pub key_type: String,
    /// Seconds left before expiry (−1 = no expiry, the driver's own
    /// convention, kept as the wire number).
    pub ttl: i64,
    /// Stored payload size in bytes.
    pub size: u64,
    /// A short text preview of the value.
    pub preview: String,
}

/// React `KeyScanResult` — one scan cursor's page of keys plus the
/// database's total key count.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyScanResult {
    /// The scan cursor to feed the next page (`0` = end of iteration, the
    /// driver convention).
    pub cursor: u64,
    pub keys: Vec<KeyEntry>,
    /// Total keys in the database (`dbsize`).
    pub db_size: u64,
}

/// React `KeyDetail` — one key's full payload.
///
/// `value` is `unknown` → opaque [`Value`] (the crate's `unknown` rule).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyDetail {
    pub key: String,
    pub key_type: String,
    /// Seconds left before expiry (−1 = no expiry).
    pub ttl: i64,
    /// The decoded payload, opaque.
    pub value: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire)
            .unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    fn encode<T: serde::Serialize>(value: &T) -> serde_json::Value {
        serde_json::to_value(value)
            .unwrap_or_else(|error| panic!("wire encode failed: {error}"))
    }

    #[test]
    fn a_table_read_pins_its_two_state_row_count_and_paging() {
        let result = TableDataResult {
            columns: vec![ColumnSchema {
                name: "id".into(),
                data_type: "integer".into(),
                nullable: false,
                default_value: None,
                is_primary_key: None,
                is_auto_increment: None,
                comment: None,
            }],
            rows: vec![vec![Some(serde_json::json!(1)), None]],
            total_rows: None,
            total_rows_kind: None,
            page: 0,
            page_size: 50,
            paging: None,
        };
        let wire = encode(&result);
        // `?: T | null` legs keep explicit nulls — the response carries the
        // keys even when it carries no fresh count.
        assert_eq!(wire["totalRows"], serde_json::Value::Null);
        assert_eq!(wire["totalRowsKind"], serde_json::Value::Null);
        assert_eq!(wire["paging"], serde_json::Value::Null);
        let back: TableDataResult = decode(wire);
        assert_eq!(back, result);

        // The fresh-count spelling: `estimated`, with a paging decision.
        let fresh = TableDataResult {
            total_rows: Some(1_234_567),
            total_rows_kind: Some(CountStrategy::Estimated),
            paging: None,
            ..result
        };
        let wire = encode(&fresh);
        assert_eq!(wire["totalRows"], 1_234_567);
        assert_eq!(wire["totalRowsKind"], "estimated");
        // The offset-default story: an OLD backend payload (no keys at
        // all) still decodes — the caller keeps its own values.
        let legacy: TableDataResult = decode(serde_json::json!({
            "columns": [],
            "rows": [],
            "page": 3,
            "pageSize": 25
        }));
        assert_eq!(legacy.total_rows, None);
        assert_eq!(legacy.total_rows_kind, None);
        assert_eq!(legacy.paging, None);
        assert_eq!(legacy.page, 3);
    }

    #[test]
    fn a_sort_condition_pins_its_two_fields() {
        let condition = SortCondition { column: "id".into(), descending: true };
        assert_eq!(
            encode(&condition),
            serde_json::json!({ "column": "id", "descending": true })
        );
        let back: SortCondition = decode(encode(&condition));
        assert_eq!(back, condition);
    }

    #[test]
    fn the_key_value_shapes_round_trip() {
        let scan = KeyScanResult {
            cursor: 42,
            keys: vec![KeyEntry {
                key: "user:1".into(),
                key_type: "hash".into(),
                ttl: -1,
                size: 128,
                preview: "{name: \"…\"}".into(),
            }],
            db_size: 10_000,
        };
        let wire = encode(&scan);
        assert_eq!(wire["cursor"], 42);
        assert_eq!(wire["keys"][0]["keyType"], "hash");
        assert_eq!(wire["keys"][0]["ttl"], -1);
        assert_eq!(wire["dbSize"], 10_000);
        let back: KeyScanResult = decode(wire);
        assert_eq!(back, scan);

        let detail = KeyDetail {
            key: "user:1".into(),
            key_type: "hash".into(),
            ttl: 300,
            value: serde_json::json!({ "name": "Ada", "roles": ["admin"] }),
        };
        let wire = encode(&detail);
        assert_eq!(wire["value"]["roles"][0], "admin");
        let back: KeyDetail = decode(wire);
        assert_eq!(back, detail);
    }
}
