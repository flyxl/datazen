//! Filter vocabulary for the data grid — ported from React
//! `packages/frontend/src/types/filters.ts` (tasklist T188, 71 lines).
//!
//! That file is the frontend's filter contract: the `FilterOperator`
//! vocabulary re-exported from `@datazen/extension-points`, the three
//! operator groups (the ten that shipped, the shared whitelist built on
//! them, the dialect-specific three) and [`FilterCondition`], one row of
//! the data grid's filter list. It lives in its own file because the
//! filter contract grows while `./index` stays the aggregate the rest of
//! the app imports from; the aggregate re-exports everything here, so
//! `import type { FilterCondition } from '../types'` keeps working for
//! every caller — this module is the file's Rust home and the crate root
//! carries the same names (the mounting rule `chart_types` set). Pure
//! contracts — no IO, no state — so the module compiles for wasm32
//! exactly like its `chart_types`/`connection_types` siblings.
//!
//! ## Dependency inventory
//!
//! | React dependency | Rust side found | decision here |
//! |---|---|---|
//! | `FilterOperator` ← `@datazen/extension-points` (a re-export) | `datazen-driver-api::filters::FilterOperator`: the same 22 variants, the same camelCase wire spelling, the same `as_str` table | thin mirror. This crate's `datazen-driver-api` dependency is `cfg(not(target_arch = "wasm32"))`-only (the T128 ruling: the wasm host compiles these contract modules without the driver stack), and this module must compile for wasm32 like its siblings — a `use` would break the wasm half. The mirror keeps driver-api's surface (variant set, order, serde representation, `as_str` literals), so the two enums stay interchangeable on the wire; tests pin all 22 literals so they cannot drift silently. |
//! | `Value` ← `./connection` | `crate::connection_types::Value`, the T128 facade alias of `serde_json::Value` | direct `use` — same crate, wasm-safe, exactly the `./connection` import the React file makes |
//!
//! ## Mirror rules applied
//!
//! - A React string-literal union becomes a Rust enum whose `as_str`
//!   pins the exact React literal and whose serde representation **is**
//!   that literal: `#[serde(rename_all = "camelCase")]` spells all 22
//!   variants as the TS literals (`eq` … `jsonContains`).
//! - `FilterCondition.connector`'s inline `'and' | 'or'` union gets the
//!   same treatment as a named enum ([`FilterConnector`]) — the move
//!   `chart_types` made for `ChartConfig.sortBy`.
//! - `interface` → struct; optional (`?:`) fields (`value`,
//!   `connector`) become `Option` with
//!   `#[serde(default, skip_serializing_if = "Option::is_none")]`, so
//!   an absent key round-trips as an absent key: `undefined` stays
//!   `undefined`, never an explicit `null` (a `null` on the wire would
//!   sit next to the absent key as a second spelling of "unset"; the flip
//!   side is serde's known collapse — a wire `null` decodes as `None` for
//!   an `Option` field, so "unset" crosses the wire one way only, the
//!   absent key).
//! - The three `readonly` arrays port as `&[FilterOperator]` constants
//!   in the file's own order, doc comments included.

use serde::{Deserialize, Serialize};

use crate::connection_types::Value;

/// React `FilterOperator` (`@datazen/extension-points` re-export) — the
/// operator vocabulary of the data grid's filter contract.
///
/// A thin mirror of `datazen-driver-api::filters::FilterOperator` (see
/// the dependency inventory): the first ten variants are the operators
/// that shipped before the extension, the next nine the shared
/// extensions every ANSI-ish dialect can render, the last three the
/// dialect-specific operators only their declaring driver renders. The
/// declaration order is the wire order the operator groups rely on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterOperator {
    // ── Existing ten: names, order and wire spelling never change ──
    Eq,
    Ne,
    Gt,
    Lt,
    Gte,
    Lte,
    Like,
    In,
    IsNull,
    IsNotNull,
    // ── Shared extensions (nine): rendered by the host for every driver ──
    NotIn,
    Between,
    NotBetween,
    Contains,
    NotContains,
    StartsWith,
    EndsWith,
    IsEmpty,
    IsNotEmpty,
    // ── Dialect-specific (three): the declaring driver renders them ──
    Regex,
    NotRegex,
    JsonContains,
}

impl FilterOperator {
    /// The wire spelling.
    ///
    /// It matches the serde representation exactly, so error messages and
    /// test assertions never hand-write the literal.
    pub fn as_str(self) -> &'static str {
        match self {
            FilterOperator::Eq => "eq",
            FilterOperator::Ne => "ne",
            FilterOperator::Gt => "gt",
            FilterOperator::Lt => "lt",
            FilterOperator::Gte => "gte",
            FilterOperator::Lte => "lte",
            FilterOperator::Like => "like",
            FilterOperator::In => "in",
            FilterOperator::IsNull => "isNull",
            FilterOperator::IsNotNull => "isNotNull",
            FilterOperator::NotIn => "notIn",
            FilterOperator::Between => "between",
            FilterOperator::NotBetween => "notBetween",
            FilterOperator::Contains => "contains",
            FilterOperator::NotContains => "notContains",
            FilterOperator::StartsWith => "startsWith",
            FilterOperator::EndsWith => "endsWith",
            FilterOperator::IsEmpty => "isEmpty",
            FilterOperator::IsNotEmpty => "isNotEmpty",
            FilterOperator::Regex => "regex",
            FilterOperator::NotRegex => "notRegex",
            FilterOperator::JsonContains => "jsonContains",
        }
    }
}

/// The operators that shipped before the extension.
///
/// This list is the regression baseline for "the existing operators keep
/// their relative order and their SQL"; nothing in the runtime returns it.
pub const EXISTING_FILTER_OPERATORS: &[FilterOperator] = &[
    FilterOperator::Eq,
    FilterOperator::Ne,
    FilterOperator::Gt,
    FilterOperator::Lt,
    FilterOperator::Gte,
    FilterOperator::Lte,
    FilterOperator::Like,
    FilterOperator::In,
    FilterOperator::IsNull,
    FilterOperator::IsNotNull,
];

/// The operators the host renders itself: the existing ten plus the nine
/// shared extensions. This is the default whitelist of a driver.
pub const SHARED_FILTER_OPERATORS: &[FilterOperator] = &[
    FilterOperator::Eq,
    FilterOperator::Ne,
    FilterOperator::Gt,
    FilterOperator::Lt,
    FilterOperator::Gte,
    FilterOperator::Lte,
    FilterOperator::Like,
    FilterOperator::In,
    FilterOperator::IsNull,
    FilterOperator::IsNotNull,
    FilterOperator::NotIn,
    FilterOperator::Between,
    FilterOperator::NotBetween,
    FilterOperator::Contains,
    FilterOperator::NotContains,
    FilterOperator::StartsWith,
    FilterOperator::EndsWith,
    FilterOperator::IsEmpty,
    FilterOperator::IsNotEmpty,
];

/// The operators only a driver can render; a driver has to declare them.
pub const DIALECT_FILTER_OPERATORS: &[FilterOperator] = &[
    FilterOperator::Regex,
    FilterOperator::NotRegex,
    FilterOperator::JsonContains,
];

/// React `FilterCondition.connector`'s inline union `'and' | 'or'` — how
/// one condition joins the condition before it.
///
/// A thin mirror of driver-api's `FilterConnector` (the Rust-side name
/// for the same wire union): the connector hangs on the *current*
/// condition — concatenation runs left to right, so the first condition
/// has no predecessor and its own connector is meaningless, and
/// renderers ignore it. `None` means "fall back to the request-wide
/// logical operator", which is exactly the behaviour every filter list
/// had before per-condition connectors existed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterConnector {
    And,
    Or,
}

impl FilterConnector {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            FilterConnector::And => "and",
            FilterConnector::Or => "or",
        }
    }
}

/// React `FilterCondition` — one condition of the data grid's filter
/// list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterCondition {
    pub column: String,
    pub operator: FilterOperator,
    /// React `value?: Value` — the operand the condition carries; absent
    /// for the literal-less operators and wherever the caller leaves it
    /// unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    /// How this condition joins the condition before it.
    ///
    /// The first condition has no predecessor, so its own connector is
    /// meaningless and the backend ignores it. `undefined` falls back to
    /// the request-wide logical operator, which is how every filter list
    /// behaved before per-condition connectors existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector: Option<FilterConnector>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // The React suite (`types/__tests__/filters.types.test.ts`) guards
    // the *split*: the aggregate (`types/index`) must keep carrying the
    // filter types, and the connection `Value` must stay the type that
    // `FilterCondition.value` is typed by — the first six tests mirror
    // its six `it()` blocks one for one. The rest are the port's own
    // pins: the 22-literal `as_str` table (plus the closure form), the
    // three arrays' full content and order, and both `FilterCondition`
    // serialization states.

    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire)
            .unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    /// The full 22-variant vocabulary in wire order — the coverage pin's
    /// stand-in for React's "together they cover every operator".
    const ALL_OPERATORS: &[FilterOperator] = &[
        FilterOperator::Eq,
        FilterOperator::Ne,
        FilterOperator::Gt,
        FilterOperator::Lt,
        FilterOperator::Gte,
        FilterOperator::Lte,
        FilterOperator::Like,
        FilterOperator::In,
        FilterOperator::IsNull,
        FilterOperator::IsNotNull,
        FilterOperator::NotIn,
        FilterOperator::Between,
        FilterOperator::NotBetween,
        FilterOperator::Contains,
        FilterOperator::NotContains,
        FilterOperator::StartsWith,
        FilterOperator::EndsWith,
        FilterOperator::IsEmpty,
        FilterOperator::IsNotEmpty,
        FilterOperator::Regex,
        FilterOperator::NotRegex,
        FilterOperator::JsonContains,
    ];

    #[test]
    fn keeps_the_aggregate_value_and_the_connection_value_the_same_type() {
        // React: the aggregate `Value` and the connection `Value` are one
        // type — if they ever drift apart, a `FilterCondition` stops being
        // assignable to the shapes the rest of the app uses, and
        // TypeScript does not say so. Rust side: the crate root's `Value`
        // **is** `connection_types::Value` (one alias), so a value built on
        // either path type-checks on the other, in both directions.
        let from_connection: crate::connection_types::Value =
            serde_json::json!({ "nested": [1, 2] });
        let through_aggregate: &crate::Value = &from_connection;
        let back_to_connection: &crate::connection_types::Value = through_aggregate;
        assert_eq!(back_to_connection, &serde_json::json!({ "nested": [1, 2] }));
    }

    #[test]
    fn keeps_filter_condition_the_same_type_through_the_aggregate() {
        // React: the re-export has to be a re-export and not a move. Rust
        // side: the crate root's `FilterCondition` is this module's type,
        // in both directions.
        let from_file: crate::filter_types::FilterCondition = FilterCondition {
            column: String::from("id"),
            operator: FilterOperator::Eq,
            value: Some(serde_json::json!(1)),
            connector: None,
        };
        let through_aggregate: &crate::FilterCondition = &from_file;
        let back_to_file: &crate::filter_types::FilterCondition = through_aggregate;
        assert_eq!(back_to_file.column, "id");
    }

    #[test]
    fn a_condition_written_against_the_aggregate_is_the_one_the_file_declares() {
        let condition: crate::FilterCondition = FilterCondition {
            column: String::from("id"),
            operator: FilterOperator::Eq,
            value: Some(serde_json::json!(1)),
            connector: None,
        };
        let as_file_type: &crate::filter_types::FilterCondition = &condition;
        assert_eq!(as_file_type.column, "id");
        assert_eq!(as_file_type.operator, FilterOperator::Eq);
        assert_eq!(as_file_type.value, Some(serde_json::json!(1)));
    }

    #[test]
    fn a_condition_value_stays_assignable_to_the_aggregate_value() {
        let condition: crate::FilterCondition = FilterCondition {
            column: String::from("id"),
            operator: FilterOperator::Eq,
            value: Some(serde_json::json!({ "nested": [1, 2] })),
            connector: None,
        };
        // The field reads out as the aggregate `Value` without conversion:
        // `FilterCondition.value` keeps its `./connection` typing.
        let value: Option<crate::Value> = condition.value.clone();
        assert_eq!(value, Some(serde_json::json!({ "nested": [1, 2] })));
    }

    #[test]
    fn the_connector_is_optional_and_only_and_or() {
        let without_connector = FilterCondition {
            column: String::from("id"),
            operator: FilterOperator::Eq,
            value: Some(serde_json::json!(1)),
            connector: None,
        };
        let with_connector = FilterCondition {
            connector: Some(FilterConnector::Or),
            ..without_connector.clone()
        };
        assert_eq!(without_connector.connector, None);
        assert_eq!(with_connector.connector, Some(FilterConnector::Or));
        // `undefined` (None) keeps the key absent on the wire.
        assert_eq!(
            serde_json::to_value(&without_connector).ok(),
            Some(serde_json::json!({ "column": "id", "operator": "eq", "value": 1 }))
        );
        assert_eq!(
            serde_json::to_value(&with_connector).ok(),
            Some(serde_json::json!({
                "column": "id",
                "operator": "eq",
                "value": 1,
                "connector": "or"
            }))
        );
        // The union is closed: both spellings parse, every near-miss
        // rejects.
        assert_eq!(
            decode::<FilterConnector>(serde_json::json!("and")),
            FilterConnector::And
        );
        assert_eq!(
            decode::<FilterConnector>(serde_json::json!("or")),
            FilterConnector::Or
        );
        assert_eq!(FilterConnector::And.as_str(), "and");
        assert_eq!(FilterConnector::Or.as_str(), "or");
        for near_miss in ["AND", "And", "OR", "xor", ""] {
            assert!(
                serde_json::from_value::<FilterConnector>(serde_json::json!(near_miss)).is_err(),
                "near-miss connector literal {near_miss:?} must not parse"
            );
        }
    }

    #[test]
    fn the_operator_lists_are_the_ones_the_rust_enum_declares() {
        let existing: Vec<&str> =
            EXISTING_FILTER_OPERATORS.iter().map(|op| op.as_str()).collect();
        assert_eq!(
            existing,
            vec![
                "eq",
                "ne",
                "gt",
                "lt",
                "gte",
                "lte",
                "like",
                "in",
                "isNull",
                "isNotNull"
            ]
        );
        assert_eq!(SHARED_FILTER_OPERATORS.len(), 19);
        let dialect: Vec<&str> =
            DIALECT_FILTER_OPERATORS.iter().map(|op| op.as_str()).collect();
        assert_eq!(dialect, vec!["regex", "notRegex", "jsonContains"]);
        // The two groups are disjoint and together they cover every
        // operator.
        assert_eq!(
            SHARED_FILTER_OPERATORS.len() + DIALECT_FILTER_OPERATORS.len(),
            22
        );
        for operator in DIALECT_FILTER_OPERATORS {
            assert!(
                !SHARED_FILTER_OPERATORS.contains(operator),
                "shared and dialect groups overlap on {}",
                operator.as_str()
            );
        }
        // Every one of the 22 variants sits in exactly one group.
        for operator in ALL_OPERATORS {
            let shared = SHARED_FILTER_OPERATORS.contains(operator);
            let dialect = DIALECT_FILTER_OPERATORS.contains(operator);
            assert!(
                shared != dialect,
                "{} must sit in exactly one operator group",
                operator.as_str()
            );
        }
    }

    #[test]
    fn as_str_pins_every_operator_literal() {
        // One row per variant: `as_str`, the serde representation and the
        // parse must all agree — the wire the frontend speaks is the TS
        // spelling.
        let table = [
            (FilterOperator::Eq, "eq"),
            (FilterOperator::Ne, "ne"),
            (FilterOperator::Gt, "gt"),
            (FilterOperator::Lt, "lt"),
            (FilterOperator::Gte, "gte"),
            (FilterOperator::Lte, "lte"),
            (FilterOperator::Like, "like"),
            (FilterOperator::In, "in"),
            (FilterOperator::IsNull, "isNull"),
            (FilterOperator::IsNotNull, "isNotNull"),
            (FilterOperator::NotIn, "notIn"),
            (FilterOperator::Between, "between"),
            (FilterOperator::NotBetween, "notBetween"),
            (FilterOperator::Contains, "contains"),
            (FilterOperator::NotContains, "notContains"),
            (FilterOperator::StartsWith, "startsWith"),
            (FilterOperator::EndsWith, "endsWith"),
            (FilterOperator::IsEmpty, "isEmpty"),
            (FilterOperator::IsNotEmpty, "isNotEmpty"),
            (FilterOperator::Regex, "regex"),
            (FilterOperator::NotRegex, "notRegex"),
            (FilterOperator::JsonContains, "jsonContains"),
        ];
        for (operator, literal) in table {
            assert_eq!(operator.as_str(), literal);
            let parsed: FilterOperator = decode(serde_json::json!(literal));
            assert_eq!(parsed, operator);
            assert_eq!(
                serde_json::to_value(operator).ok(),
                Some(serde_json::json!(literal))
            );
        }
        // The union is closed: near-miss spellings reject (the T159 audit
        // rule — a widened variant would otherwise pass silently).
        for near_miss in ["Eq", "isNulls", "notregex", "not_regex", "json_contains", ""] {
            assert!(
                serde_json::from_value::<FilterOperator>(serde_json::json!(near_miss)).is_err(),
                "near-miss operator literal {near_miss:?} must not parse"
            );
        }
    }

    #[test]
    fn as_str_maps_over_the_whole_vocabulary_as_a_closure() {
        // `as_str` is a zero-capture fn item: it must stay usable where a
        // closure is expected (`map(FilterOperator::as_str)`).
        let spellings: Vec<&'static str> =
            ALL_OPERATORS.iter().copied().map(FilterOperator::as_str).collect();
        assert_eq!(spellings.len(), 22);
        assert_eq!(spellings.first(), Some(&"eq"));
        assert_eq!(spellings[9], "isNotNull");
        assert_eq!(spellings[18], "isNotEmpty");
        assert_eq!(spellings[19], "regex");
        assert_eq!(spellings[21], "jsonContains");
    }

    #[test]
    fn shared_filter_operators_are_the_existing_ten_then_the_nine_extensions() {
        // The React file builds SHARED as `[...EXISTING_FILTER_OPERATORS,
        // …]`: the ten keep their relative order up front, the nine
        // extensions follow. The full content and order are pinned here
        // (the React test only pins the length).
        let shared: Vec<FilterOperator> = SHARED_FILTER_OPERATORS.to_vec();
        let expected: Vec<FilterOperator> = EXISTING_FILTER_OPERATORS
            .iter()
            .copied()
            .chain([
                FilterOperator::NotIn,
                FilterOperator::Between,
                FilterOperator::NotBetween,
                FilterOperator::Contains,
                FilterOperator::NotContains,
                FilterOperator::StartsWith,
                FilterOperator::EndsWith,
                FilterOperator::IsEmpty,
                FilterOperator::IsNotEmpty,
            ])
            .collect();
        assert_eq!(shared, expected);
        // The prefix really is the existing ten, in order.
        assert_eq!(
            SHARED_FILTER_OPERATORS[..EXISTING_FILTER_OPERATORS.len()].to_vec(),
            EXISTING_FILTER_OPERATORS.to_vec()
        );
    }

    #[test]
    fn a_condition_round_trips_both_serialization_states() {
        // The full state: both optional keys present.
        let full = FilterCondition {
            column: String::from("amount"),
            operator: FilterOperator::Between,
            value: Some(serde_json::json!([1, 10])),
            connector: Some(FilterConnector::Or),
        };
        let full_wire = serde_json::json!({
            "column": "amount",
            "operator": "between",
            "value": [1, 10],
            "connector": "or"
        });
        assert_eq!(serde_json::to_value(&full).ok(), Some(full_wire.clone()));
        assert_eq!(decode::<FilterCondition>(full_wire), full);

        // The minimal state: `value` and `connector` unset — the keys are
        // absent on the wire (skipped, not nulled) and stay absent on the
        // way back out.
        let bare = FilterCondition {
            column: String::from("archived_at"),
            operator: FilterOperator::IsNull,
            value: None,
            connector: None,
        };
        let bare_wire = serde_json::json!({
            "column": "archived_at",
            "operator": "isNull"
        });
        assert_eq!(serde_json::to_value(&bare).ok(), Some(bare_wire.clone()));
        let back: FilterCondition = decode(bare_wire);
        assert_eq!(back, bare);

        // A present `null` is serde's spelling of `None` for an `Option`
        // field: it decodes to unset and writes back absent — the family
        // `?:` rule's known collapse, so the wire carries "unset" one way
        // only (the absent key).
        let null_valued: FilterCondition = decode(serde_json::json!({
            "column": "id",
            "operator": "eq",
            "value": null
        }));
        assert_eq!(null_valued.value, None);
        assert_eq!(
            serde_json::to_value(&null_valued).ok(),
            Some(serde_json::json!({ "column": "id", "operator": "eq" }))
        );
    }

    // ── audit pins (T188) ──
    //
    // The port's own ten pins caught the audit's literal, serde and
    // array mutations (as_str drift, camelCase/lowercase removal, the
    // optional pair's skip/default rules, every three-array edit tried).
    // Three mutants survived green, all silent:
    //
    // - `#[serde(default)]` on `column` let `{"operator":"eq"}` decode to
    //   `column: ""` — every round-trip witness sent both required keys,
    //   so nothing pinned the required pair against the optional pair
    //   (TS's `column: string` has no runtime mirror);
    // - shrinking lib.rs's re-export to `FilterCondition` alone kept
    //   every test compiling — only `FilterCondition`/`Value` were used
    //   through `crate::` paths, not the other five root names (the T159
    //   audit's "resolves sees types only" hole, now with a value half);
    // - widening the vocabulary with a 23rd variant (`Ilike`, wire
    //   "ilike") passed all ten pins — the tables pin what each member
    //   spells, not how many members exist (the same hole T159's `grid`
    //   ViewMode mutant fell through; a near-miss list alone can only
    //   reject the spellings someone thought to try).

    #[test]
    fn a_condition_rejects_a_wire_without_its_required_keys() {
        // React `column: string` / `operator: FilterOperator` are required
        // — the `?` pair is only `value`/`connector`. A `#[serde(default)]`
        // slipped onto either required field would let a half-empty wire
        // decode into a silently-empty condition, the `column` half
        // especially: `""` is a shape a real column name could take.
        assert!(
            serde_json::from_value::<FilterCondition>(serde_json::json!({
                "operator": "eq", "value": 1
            }))
            .is_err(),
            "a wire without `column` must not decode"
        );
        assert!(
            serde_json::from_value::<FilterCondition>(serde_json::json!({
                "column": "id", "value": 1
            }))
            .is_err(),
            "a wire without `operator` must not decode"
        );
        // And the degenerate end: neither required key present.
        assert!(
            serde_json::from_value::<FilterCondition>(serde_json::json!({})).is_err(),
            "an empty wire must not decode"
        );
        // One explicit witness of the contrast — the optional pair absent,
        // the required pair present — keeps the pair-of-pairs honest.
        assert!(
            serde_json::from_value::<FilterCondition>(serde_json::json!({
                "column": "id", "operator": "eq"
            }))
            .is_ok()
        );
    }

    #[test]
    fn the_filter_vocabulary_is_reexported_at_the_crate_root() {
        // `crate::` paths only — a dropped root re-export stops this file
        // from compiling instead of silently shrinking the facade (the pin
        // style `dashboard_types` set). `FilterCondition` and `Value` are
        // already pinned by the aggregate-identity tests above; this
        // covers the other five of the six root names.
        fn resolves<T>() {}
        resolves::<crate::FilterOperator>();
        resolves::<crate::FilterConnector>();
        // The value half — `resolves` sees types only, so the three arrays
        // are *used* through their root paths.
        assert_eq!(crate::EXISTING_FILTER_OPERATORS.len(), 10);
        assert_eq!(crate::SHARED_FILTER_OPERATORS.len(), 19);
        assert_eq!(
            crate::DIALECT_FILTER_OPERATORS.first().copied(),
            Some(crate::FilterOperator::Regex)
        );
        // One live decode through the root paths keeps the pin honest.
        let condition: crate::FilterCondition = decode(serde_json::json!({
            "column": "id",
            "operator": "notIn",
            "connector": "or"
        }));
        assert_eq!(condition.operator, crate::FilterOperator::NotIn);
        assert_eq!(condition.connector, Some(crate::FilterConnector::Or));
    }

    #[test]
    fn the_operator_vocabulary_is_closed_at_twenty_two_variants() {
        // Compile-time closure witness: the match below carries one arm per
        // declared variant and no wildcard, so it stops compiling the moment
        // a 23rd variant is added — or a declared one renamed or removed.
        // The runtime tables pin what each member spells; this pins the
        // membership itself, which no array of literals can (a widened enum
        // would happily spell its extra literal too).
        fn variant_index(op: FilterOperator) -> usize {
            match op {
                FilterOperator::Eq => 0,
                FilterOperator::Ne => 1,
                FilterOperator::Gt => 2,
                FilterOperator::Lt => 3,
                FilterOperator::Gte => 4,
                FilterOperator::Lte => 5,
                FilterOperator::Like => 6,
                FilterOperator::In => 7,
                FilterOperator::IsNull => 8,
                FilterOperator::IsNotNull => 9,
                FilterOperator::NotIn => 10,
                FilterOperator::Between => 11,
                FilterOperator::NotBetween => 12,
                FilterOperator::Contains => 13,
                FilterOperator::NotContains => 14,
                FilterOperator::StartsWith => 15,
                FilterOperator::EndsWith => 16,
                FilterOperator::IsEmpty => 17,
                FilterOperator::IsNotEmpty => 18,
                FilterOperator::Regex => 19,
                FilterOperator::NotRegex => 20,
                FilterOperator::JsonContains => 21,
            }
        }
        // The witness and the wire-order array agree index for index, so
        // the closure pin runs over the same list the coverage pins do.
        for (index, operator) in ALL_OPERATORS.iter().enumerate() {
            assert_eq!(variant_index(*operator), index);
        }
        // The runtime half of the closure: the spellings a widened
        // vocabulary would accept reject (the T159 rule). `ilike` is the
        // literal the audit's surviving mutant carried.
        for near_miss in ["ilike", "nlike", "notLike", "inList", "isNullIf"] {
            assert!(
                serde_json::from_value::<FilterOperator>(serde_json::json!(near_miss)).is_err(),
                "near-miss operator literal {near_miss:?} must not parse"
            );
        }
    }
}
