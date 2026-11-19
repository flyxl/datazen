//! Keyset (seek) paging vocabulary — ported from React
//! `packages/frontend/src/types/keyset.ts` (tasklist T234, 82 lines).
//!
//! The host's paging report in one small file, the front end's side of
//! the seek contract: how the page was read ([`PagingMode`]), why it
//! was not read by seek ([`KeysetUnavailableReason`]), the tagged
//! boundary values a seek compares against ([`CursorValue`]), the
//! column order of the page ([`OrderColumn`]), and the report itself
//! ([`PagePagingInfo`]). The front end believes the report — it does
//! not guess which strategy is in force. Pure contracts — no IO, no
//! state — so the module compiles for wasm32 exactly like its
//! `chart_types`/`filter_types` siblings, and the aggregate
//! `types/index.ts` re-exports every name the React file declares, so
//! the crate root carries the same names (the mounting rule
//! `chart_types` set). Every doc comment below is the React file's,
//! preserved.
//!
//! ## Dependency inventory
//!
//! None — the React file imports nothing. (The `json` leg's `unknown`
//! payload rides the crate's `unknown` rule — opaque
//! `serde_json::Value`, nulls included — which is a mapping, not an
//! import the React file makes.)
//!
//! ## Mirror rules applied
//!
//! - A React string-literal union becomes a Rust enum whose `as_str`
//!   pins the exact React literal and whose serde representation **is**
//!   that literal: `#[serde(rename_all = "camelCase")]` spells every
//!   variant as its TS spelling (`offset`/`keyset`;
//!   `driverUnsupported` … `notAdjacent`). The literal tables are
//!   pinned by tests, one row per variant.
//! - React's discriminated union (the `{ kind: … }` legs) becomes a
//!   serde-tagged enum: `#[serde(tag = "kind", rename_all =
//!   "camelCase")]` with struct legs keeps `{ kind: 'null' }` a unit
//!   variant and every other leg exactly `{ kind, value }` — the move
//!   `ai_types::chat`'s `AiChatAttribution` made. The tag is this
//!   file's whole point: the payload types follow the host's `Value`
//!   variants one-for-one (`Integer(i64)`, `Bytes(Vec<u8>)`,
//!   `Timestamp(String)`, `Json(serde_json::Value)` — driver-api's own
//!   payload types), so a boundary survives the "Rust → front end →
//!   Rust" trip with its variant *and* its literal intact. TS `number`
//!   payloads: the `integer` leg carries the JSON integer the host's
//!   `Value::Integer(i64)` emits, so it is `i64` — the host's full
//!   range, and a double would re-emit `3` as `3.0` and flip the
//!   literal; the `bytes` leg's `number[]` cells are the host's u8
//!   width.
//! - `interface` → struct; optional (`?:`) fields (`nextAfterKey`,
//!   `unavailableReason`) become `Option` with
//!   `#[serde(default, skip_serializing_if = "Option::is_none")]`, so
//!   an absent key round-trips as an absent key. No
//!   required-but-nullable field lives in this file, so no field keeps
//!   the explicit-`null` spelling.
//! - No `DEFAULT_*` constant lives in the React file, so there is no
//!   `Default` to port.
//! - React has no dedicated test for this file
//!   (`lib/__tests__/keysetPaging.test.ts` tests `lib/keysetPaging.ts`,
//!   another task), so the pins below are the port's own: every
//!   literal table, every Option's two states, the payload widths
//!   the legs claim — and, from the T234 audit round, the three
//!   unions' closures, the crate-root facade, the required-vs-
//!   optional split, and the float leg's f64 width, each pinned after
//!   its surviving mutant.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How a page was read.
///
/// The mirror of the host's `PagingMode`: by offset (the page starts
/// `page × pageSize` rows in) or by seek (the page starts after a value
/// boundary that was read from a real row).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PagingMode {
    Offset,
    Keyset,
}

impl PagingMode {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Offset => "offset",
            Self::Keyset => "keyset",
        }
    }
}

/// Why a page could not be read by seek.
///
/// The mirror of the host's `KeysetUnavailableReason`. Every one of them
/// is a **degradation**, not an error: the page is read by offset and
/// the reason says why.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeysetUnavailableReason {
    DriverUnsupported,
    NoStableOrder,
    NullableKeyColumn,
    NullBoundary,
    PredicateUnavailable,
    NoPaginationClause,
    StaleCursor,
    NotAdjacent,
}

impl KeysetUnavailableReason {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DriverUnsupported => "driverUnsupported",
            Self::NoStableOrder => "noStableOrder",
            Self::NullableKeyColumn => "nullableKeyColumn",
            Self::NullBoundary => "nullBoundary",
            Self::PredicateUnavailable => "predicateUnavailable",
            Self::NoPaginationClause => "noPaginationClause",
            Self::StaleCursor => "staleCursor",
            Self::NotAdjacent => "notAdjacent",
        }
    }
}

/// One boundary value, **with its tag**.
///
/// The host's `Value` is untagged, so `[1, 2, 3]` reads as `Bytes` *or*
/// as `Json`. A boundary travels "Rust → front end → Rust", and
/// untagged it would change its variant on the way back — the seek
/// would then compare against the wrong literal, with no error
/// anywhere. The tag is what keeps the two apart.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CursorValue {
    /// `{ kind: 'null' }` — the boundary is SQL NULL; no value key rides
    /// the wire.
    Null,
    /// `{ kind: 'bool'; value: boolean }`.
    Bool {
        value: bool,
    },
    /// `{ kind: 'integer'; value: number }` — the JSON integer the
    /// host's `Value::Integer(i64)` emits; `i64` keeps the host's range
    /// and the literal stable on the trip back.
    Integer {
        value: i64,
    },
    /// `{ kind: 'float'; value: number }`.
    Float {
        value: f64,
    },
    /// `{ kind: 'string'; value: string }`.
    String {
        value: String,
    },
    /// `{ kind: 'bytes'; value: number[] }` — the host's
    /// `Value::Bytes(Vec<u8>)`; u8 is the cell width.
    Bytes {
        value: Vec<u8>,
    },
    /// `{ kind: 'timestamp'; value: string }` — the host's
    /// `Value::Timestamp(String)` spelling, verbatim.
    Timestamp {
        value: String,
    },
    /// `{ kind: 'json'; value: unknown }` — opaque JSON, nulls
    /// included.
    Json {
        value: Value,
    },
}

/// React `OrderColumn` — one ordering column and its direction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderColumn {
    pub column: String,
    pub descending: bool,
}

/// React `PagePagingInfo` — what the host did about the paging of one
/// page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PagePagingInfo {
    /// The strategy that was used. **The front end believes this
    /// field**, not its own guess about which strategy is in force.
    pub mode: PagingMode,
    /// The real column order of the page, key columns included.
    pub order_columns: Vec<OrderColumn>,
    /// The key values of the last row, to be sent back for the next
    /// page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_after_key: Option<Vec<CursorValue>>,
    /// Set when the page was **not** read by seek: why not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<KeysetUnavailableReason>,
    /// Rows were deleted while the user was paging.
    pub drift_suspected: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    // No React test suite backs this file — these tests are the port's
    // own pins: every literal table, the tagged union's exact legs, the
    // payload widths, and the Option two-states the mirror rules
    // promise. The audit round's own additions close the gaps its
    // surviving mutants found (the closure/root/required/float-width
    // tests below); two of its mutants died as documented equivalents
    // instead: dropping `default` from an `Option` field changes nothing
    // (serde decodes a missing field of `Option` type as `None`, pinned
    // by the last block of the required-fields test), and dropping
    // `OrderColumn`'s `rename_all` changes nothing (both fields are
    // single words whose camelCase spelling is the field name).

    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire)
            .unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    #[test]
    fn paging_modes_keep_the_react_literals() {
        let table = [
            (PagingMode::Offset, "offset"),
            (PagingMode::Keyset, "keyset"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: PagingMode = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(serde_json::to_value(parsed).ok(), Some(serde_json::json!(literal)));
        }
    }

    #[test]
    fn keyset_unavailable_reasons_keep_the_react_literals() {
        let table = [
            (KeysetUnavailableReason::DriverUnsupported, "driverUnsupported"),
            (KeysetUnavailableReason::NoStableOrder, "noStableOrder"),
            (KeysetUnavailableReason::NullableKeyColumn, "nullableKeyColumn"),
            (KeysetUnavailableReason::NullBoundary, "nullBoundary"),
            (KeysetUnavailableReason::PredicateUnavailable, "predicateUnavailable"),
            (KeysetUnavailableReason::NoPaginationClause, "noPaginationClause"),
            (KeysetUnavailableReason::StaleCursor, "staleCursor"),
            (KeysetUnavailableReason::NotAdjacent, "notAdjacent"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: KeysetUnavailableReason = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(serde_json::to_value(parsed).ok(), Some(serde_json::json!(literal)));
        }
    }

    #[test]
    fn cursor_values_round_trip_every_tagged_leg() {
        // Every leg of the discriminated union, in declaration order,
        // wire for wire — including the valueless null leg's single key.
        let table = [
            (
                CursorValue::Null,
                serde_json::json!({ "kind": "null" }),
            ),
            (
                CursorValue::Bool { value: true },
                serde_json::json!({ "kind": "bool", "value": true }),
            ),
            (
                CursorValue::Integer { value: 42 },
                serde_json::json!({ "kind": "integer", "value": 42 }),
            ),
            (
                CursorValue::Float { value: 0.5 },
                serde_json::json!({ "kind": "float", "value": 0.5 }),
            ),
            (
                CursorValue::String { value: String::from("m") },
                serde_json::json!({ "kind": "string", "value": "m" }),
            ),
            (
                CursorValue::Bytes { value: vec![1, 2, 3] },
                serde_json::json!({ "kind": "bytes", "value": [1, 2, 3] }),
            ),
            (
                CursorValue::Timestamp { value: String::from("2026-01-07T10:20:30Z") },
                serde_json::json!({ "kind": "timestamp", "value": "2026-01-07T10:20:30Z" }),
            ),
            (
                CursorValue::Json { value: serde_json::json!({ "nested": [1, null] }) },
                serde_json::json!({ "kind": "json", "value": { "nested": [1, null] } }),
            ),
        ];
        for (value, wire) in table {
            assert_eq!(serde_json::to_value(&value).ok(), Some(wire.clone()));
            let back: CursorValue = decode(wire);
            assert_eq!(back, value);
        }

        // The file's motivating pair: `[1, 2, 3]` reads as bytes *or*
        // as json — the tag, not the payload, is what keeps the two
        // apart, and neither leg flips on the way back.
        let bytes: CursorValue = decode(serde_json::json!({ "kind": "bytes", "value": [1, 2, 3] }));
        assert_eq!(bytes, CursorValue::Bytes { value: vec![1, 2, 3] });
        let json: CursorValue = decode(serde_json::json!({ "kind": "json", "value": [1, 2, 3] }));
        assert_eq!(
            json,
            CursorValue::Json { value: serde_json::json!([1, 2, 3]) }
        );
        assert_ne!(bytes, json);
    }

    #[test]
    fn cursor_values_pin_the_payload_widths() {
        // The integer leg carries the host's full i64 range, both ends
        // exact on the way back.
        for edge in [i64::MIN, i64::MAX, 0, -1] {
            let value = CursorValue::Integer { value: edge };
            let wire = serde_json::json!({ "kind": "integer", "value": edge });
            assert_eq!(serde_json::to_value(&value).ok(), Some(wire.clone()));
            let back: CursorValue = decode(wire);
            assert_eq!(back, value);
        }
        // Just past i64's top (2^63) is not an integer boundary.
        let rejected = serde_json::from_value::<CursorValue>(serde_json::json!({
            "kind": "integer",
            "value": 9223372036854775808u64
        }));
        assert!(rejected.is_err());

        // The bytes leg's cell width is u8: empty and [0, 255] ride,
        // 256 does not.
        for wire in [
            serde_json::json!({ "kind": "bytes", "value": [] }),
            serde_json::json!({ "kind": "bytes", "value": [0, 255] }),
        ] {
            let back: CursorValue = decode(wire.clone());
            assert_eq!(serde_json::to_value(&back).ok(), Some(wire));
        }
        let rejected = serde_json::from_value::<CursorValue>(serde_json::json!({
            "kind": "bytes",
            "value": [256]
        }));
        assert!(rejected.is_err());

        // Literal stability, both directions of the integer/float
        // split: an integer stays the integer `3` (never `3.0`), a
        // float stays the float `2.0` (never the integer `2` — the two
        // are different serde_json numbers).
        assert_eq!(
            serde_json::to_value(&CursorValue::Integer { value: 3 }).ok(),
            Some(serde_json::json!({ "kind": "integer", "value": 3 }))
        );
        assert_ne!(
            serde_json::to_value(&CursorValue::Integer { value: 3 }).ok(),
            Some(serde_json::json!({ "kind": "integer", "value": 3.0 }))
        );
        assert_eq!(
            serde_json::to_value(&CursorValue::Float { value: 2.0 }).ok(),
            Some(serde_json::json!({ "kind": "float", "value": 2.0 }))
        );
        assert_ne!(
            serde_json::to_value(&CursorValue::Float { value: 2.0 }).ok(),
            Some(serde_json::json!({ "kind": "float", "value": 2 }))
        );
    }

    #[test]
    fn order_columns_round_trip_both_directions() {
        // One ascending, one descending — the only state the pair of
        // fields has.
        let wire = serde_json::json!([
            { "column": "id", "descending": false },
            { "column": "created_at", "descending": true }
        ]);
        let columns: Vec<OrderColumn> = decode(wire.clone());
        assert_eq!(columns[0].column, "id");
        assert!(!columns[0].descending);
        assert_eq!(columns[1].column, "created_at");
        assert!(columns[1].descending);
        assert_eq!(serde_json::to_value(&columns).ok(), Some(wire));
    }

    #[test]
    fn page_paging_info_round_trips_its_option_two_states() {
        // A page read by seek: the boundary to send back for the next
        // page is present (a compound key, null leg included), no
        // degradation to report, no drift.
        let keyset_wire = serde_json::json!({
            "mode": "keyset",
            "orderColumns": [
                { "column": "id", "descending": false },
                { "column": "created_at", "descending": true }
            ],
            "nextAfterKey": [
                { "kind": "integer", "value": 7 },
                { "kind": "null" }
            ],
            "driftSuspected": false
        });
        let keyset: PagePagingInfo = decode(keyset_wire.clone());
        assert_eq!(keyset.mode, PagingMode::Keyset);
        assert_eq!(keyset.order_columns.len(), 2);
        assert_eq!(
            keyset.next_after_key,
            Some(vec![
                CursorValue::Integer { value: 7 },
                CursorValue::Null,
            ])
        );
        assert_eq!(keyset.unavailable_reason, None);
        assert!(!keyset.drift_suspected);
        assert_eq!(serde_json::to_value(&keyset).ok(), Some(keyset_wire.clone()));
        let keyset_back: PagePagingInfo = decode(keyset_wire);
        assert_eq!(keyset_back, keyset);

        // A page read by offset: the reason why seek was unavailable is
        // present, no boundary to send back, drift suspected.
        let offset_wire = serde_json::json!({
            "mode": "offset",
            "orderColumns": [],
            "unavailableReason": "driverUnsupported",
            "driftSuspected": true
        });
        let offset: PagePagingInfo = decode(offset_wire.clone());
        assert_eq!(offset.mode, PagingMode::Offset);
        assert!(offset.order_columns.is_empty());
        assert_eq!(offset.next_after_key, None);
        assert_eq!(
            offset.unavailable_reason,
            Some(KeysetUnavailableReason::DriverUnsupported)
        );
        assert!(offset.drift_suspected);
        assert_eq!(serde_json::to_value(&offset).ok(), Some(offset_wire.clone()));
        let offset_back: PagePagingInfo = decode(offset_wire);
        assert_eq!(offset_back, offset);
    }

    // ---- T234 audit pins ------------------------------------------------
    //
    // Everything below was added by the audit round: each test closes a
    // gap a surviving mutant walked through (the mutations never landed
    // — the file was restored after each red/green verdict).

    #[test]
    fn cursor_values_pin_the_float_legs_f64_width() {
        // `0.1` is exact in neither f32 nor f64, but the f32 round trip
        // visibly widens it (0.100000001490116…), so a leg narrowed to
        // `f32` fails both comparisons below while the f64 leg rides
        // clean. The surviving `f64 → f32` mutant died here — the
        // existing width pins only exercised 0.5/2.0, both exact in f32.
        let value = CursorValue::Float { value: 0.1 };
        let wire = serde_json::json!({ "kind": "float", "value": 0.1 });
        assert_eq!(serde_json::to_value(&value).ok(), Some(wire.clone()));
        let back: CursorValue = decode(wire.clone());
        assert_eq!(serde_json::to_value(&back).ok(), Some(wire));
    }

    #[test]
    fn the_three_unions_are_closed_exactly_as_react_declares_them() {
        // Compile-time closure witnesses (the `filter_types` pin form):
        // each match carries one arm per declared variant and no
        // wildcard, so widening any of the three unions with a
        // React-unknown member stops this file from compiling instead of
        // silently widening a vocabulary the tables above would happily
        // spell too. The surviving widening mutants (a third
        // `PagingMode`, a ninth reason, a `uuid` leg) died here.
        fn mode_index(mode: PagingMode) -> usize {
            match mode {
                PagingMode::Offset => 0,
                PagingMode::Keyset => 1,
            }
        }
        fn reason_index(reason: KeysetUnavailableReason) -> usize {
            match reason {
                KeysetUnavailableReason::DriverUnsupported => 0,
                KeysetUnavailableReason::NoStableOrder => 1,
                KeysetUnavailableReason::NullableKeyColumn => 2,
                KeysetUnavailableReason::NullBoundary => 3,
                KeysetUnavailableReason::PredicateUnavailable => 4,
                KeysetUnavailableReason::NoPaginationClause => 5,
                KeysetUnavailableReason::StaleCursor => 6,
                KeysetUnavailableReason::NotAdjacent => 7,
            }
        }
        // The CursorValue witness doubles as the tag table: the arm's
        // literal *is* the wire tag of the leg.
        fn cursor_tag(cursor: &CursorValue) -> &'static str {
            match cursor {
                CursorValue::Null => "null",
                CursorValue::Bool { .. } => "bool",
                CursorValue::Integer { .. } => "integer",
                CursorValue::Float { .. } => "float",
                CursorValue::String { .. } => "string",
                CursorValue::Bytes { .. } => "bytes",
                CursorValue::Timestamp { .. } => "timestamp",
                CursorValue::Json { .. } => "json",
            }
        }
        // The witnesses agree index for index with the wire order the
        // coverage tables use…
        for (index, mode) in [PagingMode::Offset, PagingMode::Keyset].into_iter().enumerate() {
            assert_eq!(mode_index(mode), index);
        }
        for (index, reason) in [
            KeysetUnavailableReason::DriverUnsupported,
            KeysetUnavailableReason::NoStableOrder,
            KeysetUnavailableReason::NullableKeyColumn,
            KeysetUnavailableReason::NullBoundary,
            KeysetUnavailableReason::PredicateUnavailable,
            KeysetUnavailableReason::NoPaginationClause,
            KeysetUnavailableReason::StaleCursor,
            KeysetUnavailableReason::NotAdjacent,
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(reason_index(reason), index);
        }
        // …and the cursor witness's tag is the tag on the wire, every leg.
        for (value, tag) in [
            (CursorValue::Null, "null"),
            (CursorValue::Bool { value: false }, "bool"),
            (CursorValue::Integer { value: 0 }, "integer"),
            (CursorValue::Float { value: 0.0 }, "float"),
            (CursorValue::String { value: String::new() }, "string"),
            (CursorValue::Bytes { value: Vec::new() }, "bytes"),
            (CursorValue::Timestamp { value: String::new() }, "timestamp"),
            (CursorValue::Json { value: serde_json::json!(null) }, "json"),
        ] {
            assert_eq!(cursor_tag(&value), tag);
            assert_eq!(
                serde_json::to_value(&value)
                    .ok()
                    .and_then(|wire| wire.get("kind").cloned()),
                Some(serde_json::json!(tag))
            );
        }
        // The runtime half of the closure: the spellings a widened
        // vocabulary would accept reject. `cursor`/`hostGone`/`uuid`
        // are the audit's surviving widening mutants' own literals;
        // the rest are the near-miss spellings each union's camelCase
        // rule forbids.
        for near_miss in ["seek", "cursor", "Offset", "KEYSET"] {
            assert!(
                serde_json::from_value::<PagingMode>(serde_json::json!(near_miss)).is_err(),
                "near-miss paging literal {near_miss:?} must not parse"
            );
        }
        for near_miss in [
            "hostGone",
            "driver-unsupported",
            "DriverUnsupported",
            "stalecursor",
            "no_stable_order",
        ] {
            assert!(
                serde_json::from_value::<KeysetUnavailableReason>(serde_json::json!(near_miss))
                    .is_err(),
                "near-miss reason literal {near_miss:?} must not parse"
            );
        }
        for near_miss in ["uuid", "int", "Null", "Integer", "boolean", "number"] {
            assert!(
                serde_json::from_value::<CursorValue>(serde_json::json!({ "kind": near_miss }))
                    .is_err(),
                "near-miss cursor tag {near_miss:?} must not parse"
            );
        }
        // The tag key itself: a renamed tag key must not decode either
        // (a `tag = "type"` mutant would take `{"type": "null"}`).
        let renamed_tag_key = serde_json::from_value::<CursorValue>(serde_json::json!({
            "type": "null"
        }));
        assert!(renamed_tag_key.is_err());
    }

    #[test]
    fn the_keyset_vocabulary_is_reexported_at_the_crate_root() {
        // `crate::` paths only (the `chart_types`/`filter_types` pin
        // form): a dropped root re-export stops this file from compiling
        // instead of silently shrinking the facade `types/index.ts`
        // promises — the surviving root-shrink mutant (which dropped
        // `OrderColumn`) died here. Every test above speaks `super::`,
        // so nothing else would have noticed.
        fn resolves<T>() {}
        resolves::<crate::PagingMode>();
        resolves::<crate::KeysetUnavailableReason>();
        resolves::<crate::CursorValue>();
        resolves::<crate::OrderColumn>();
        resolves::<crate::PagePagingInfo>();
        // One live decode through the root paths keeps the pin honest.
        let page: crate::PagePagingInfo = decode(serde_json::json!({
            "mode": "keyset",
            "orderColumns": [{ "column": "id", "descending": true }],
            "nextAfterKey": [{ "kind": "null" }],
            "driftSuspected": false
        }));
        assert_eq!(page.mode, crate::PagingMode::Keyset);
        assert_eq!(page.order_columns[0].column, "id");
        assert!(page.order_columns[0].descending);
        assert_eq!(page.next_after_key, Some(vec![crate::CursorValue::Null]));
        assert_eq!(page.unavailable_reason, None);
    }

    #[test]
    fn required_fields_stay_required_and_optional_keys_stay_optional() {
        // The mirror rule the `?:` split encodes: `mode`,
        // `orderColumns`, and `driftSuspected` have no default — a wire
        // without any one of them is not a page report. Relaxing any of
        // them to `#[serde(default)]` (the surviving relaxation mutants)
        // lets these minimal wires parse.
        for wire in [
            serde_json::json!({ "orderColumns": [], "driftSuspected": false }),
            serde_json::json!({ "mode": "offset", "driftSuspected": false }),
            serde_json::json!({ "mode": "offset", "orderColumns": [] }),
        ] {
            assert!(
                serde_json::from_value::<PagePagingInfo>(wire).is_err(),
                "a PagePagingInfo without a required field must not decode"
            );
        }
        // The same rule for `OrderColumn`'s two fields.
        for wire in [
            serde_json::json!({ "column": "id" }),
            serde_json::json!({ "descending": true }),
        ] {
            assert!(
                serde_json::from_value::<OrderColumn>(wire).is_err(),
                "an OrderColumn without both fields must not decode"
            );
        }
        // The `?:` half of the rule: the two optional keys absent decode
        // as `None` (serde decodes a missing field of `Option` type as
        // `None` — which is why `#[serde(default)]` on an `Option` field
        // is belt-and-braces, not behavior).
        let page: PagePagingInfo = decode(serde_json::json!({
            "mode": "offset",
            "orderColumns": [],
            "driftSuspected": false
        }));
        assert_eq!(page.next_after_key, None);
        assert_eq!(page.unavailable_reason, None);
    }
}
