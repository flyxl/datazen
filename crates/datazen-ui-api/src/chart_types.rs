//! Chart vocabulary — ported from React
//! `packages/frontend/src/types/chart.ts` (tasklist T090, 61 lines).
//!
//! The chart pipeline's contracts in one small file: the three exported
//! string-literal unions ([`ChartType`], [`AggregationType`],
//! [`InferredFieldType`]), the column descriptors field inference reads
//! ([`ChartField`]), the chart panel config ([`ChartConfig`], with the
//! optional numeric-time-axis display extension), the recommendation
//! payload ([`ChartRecommendation`]), and the dynamic per-row shape the
//! chart transform feeds renderers ([`ChartDataPoint`]). Pure contracts
//! — no IO, no state, so the module compiles for wasm32 exactly like its
//! siblings.
//!
//! ## Mirror rules applied
//!
//! - A React string-literal union becomes a Rust enum whose `as_str`
//!   pins the exact React literal, and whose serde representation
//!   **is** that literal (the wire the frontend speaks is the React
//!   spelling). `ChartConfig.sortBy`'s inline union
//!   (`'x_asc' | 'x_desc' | 'y_asc' | 'y_desc' | 'none'`) gets the same
//!   treatment as the named enum [`ChartSortBy`] — the same move
//!   `ai_types::write` made for the audit `decision` union. The full
//!   literal tables are pinned by tests, one row per variant.
//! - `interface` → struct; optional (`?:`) fields (`distinctCount`,
//!   `sampleValues`, `timeDomain`, `timeTicks`) become `Option` with
//!   `#[serde(default, skip_serializing_if = "Option::is_none")]` so an
//!   absent key round-trips as an absent key.
//! - Required-but-nullable fields (`xAxis`/`groupBy`: `string | null`
//!   with no `?`) keep the null shape: `Option` **without**
//!   `skip_serializing_if`, so `None` serializes as an explicit `null`
//!   and the key stays present — React's `DEFAULT_CHART_CONFIG` writes
//!   `xAxis: null`, and a pin test asserts that exact wire.
//! - JS `number` follows the crate's double precedent (`ai_sessions`'
//!   `iso_from_epoch_ms`): `confidence`, `timeDomain`, and `timeTicks`
//!   are `f64`. `distinctCount` is a count → `u64` (the `useCount` /
//!   `rowCount` precedent).
//! - `sampleValues?: unknown[]` → `Option<Vec<serde_json::Value>>` —
//!   the crate's `unknown` rule: opaque `Value`, nulls included.
//! - `ChartDataPoint`'s index signature `[key: string]: string |
//!   number | null` becomes a newtype over `BTreeMap<String,
//!   ChartDataPointValue>` with `#[serde(transparent)]`, so the wire
//!   stays a plain JSON object; the closed value union gets its own
//!   untagged enum ([`ChartDataPointValue`]). JS object key order is
//!   insertion order, but the transform reads points by column key and
//!   never observes object order, so `BTreeMap`'s sorted iteration is
//!   inert on the semantic while keeping construction deterministic
//!   (two points built in different orders compare equal).
//! - `DEFAULT_CHART_CONFIG` holds a `String` (`colorScheme: 'default'`)
//!   and so cannot be a `const` (the `DEFAULT_AI_MEMORY_SETTINGS`
//!   trick only works for String-less literals); it ports as a
//!   hand-written `Default` implementation whose every field is pinned
//!   by a test — `ChartConfig::default()` serializes to the React
//!   literal object, key for key.
//!
//! The display-vs-wire split React documents next door in
//! `types/dashboard.ts`: `timeDomain`/`timeTicks` are rendering-time
//! *display* extensions — present on this (display) version of
//! `ChartConfig`, absent on the backend's wire version, structurally
//! compatible because both extensions are optional. This module
//! mirrors the display version `types/chart.ts` declares.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// React `ChartType = 'bar' | 'line' | 'pie' | 'scatter' | 'area'` — the
/// renderer family a chart panel uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartType {
    Bar,
    Line,
    Pie,
    Scatter,
    Area,
}

impl ChartType {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bar => "bar",
            Self::Line => "line",
            Self::Pie => "pie",
            Self::Scatter => "scatter",
            Self::Area => "area",
        }
    }
}

impl Default for ChartType {
    /// React's `DEFAULT_CHART_CONFIG.chartType` is `'bar'`.
    fn default() -> Self {
        Self::Bar
    }
}

/// React `AggregationType = 'none' | 'sum' | 'avg' | 'count' | 'min' |
/// 'max' | 'distinct_count'` — how a series folds its rows. The `'none'`
/// variant is named for the React literal, not for `Option::None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregationType {
    None,
    Sum,
    Avg,
    Count,
    Min,
    Max,
    DistinctCount,
}

impl AggregationType {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Sum => "sum",
            Self::Avg => "avg",
            Self::Count => "count",
            Self::Min => "min",
            Self::Max => "max",
            Self::DistinctCount => "distinct_count",
        }
    }
}

impl Default for AggregationType {
    /// React's `DEFAULT_CHART_CONFIG.aggregation` is `'none'`.
    fn default() -> Self {
        Self::None
    }
}

/// React `InferredFieldType = 'numeric' | 'datetime' | 'categorical' |
/// 'boolean' | 'unknown'` — the role the chart field inference assigns a
/// column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferredFieldType {
    Numeric,
    Datetime,
    Categorical,
    Boolean,
    Unknown,
}

impl InferredFieldType {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Numeric => "numeric",
            Self::Datetime => "datetime",
            Self::Categorical => "categorical",
            Self::Boolean => "boolean",
            Self::Unknown => "unknown",
        }
    }
}

/// React `ChartConfig.sortBy`'s inline union
/// `'x_asc' | 'x_desc' | 'y_asc' | 'y_desc' | 'none'` — how the rendered
/// series are ordered. React declares it inline with no exported name;
/// the mirror gives it one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartSortBy {
    XAsc,
    XDesc,
    YAsc,
    YDesc,
    None,
}

impl ChartSortBy {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::XAsc => "x_asc",
            Self::XDesc => "x_desc",
            Self::YAsc => "y_asc",
            Self::YDesc => "y_desc",
            Self::None => "none",
        }
    }
}

impl Default for ChartSortBy {
    /// React's `DEFAULT_CHART_CONFIG.sortBy` is `'none'`.
    fn default() -> Self {
        Self::None
    }
}

/// React `ChartField` — one column as the chart field inference sees it:
/// the declared type, the inferred role, and optional profile evidence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartField {
    pub name: String,
    /// The column's declared SQL type, driver spelling verbatim.
    pub data_type: String,
    pub inferred_type: InferredFieldType,
    /// Optional distinct count from column profiling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distinct_count: Option<u64>,
    /// Optional sample values — React `unknown[]`, opaque JSON here
    /// (nulls included).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_values: Option<Vec<Value>>,
}

/// React `ChartConfig` — the chart panel's settings. `xAxis`/`groupBy`
/// are required-but-nullable (`string | null` with no `?`): they
/// serialize as an explicit `null` when `None` — no
/// `skip_serializing_if`, so the keys stay present on the wire.
/// `timeDomain`/`timeTicks` are the optional display extension (see
/// module docs).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartConfig {
    pub chart_type: ChartType,
    pub x_axis: Option<String>,
    pub y_axes: Vec<String>,
    pub group_by: Option<String>,
    pub aggregation: AggregationType,
    pub sort_by: ChartSortBy,
    pub show_legend: bool,
    pub show_grid: bool,
    pub show_values: bool,
    pub color_scheme: String,
    /// Optional: render the X axis as a numeric time axis (`HH:mm:ss`).
    /// `timeDomain` pins the displayed data range `[startMs, endMs]` —
    /// the two ends of the rolling window. While set, the chart uses
    /// `type="number" scale="time" domain`, draws no data points, and
    /// disables animation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_domain: Option<(f64, f64)>,
    /// Optional: explicit ticks, in the same unit as `timeDomain` (ms).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_ticks: Option<Vec<f64>>,
}

impl Default for ChartConfig {
    /// React `DEFAULT_CHART_CONFIG` — the exact literal object. A
    /// `String` field (`colorScheme: 'default'`) rules out a `const`,
    /// and `showLegend`/`showGrid` being `true` rules out a derived
    /// `Default` (bools would fold to `false`), so the hand-written
    /// impl is the port; a test pins every field.
    fn default() -> Self {
        Self {
            chart_type: ChartType::Bar,
            x_axis: None,
            y_axes: Vec::new(),
            group_by: None,
            aggregation: AggregationType::None,
            sort_by: ChartSortBy::None,
            show_legend: true,
            show_grid: true,
            show_values: false,
            color_scheme: String::from("default"),
            time_domain: None,
            time_ticks: None,
        }
    }
}

/// React `ChartRecommendation` — one candidate the chart recommender
/// proposes for a dataset. `xAxis`/`groupBy` are required-but-nullable
/// (`string | null` without `?`): they serialize as an explicit `null`
/// when `None`, keys present.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartRecommendation {
    pub chart_type: ChartType,
    pub x_axis: Option<String>,
    pub y_axes: Vec<String>,
    pub group_by: Option<String>,
    pub aggregation: AggregationType,
    /// How sure the recommender is (`0.0`–`1.0`).
    pub confidence: f64,
    /// Human-readable justification shown with the candidate.
    pub reason: String,
}

/// One cell of a [`ChartDataPoint`] — React's closed
/// `string | number | null` union, untagged so the wire stays the bare
/// JSON value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChartDataPointValue {
    Str(String),
    Number(f64),
    Null,
}

/// React `ChartDataPoint` — `[key: string]: string | number | null`: one
/// transformed row, keyed by column name (any column may key the row).
/// A newtype over `BTreeMap` with `#[serde(transparent)]`, so the wire
/// stays a plain JSON object. JS object key order is insertion order;
/// points are read by column key and never by object order, so the
/// sorted iteration is inert on the semantic while making construction
/// deterministic — see module docs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChartDataPoint(pub BTreeMap<String, ChartDataPointValue>);

#[cfg(test)]
mod tests {
    use super::*;

    // No React test suite backs this file — these tests are the port's
    // own pins: every literal table, the DEFAULT_CHART_CONFIG constant,
    // and the Option/null/dynamic-key shapes the mirror rules promise.

    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire)
            .unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    #[test]
    fn chart_types_keep_the_react_literals() {
        let table = [
            (ChartType::Bar, "bar"),
            (ChartType::Line, "line"),
            (ChartType::Pie, "pie"),
            (ChartType::Scatter, "scatter"),
            (ChartType::Area, "area"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: ChartType = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(serde_json::to_value(parsed).ok(), Some(serde_json::json!(literal)));
        }
        // DEFAULT_CHART_CONFIG.chartType is 'bar'.
        assert_eq!(ChartType::default(), ChartType::Bar);
    }

    #[test]
    fn aggregation_types_keep_the_react_literals() {
        let table = [
            (AggregationType::None, "none"),
            (AggregationType::Sum, "sum"),
            (AggregationType::Avg, "avg"),
            (AggregationType::Count, "count"),
            (AggregationType::Min, "min"),
            (AggregationType::Max, "max"),
            (AggregationType::DistinctCount, "distinct_count"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: AggregationType = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(serde_json::to_value(parsed).ok(), Some(serde_json::json!(literal)));
        }
        // DEFAULT_CHART_CONFIG.aggregation is 'none'.
        assert_eq!(AggregationType::default(), AggregationType::None);
    }

    #[test]
    fn inferred_field_types_keep_the_react_literals() {
        let table = [
            (InferredFieldType::Numeric, "numeric"),
            (InferredFieldType::Datetime, "datetime"),
            (InferredFieldType::Categorical, "categorical"),
            (InferredFieldType::Boolean, "boolean"),
            (InferredFieldType::Unknown, "unknown"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: InferredFieldType = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(serde_json::to_value(parsed).ok(), Some(serde_json::json!(literal)));
        }
    }

    #[test]
    fn sort_modes_keep_the_react_literals() {
        // The inline union inside ChartConfig.sortBy, one row per variant.
        let table = [
            (ChartSortBy::XAsc, "x_asc"),
            (ChartSortBy::XDesc, "x_desc"),
            (ChartSortBy::YAsc, "y_asc"),
            (ChartSortBy::YDesc, "y_desc"),
            (ChartSortBy::None, "none"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: ChartSortBy = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(serde_json::to_value(parsed).ok(), Some(serde_json::json!(literal)));
        }
        // DEFAULT_CHART_CONFIG.sortBy is 'none'.
        assert_eq!(ChartSortBy::default(), ChartSortBy::None);
    }

    #[test]
    fn default_chart_config_pins_the_react_constant() {
        let default = ChartConfig::default();
        assert_eq!(default.chart_type, ChartType::Bar);
        assert_eq!(default.x_axis, None);
        assert!(default.y_axes.is_empty());
        assert_eq!(default.group_by, None);
        assert_eq!(default.aggregation, AggregationType::None);
        assert_eq!(default.sort_by, ChartSortBy::None);
        assert!(default.show_legend);
        assert!(default.show_grid);
        assert!(!default.show_values);
        assert_eq!(default.color_scheme, "default");
        assert_eq!(default.time_domain, None);
        assert_eq!(default.time_ticks, None);
        // The React literal object, key for key: required-but-nullable
        // fields keep their explicit `null`, the optional display
        // extensions stay absent.
        assert_eq!(
            serde_json::to_value(&default).ok(),
            Some(serde_json::json!({
                "chartType": "bar",
                "xAxis": null,
                "yAxes": [],
                "groupBy": null,
                "aggregation": "none",
                "sortBy": "none",
                "showLegend": true,
                "showGrid": true,
                "showValues": false,
                "colorScheme": "default"
            }))
        );
    }

    #[test]
    fn a_chart_config_round_trips_its_null_and_optional_states() {
        // Every Option field in its Some state: the nullable keys carry
        // values and the display extensions are present.
        let full = ChartConfig {
            chart_type: ChartType::Line,
            x_axis: Some(String::from("ts")),
            y_axes: vec![String::from("cpu"), String::from("mem")],
            group_by: Some(String::from("host")),
            aggregation: AggregationType::Avg,
            sort_by: ChartSortBy::XDesc,
            show_legend: false,
            show_grid: false,
            show_values: true,
            color_scheme: String::from("ocean"),
            time_domain: Some((1_767_225_600_000.0, 1_767_225_660_000.0)),
            time_ticks: Some(vec![0.0, 1.5, 2.5]),
        };
        let wire = serde_json::json!({
            "chartType": "line",
            "xAxis": "ts",
            "yAxes": ["cpu", "mem"],
            "groupBy": "host",
            "aggregation": "avg",
            "sortBy": "x_desc",
            "showLegend": false,
            "showGrid": false,
            "showValues": true,
            "colorScheme": "ocean",
            "timeDomain": [1767225600000.0, 1767225660000.0],
            "timeTicks": [0.0, 1.5, 2.5]
        });
        assert_eq!(serde_json::to_value(&full).ok(), Some(wire.clone()));
        let back: ChartConfig = decode(wire);
        assert_eq!(back, full);

        // The None state: nullable keys stay present as explicit nulls,
        // optional keys vanish from the wire.
        let bare = ChartConfig {
            x_axis: None,
            group_by: None,
            time_domain: None,
            time_ticks: None,
            ..ChartConfig::default()
        };
        let bare_wire = serde_json::json!({
            "chartType": "bar",
            "xAxis": null,
            "yAxes": [],
            "groupBy": null,
            "aggregation": "none",
            "sortBy": "none",
            "showLegend": true,
            "showGrid": true,
            "showValues": false,
            "colorScheme": "default"
        });
        assert_eq!(serde_json::to_value(&bare).ok(), Some(bare_wire.clone()));
        let bare_back: ChartConfig = decode(bare_wire);
        assert_eq!(bare_back, bare);
    }

    #[test]
    fn chart_fields_round_trip_their_optionality() {
        let wire = serde_json::json!({
            "name": "created_at",
            "dataType": "timestamp with time zone",
            "inferredType": "datetime",
            "distinctCount": 4096,
            "sampleValues": ["2026-01-01", 42, null]
        });
        let field: ChartField = decode(wire.clone());
        assert_eq!(field.name, "created_at");
        assert_eq!(field.data_type, "timestamp with time zone");
        assert_eq!(field.inferred_type, InferredFieldType::Datetime);
        assert_eq!(field.distinct_count, Some(4096));
        assert_eq!(field.sample_values.as_ref().map(Vec::len), Some(3));
        assert_eq!(serde_json::to_value(&field).ok(), Some(wire));

        // Minimal: the `?:` evidence fields absent → None, and omitted
        // on the way back out.
        let minimal = serde_json::json!({
            "name": "flag",
            "dataType": "bool",
            "inferredType": "boolean"
        });
        let field: ChartField = decode(minimal.clone());
        assert_eq!(field.inferred_type, InferredFieldType::Boolean);
        assert_eq!(field.distinct_count, None);
        assert_eq!(field.sample_values, None);
        assert_eq!(serde_json::to_value(&field).ok(), Some(minimal));
    }

    #[test]
    fn a_recommendation_round_trips_its_null_shape() {
        // Unbound axes: the required-but-nullable fields are explicit
        // nulls in and out.
        let wire = serde_json::json!({
            "chartType": "scatter",
            "xAxis": null,
            "yAxes": ["latency_ms"],
            "groupBy": null,
            "aggregation": "distinct_count",
            "confidence": 0.8,
            "reason": "two numeric columns, no time axis"
        });
        let rec: ChartRecommendation = decode(wire.clone());
        assert_eq!(rec.chart_type, ChartType::Scatter);
        assert_eq!(rec.x_axis, None);
        assert_eq!(rec.y_axes, vec![String::from("latency_ms")]);
        assert_eq!(rec.group_by, None);
        assert_eq!(rec.aggregation, AggregationType::DistinctCount);
        assert_eq!(rec.confidence, 0.8);
        assert_eq!(rec.reason, "two numeric columns, no time axis");
        assert_eq!(serde_json::to_value(&rec).ok(), Some(wire));

        // Bound axes: the nullable fields present.
        let wire = serde_json::json!({
            "chartType": "bar",
            "xAxis": "category",
            "yAxes": ["amount"],
            "groupBy": "region",
            "aggregation": "sum",
            "confidence": 0.65,
            "reason": "low-cardinality category axis"
        });
        let rec: ChartRecommendation = decode(wire.clone());
        assert_eq!(rec.x_axis.as_deref(), Some("category"));
        assert_eq!(rec.group_by.as_deref(), Some("region"));
        assert_eq!(serde_json::to_value(&rec).ok(), Some(wire));
    }

    #[test]
    fn data_points_carry_dynamic_keys_in_deterministic_order() {
        // Any column name may key the row; built in scrambled order, the
        // same point twice compares equal and iterates sorted.
        let mut point = ChartDataPoint(BTreeMap::new());
        point.0.insert(String::from("y"), ChartDataPointValue::Number(7.5));
        point.0.insert(String::from("x"), ChartDataPointValue::Str(String::from("q1")));
        point.0.insert(String::from("z"), ChartDataPointValue::Null);

        let mut same = ChartDataPoint(BTreeMap::new());
        same.0.insert(String::from("z"), ChartDataPointValue::Null);
        same.0.insert(String::from("x"), ChartDataPointValue::Str(String::from("q1")));
        same.0.insert(String::from("y"), ChartDataPointValue::Number(7.5));
        assert_eq!(point, same);

        let keys: Vec<&str> = point.0.keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["x", "y", "z"]);
        assert_eq!(
            point.0.get("x"),
            Some(&ChartDataPointValue::Str(String::from("q1")))
        );
        assert!(!point.0.contains_key("missing"));
    }

    #[test]
    fn data_point_values_cover_the_string_number_null_union() {
        // The transparent newtype keeps the wire a plain JSON object with
        // the union's three spellings in place.
        let wire = serde_json::json!({
            "category": "tools",
            "amount": 120.5,
            "note": null
        });
        let point: ChartDataPoint = decode(wire.clone());
        assert_eq!(
            point.0.get("category"),
            Some(&ChartDataPointValue::Str(String::from("tools")))
        );
        assert_eq!(point.0.get("amount"), Some(&ChartDataPointValue::Number(120.5)));
        assert_eq!(point.0.get("note"), Some(&ChartDataPointValue::Null));
        assert_eq!(serde_json::to_value(&point).ok(), Some(wire));

        // An integer literal decodes as the JS double it is.
        let point: ChartDataPoint = decode(serde_json::json!({ "amount": 42 }));
        assert_eq!(point.0.get("amount"), Some(&ChartDataPointValue::Number(42.0)));

        // The union is closed: anything beyond string | number | null
        // (a boolean here) is not a chart data point.
        let rejected = serde_json::from_value::<ChartDataPoint>(serde_json::json!({
            "flag": true
        }));
        assert!(rejected.is_err());
    }
}
