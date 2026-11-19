//! Independent audit for T251 (the timestampContribution port + the
//! extension-points contracts/registry slice it brought in).
//!
//! Every assertion comes from re-reading
//! `packages/frontend/src/lib/valuePresentation/timestampContribution.ts`,
//! `packages/extension-points/src/valuePresentation/{contracts,registry}.ts`
//! and the React test `__tests__/timestampContribution.test.ts` — the six
//! adjudications (resolution order, environment-snapshot zone resolution,
//! epoch-day anchoring, reason-code passthrough, exact big-integer
//! routing, the semantic-kind gate before the null check), the override
//! matrix, and the registry's register/release/duplicate semantics.
//!
//! Type-level elisions (module header documents them): the try/catch
//! never-throws defenses and JSON-unrepresentable battery values (NaN,
//! Infinity, Symbol, functions, bigint literals — exact integers beyond
//! 2^53 arrive as JSON i64/u64 and are pinned through the string route).

use std::rc::Rc;

use serde_json::json;

use crate::support::value_presentation::contracts::{
    ColumnPresentationOverride, PresentationColumnContext, PresentationEnvironment,
    PresentationPrepareInput, PresentationResult, PreparedPresentation, SemanticKind,
    ValuePresentationContribution, VersionedPresentationConfig,
};
use crate::support::value_presentation::registry::ValuePresentationRegistry;
use crate::support::value_presentation::timestamp_contribution::{
    TimestampContribution, TIMESTAMP_CONTRIBUTION_CONFIG_VERSION, TIMESTAMP_CONTRIBUTION_ID,
    timestamp_contribution, timestamp_options_to_config_value, versioned_config,
};

// ---------------------------------------------------------------------------
// Helpers (mirroring the React test file)
// ---------------------------------------------------------------------------

const CONTRIBUTION: TimestampContribution = TimestampContribution;

fn env_shanghai() -> PresentationEnvironment<'static> {
    PresentationEnvironment { locale: "en-US", local_time_zone: "Asia/Shanghai" }
}

fn env_new_york() -> PresentationEnvironment<'static> {
    PresentationEnvironment { locale: "en-US", local_time_zone: "America/New_York" }
}

fn column(semantic_kind: SemanticKind) -> PresentationColumnContext<'static> {
    PresentationColumnContext {
        column_name: "created_at",
        database_type_name: None,
        semantic_kind,
        source: None,
    }
}

struct PrepareOptions {
    config_version: u32,
    column_override: Option<ColumnPresentationOverride>,
    column: PresentationColumnContext<'static>,
    environment: PresentationEnvironment<'static>,
}

impl Default for PrepareOptions {
    fn default() -> Self {
        Self {
            config_version: 1,
            column_override: None,
            column: column(SemanticKind::Instant),
            environment: env_shanghai(),
        }
    }
}

fn make_input(
    config: serde_json::Value,
    options: PrepareOptions,
) -> PresentationPrepareInput<'static> {
    // The config is passed in its JSON host-boundary shape; the
    // contribution validates at runtime.
    let versioned = Box::leak(Box::new(VersionedPresentationConfig {
        config_version: options.config_version,
        config,
    }));
    let r#override = Box::leak(Box::new(options.column_override));
    PresentationPrepareInput {
        config: versioned,
        column_override: r#override.as_ref(),
        column: options.column,
        environment: options.environment,
    }
}

fn prepare_with(config: serde_json::Value) -> Box<dyn PreparedPresentation> {
    prepare_opts(config, PrepareOptions::default())
}

fn prepare_opts(
    config: serde_json::Value,
    options: PrepareOptions,
) -> Box<dyn PreparedPresentation> {
    CONTRIBUTION.prepare(&make_input(config, options))
}

fn native_shanghai_iso_tz() -> serde_json::Value {
    json!({
        "interpretation": { "kind": "native" },
        "format": "iso-tz",
        "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
    })
}

fn native_utc_iso() -> serde_json::Value {
    json!({
        "interpretation": { "kind": "native" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
    })
}

fn text(value: &str) -> PresentationResult {
    PresentationResult::Text { text: value.to_string(), interpretation: None }
}

fn unchanged(reason: &str) -> PresentationResult {
    PresentationResult::unchanged(reason)
}

/// Assert the full React result shape (text + interpretation).
fn assert_text(result: &PresentationResult, expected: &str, interpretation: &str) {
    match result {
        PresentationResult::Text { text, interpretation: actual } => {
            assert_eq!(text, expected, "text");
            assert_eq!(actual.as_deref(), Some(interpretation), "interpretation");
        }
        other => panic!("expected text {expected:?}, got {other:?}"),
    }
}

fn assert_unchanged(result: &PresentationResult, reason: &str) {
    match result {
        PresentationResult::Unchanged { reason: actual } => {
            assert_eq!(actual.as_deref(), Some(reason));
        }
        other => panic!("expected unchanged {reason:?}, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

#[test]
fn declares_id_and_config_version() {
    assert_eq!(CONTRIBUTION.id(), "datazen.timestamp");
    assert_eq!(CONTRIBUTION.config_version(), 1);
    assert_eq!(TIMESTAMP_CONTRIBUTION_ID, "datazen.timestamp");
    assert_eq!(TIMESTAMP_CONTRIBUTION_CONFIG_VERSION, 1);
}

// ---------------------------------------------------------------------------
// validateConfig
// ---------------------------------------------------------------------------

#[test]
fn validate_config_accepts_minimal_native_and_returns_json() {
    let result = CONTRIBUTION.validate_config(&json!({
        "interpretation": { "kind": "native" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
    }));
    match &result {
        crate::support::value_presentation::contracts::ConfigValidationResult::Ok(config) => {
            assert_eq!(
                config,
                &json!({
                    "interpretation": { "kind": "native" },
                    "format": "iso",
                    "displayTimeZone": { "kind": "utc" },
                })
            );
            assert!(config.get("sourceTimeZone").is_none());
        }
        other => panic!("expected ok, got {other:?}"),
    }
}

#[test]
fn validate_config_keeps_source_time_zone() {
    let result = CONTRIBUTION.validate_config(&json!({
        "interpretation": { "kind": "text" },
        "format": "iso-tz",
        "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
        "sourceTimeZone": { "kind": "iana", "name": "America/New_York" },
    }));
    match &result {
        crate::support::value_presentation::contracts::ConfigValidationResult::Ok(config) => {
            assert_eq!(
                config,
                &json!({
                    "interpretation": { "kind": "text" },
                    "format": "iso-tz",
                    "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
                    "sourceTimeZone": { "kind": "iana", "name": "America/New_York" },
                })
            );
        }
        other => panic!("expected ok, got {other:?}"),
    }
}

#[test]
fn validate_config_rejects_unix_with_source_zone() {
    let result = CONTRIBUTION.validate_config(&json!({
        "interpretation": { "kind": "unix", "unit": "seconds" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
        "sourceTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
    }));
    match &result {
        crate::support::value_presentation::contracts::ConfigValidationResult::Err(problems) => {
            assert!(problems.iter().any(|problem|
                problem.path == "sourceTimeZone"
                    && problem.message == "unix interpretation must not carry a source time zone"));
        }
        other => panic!("expected err, got {other:?}"),
    }
}

#[test]
fn validate_config_rejects_unknown_fields() {
    let result = CONTRIBUTION.validate_config(&json!({
        "interpretation": { "kind": "native" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
        "rules": [],
    }));
    match &result {
        crate::support::value_presentation::contracts::ConfigValidationResult::Err(problems) => {
            assert!(problems
                .iter()
                .any(|problem| problem.path == "rules" && problem.message == "unknown field"));
        }
        other => panic!("expected err, got {other:?}"),
    }
}

#[test]
fn validate_config_rejects_non_objects_with_pathless_problem() {
    for bad in [json!(null), json!(42), json!("nope"), json!(true), json!([{}])] {
        assert_eq!(
            CONTRIBUTION.validate_config(&bad),
            crate::support::value_presentation::contracts::ConfigValidationResult::Err(vec![
                crate::support::value_presentation::contracts::ConfigValidationProblem {
                    path: String::new(),
                    message: "expected an object".to_string(),
                }
            ]),
            "input {bad}"
        );
    }
}

#[test]
fn validate_config_passes_t02_problems_through() {
    let result = CONTRIBUTION.validate_config(&json!({
        "interpretation": { "kind": "native" },
        "format": "rfc2822",
        "displayTimeZone": { "kind": "gps" },
    }));
    match &result {
        crate::support::value_presentation::contracts::ConfigValidationResult::Err(problems) => {
            assert!(problems.iter().any(|problem|
                problem.path == "format"
                    && problem.message == "expected \"iso\", \"iso-tz\" or \"locale\""));
            assert!(problems.iter().any(|problem|
                problem.path == "displayTimeZone.kind"
                    && problem.message == "expected \"utc\", \"local\" or \"iana\""));
        }
        other => panic!("expected err, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// prepare: config version + global config
// ---------------------------------------------------------------------------

#[test]
fn wrong_config_version_answers_unsupported_for_every_value() {
    let prepared = prepare_opts(
        native_utc_iso(),
        PrepareOptions { config_version: 2, ..PrepareOptions::default() },
    );
    assert_unchanged(&prepared.present(&json!("2026-01-15T14:30:00Z")), "unsupported-config-version");
    assert_unchanged(&prepared.present(&json!(null)), "unsupported-config-version");
    // Version 0 mismatches too (the config body is never inspected).
    let v0 = prepare_opts(
        json!(null),
        PrepareOptions { config_version: 0, ..PrepareOptions::default() },
    );
    assert_unchanged(&v0.present(&json!("2026-01-15T14:30:00Z")), "unsupported-config-version");
}

#[test]
fn invalid_global_config_answers_invalid_config_for_every_value() {
    let prepared = prepare_with(json!({
        "interpretation": { "kind": "native" },
        "format": "nope",
    }));
    assert_unchanged(&prepared.present(&json!("2026-01-15T14:30:00Z")), "invalid-config");
    assert_unchanged(&prepared.present(&json!(null)), "invalid-config");
}

#[test]
fn invalid_global_config_beats_raw_override() {
    let prepared = prepare_opts(
        json!({ "interpretation": { "kind": "native" }, "format": "nope" }),
        PrepareOptions {
            column_override: Some(ColumnPresentationOverride::Raw),
            ..PrepareOptions::default()
        },
    );
    assert_unchanged(&prepared.present(&json!("2026-01-15T14:30:00Z")), "invalid-config");
}

// ---------------------------------------------------------------------------
// prepare: column overrides
// ---------------------------------------------------------------------------

#[test]
fn raw_override_answers_raw_override_for_every_value() {
    let prepared = prepare_opts(
        native_shanghai_iso_tz(),
        PrepareOptions {
            column_override: Some(ColumnPresentationOverride::Raw),
            ..PrepareOptions::default()
        },
    );
    assert_unchanged(&prepared.present(&json!("2026-01-15T14:30:00Z")), "raw-override");
    assert_unchanged(&prepared.present(&json!(null)), "raw-override");
}

#[test]
fn inherit_and_absent_overrides_use_the_global_config() {
    for column_override in [None, Some(ColumnPresentationOverride::Inherit)] {
        let prepared = prepare_opts(
            native_shanghai_iso_tz(),
            PrepareOptions { column_override, ..PrepareOptions::default() },
        );
        assert_text(
            &prepared.present(&json!("2026-01-15T14:30:00Z")),
            "2026-01-15T22:30:00+08:00",
            "native",
        );
    }
}

#[test]
fn custom_override_with_matching_renderer_replaces_global() {
    let prepared = prepare_opts(
        native_utc_iso(),
        PrepareOptions {
            column_override: Some(ColumnPresentationOverride::Custom {
                renderer_id: "datazen.timestamp".to_string(),
                options: json!({
                    "interpretation": { "kind": "native" },
                    "format": "iso-tz",
                    "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
                }),
            }),
            ..PrepareOptions::default()
        },
    );
    // Global is iso/UTC, the override is iso-tz/Shanghai: the override wins.
    assert_text(
        &prepared.present(&json!("2026-01-15T14:30:00Z")),
        "2026-01-15T22:30:00+08:00",
        "native",
    );
}

#[test]
fn custom_override_with_other_renderer_answers_target_mismatch() {
    let prepared = prepare_opts(
        native_shanghai_iso_tz(),
        PrepareOptions {
            column_override: Some(ColumnPresentationOverride::Custom {
                renderer_id: "other.renderer".to_string(),
                options: native_utc_iso(),
            }),
            ..PrepareOptions::default()
        },
    );
    assert_unchanged(&prepared.present(&json!("2026-01-15T14:30:00Z")), "override-target-mismatch");
}

#[test]
fn custom_override_with_invalid_options_answers_invalid_config() {
    let prepared = prepare_opts(
        native_shanghai_iso_tz(),
        PrepareOptions {
            column_override: Some(ColumnPresentationOverride::Custom {
                renderer_id: "datazen.timestamp".to_string(),
                options: json!({
                    "interpretation": { "kind": "unix", "unit": "seconds" },
                    "format": "iso",
                    "displayTimeZone": { "kind": "utc" },
                    "sourceTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
                }),
            }),
            ..PrepareOptions::default()
        },
    );
    assert_unchanged(&prepared.present(&json!(1705316400i64)), "invalid-config");
}

// ---------------------------------------------------------------------------
// present: native interpretation
// ---------------------------------------------------------------------------

#[test]
fn native_prd_anchor_zoned_text() {
    let prepared = prepare_with(native_shanghai_iso_tz());
    assert_text(
        &prepared.present(&json!("2026-01-15T14:30:00Z")),
        "2026-01-15T22:30:00+08:00",
        "native",
    );
}

#[test]
fn native_iso_output_is_utc_regardless_of_display_zone() {
    let iso = prepare_with(json!({
        "interpretation": { "kind": "native" },
        "format": "iso",
        "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
    }));
    assert_text(&iso.present(&json!("2026-01-15T14:30:00Z")), "2026-01-15T14:30:00Z", "native");
    let prepared = prepare_with(native_shanghai_iso_tz());
    assert_text(
        &prepared.present(&json!("2026-01-15T14:30:00+08:00")),
        "2026-01-15T14:30:00+08:00",
        "native",
    );
}

#[test]
fn native_locale_output_is_text() {
    let prepared = prepare_with(json!({
        "interpretation": { "kind": "native" },
        "format": "locale",
        "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
    }));
    match prepared.present(&json!("2026-01-15T14:30:00Z")) {
        PresentationResult::Text { text, interpretation } => {
            assert!(!text.is_empty());
            assert_eq!(interpretation.as_deref(), Some("native"));
        }
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn native_display_local_resolves_from_environment_snapshot() {
    // The machine zone is UTC; the environment says Shanghai → +08:00.
    let prepared = prepare_with(json!({
        "interpretation": { "kind": "native" },
        "format": "iso-tz",
        "displayTimeZone": { "kind": "local" },
    }));
    assert_text(
        &prepared.present(&json!("2026-01-15T14:30:00Z")),
        "2026-01-15T22:30:00+08:00",
        "native",
    );
}

#[test]
fn native_does_not_auto_interpret_numbers_as_unix() {
    let prepared = prepare_with(native_shanghai_iso_tz());
    assert_unchanged(&prepared.present(&json!(1705316400000i64)), "not-applicable");
    // Exact integers beyond the f64-safe range are still numbers under
    // native — not auto-interpreted.
    assert_unchanged(&prepared.present(&json!(9007199254740993i64)), "not-applicable");
}

#[test]
fn native_falls_back_to_wall_clock_with_source_zone() {
    let prepared = prepare_with(json!({
        "interpretation": { "kind": "native" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
        "sourceTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
    }));
    assert_text(
        &prepared.present(&json!("2026-01-15 14:30:00")),
        "2026-01-15T06:30:00Z",
        "native",
    );
}

#[test]
fn native_source_local_resolves_from_environment_snapshot() {
    let prepared = prepare_opts(
        json!({
            "interpretation": { "kind": "native" },
            "format": "iso",
            "displayTimeZone": { "kind": "utc" },
            "sourceTimeZone": { "kind": "local" },
        }),
        PrepareOptions { environment: env_new_york(), ..PrepareOptions::default() },
    );
    // 2026-01-15 New York is EST (-05:00): 14:30 wall = 19:30 UTC.
    assert_text(
        &prepared.present(&json!("2026-01-15 14:30:00")),
        "2026-01-15T19:30:00Z",
        "native",
    );
}

#[test]
fn native_wall_clock_without_source_zone_passes_zoned_reason() {
    let prepared = prepare_with(native_utc_iso());
    assert_unchanged(&prepared.present(&json!("2026-01-15 14:30:00")), "invalid-format");
}

#[test]
fn native_passes_parse_reason_codes_through() {
    let prepared = prepare_with(native_utc_iso());
    assert_unchanged(&prepared.present(&json!("not-a-date")), "invalid-format");
    assert_unchanged(&prepared.present(&json!("2026-13-01T00:00:00Z")), "invalid-month");
    assert_unchanged(&prepared.present(&json!("")), "empty-input");
}

#[test]
fn native_wall_clock_fallback_reports_dst_codes() {
    let prepared = prepare_with(json!({
        "interpretation": { "kind": "native" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
        "sourceTimeZone": { "kind": "iana", "name": "America/New_York" },
    }));
    // 2026-11-01 US DST ends: 01:30 happens twice.
    assert_unchanged(&prepared.present(&json!("2026-11-01 01:30:00")), "dst-ambiguous");
    // 2026-03-08 US DST starts: 02:30 does not exist.
    assert_unchanged(&prepared.present(&json!("2026-03-08 02:30:00")), "dst-invalid");
}

// ---------------------------------------------------------------------------
// present: unix interpretation
// ---------------------------------------------------------------------------

fn unix_ms_shanghai() -> serde_json::Value {
    json!({
        "interpretation": { "kind": "unix", "unit": "milliseconds" },
        "format": "iso-tz",
        "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
    })
}

#[test]
fn unix_prd_anchor_milliseconds() {
    let prepared = prepare_with(unix_ms_shanghai());
    assert_text(
        &prepared.present(&json!(1705316400000i64)),
        "2024-01-15T19:00:00+08:00",
        "unix",
    );
}

#[test]
fn unix_accepts_seconds_numbers_strings_and_exact_large_integers() {
    let seconds = json!({
        "interpretation": { "kind": "unix", "unit": "seconds" },
        "format": "iso-tz",
        "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
    });
    let prepared = prepare_with(seconds);
    assert_text(&prepared.present(&json!(1705316400i64)), "2024-01-15T19:00:00+08:00", "unix");
    assert_text(&prepared.present(&json!("1705316400")), "2024-01-15T19:00:00+08:00", "unix");
    let ms = prepare_with(unix_ms_shanghai());
    assert_text(&ms.present(&json!("1705316400000")), "2024-01-15T19:00:00+08:00", "unix");
    // The JSON number boundary: exact integers beyond 2^53 answer
    // not-safe-integer (React's BigInt route is transport-only).
    let large = prepare_with(unix_ms_shanghai());
    assert_unchanged(&large.present(&json!(9007199254740993i64)), "not-safe-integer");
}

#[test]
fn unix_handles_zero_and_negative_values() {
    let prepared = prepare_with(json!({
        "interpretation": { "kind": "unix", "unit": "milliseconds" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
    }));
    assert_text(&prepared.present(&json!(0i64)), "1970-01-01T00:00:00Z", "unix");
    let seconds = prepare_with(json!({
        "interpretation": { "kind": "unix", "unit": "seconds" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
    }));
    assert_text(&seconds.present(&json!(-1i64)), "1969-12-31T23:59:59Z", "unix");
}

#[test]
fn unix_passes_parse_reason_codes_through() {
    let prepared = prepare_with(unix_ms_shanghai());
    assert_unchanged(&prepared.present(&json!(9007199254740992u64)), "not-safe-integer");
    assert_unchanged(&prepared.present(&json!("12.5")), "invalid-integer-syntax");
    assert_unchanged(&prepared.present(&json!(" 42")), "invalid-integer-syntax");
    assert_unchanged(&prepared.present(&json!("")), "invalid-integer-syntax");
    assert_unchanged(&prepared.present(&json!("99999999999999999")), "out-of-range");
}

// ---------------------------------------------------------------------------
// present: text interpretation
// ---------------------------------------------------------------------------

fn text_shanghai_utc() -> serde_json::Value {
    json!({
        "interpretation": { "kind": "text" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
        "sourceTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
    })
}

#[test]
fn text_interpretation_converts_wall_clock_datetime() {
    let prepared = prepare_with(text_shanghai_utc());
    assert_text(
        &prepared.present(&json!("2026-01-15 14:30:00")),
        "2026-01-15T06:30:00Z",
        "text",
    );
}

#[test]
fn text_treats_date_only_as_midnight_in_source_zone() {
    let prepared = prepare_with(text_shanghai_utc());
    // 2026-03-01 00:00 +08:00 = 2026-02-28 16:00 UTC (2026 is not a leap year).
    assert_text(&prepared.present(&json!("2026-03-01")), "2026-02-28T16:00:00Z", "text");
}

#[test]
fn text_anchors_time_only_to_epoch_day() {
    let prepared = prepare_with(text_shanghai_utc());
    assert_text(&prepared.present(&json!("14:30:00")), "1970-01-01T06:30:00Z", "text");
}

#[test]
fn text_without_source_zone_answers_missing_source() {
    let prepared = prepare_with(json!({
        "interpretation": { "kind": "text" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
    }));
    assert_unchanged(
        &prepared.present(&json!("2026-01-15 14:30:00")),
        "missing-source-time-zone",
    );
}

#[test]
fn text_passes_parse_and_dst_reason_codes_through() {
    let prepared = prepare_with(text_shanghai_utc());
    assert_unchanged(&prepared.present(&json!("not-a-date")), "invalid-format");
    let new_york = prepare_with(json!({
        "interpretation": { "kind": "text" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
        "sourceTimeZone": { "kind": "iana", "name": "America/New_York" },
    }));
    assert_unchanged(&new_york.present(&json!("2026-11-01 01:30:00")), "dst-ambiguous");
    assert_unchanged(&new_york.present(&json!("2026-03-08 02:30:00")), "dst-invalid");
}

#[test]
fn text_only_accepts_strings() {
    let prepared = prepare_with(text_shanghai_utc());
    assert_unchanged(&prepared.present(&json!(1705316400000i64)), "not-applicable");
    assert_unchanged(&prepared.present(&json!(9007199254740993i64)), "not-applicable");
}

// ---------------------------------------------------------------------------
// present: null, inapplicable values, the kind gate, format failures
// ---------------------------------------------------------------------------

#[test]
fn null_answers_reason_null() {
    let prepared = prepare_with(native_shanghai_iso_tz());
    assert_unchanged(&prepared.present(&json!(null)), "null");
}

#[test]
fn non_temporal_values_answer_not_applicable() {
    let prepared = prepare_with(native_shanghai_iso_tz());
    assert_unchanged(&prepared.present(&json!(true)), "not-applicable");
    assert_unchanged(&prepared.present(&json!([2026])), "not-applicable");
    assert_unchanged(&prepared.present(&json!({ "year": 2026 })), "not-applicable");
}

#[test]
fn semantic_other_never_applies_even_for_null() {
    let prepared = prepare_opts(
        native_shanghai_iso_tz(),
        PrepareOptions {
            column: column(SemanticKind::Other),
            ..PrepareOptions::default()
        },
    );
    assert_unchanged(&prepared.present(&json!("2026-01-15T14:30:00Z")), "not-applicable");
    // The gate runs before the null check.
    assert_unchanged(&prepared.present(&json!(null)), "not-applicable");
}

#[test]
fn semantic_number_blocks_native_but_allows_unix() {
    let native = prepare_opts(
        native_shanghai_iso_tz(),
        PrepareOptions {
            column: column(SemanticKind::Number),
            ..PrepareOptions::default()
        },
    );
    assert_unchanged(&native.present(&json!("2026-01-15T14:30:00Z")), "not-applicable");
    let unix = prepare_opts(
        unix_ms_shanghai(),
        PrepareOptions {
            column: column(SemanticKind::Number),
            ..PrepareOptions::default()
        },
    );
    assert_text(&unix.present(&json!(1705316400000i64)), "2024-01-15T19:00:00+08:00", "unix");
}

#[test]
fn other_semantic_kinds_follow_the_interpretation() {
    for kind in [
        SemanticKind::Date,
        SemanticKind::Time,
        SemanticKind::LocalDatetime,
        SemanticKind::Text,
        SemanticKind::Instant,
    ] {
        let prepared = prepare_opts(
            native_shanghai_iso_tz(),
            PrepareOptions { column: column(kind), ..PrepareOptions::default() },
        );
        assert_text(
            &prepared.present(&json!("2026-01-15T14:30:00Z")),
            "2026-01-15T22:30:00+08:00",
            "native",
        );
    }
}

#[test]
fn format_failures_surface_the_format_reason() {
    // 'Mars/Olympus' passes the IANA format check but fails resolution.
    let prepared = prepare_with(json!({
        "interpretation": { "kind": "native" },
        "format": "iso-tz",
        "displayTimeZone": { "kind": "iana", "name": "Mars/Olympus" },
    }));
    assert_unchanged(&prepared.present(&json!("2026-01-15T14:30:00Z")), "invalid-time-zone");
    // An empty locale is rejected by the format layer (only for the locale
    // format; iso-tz output never touches the locale).
    let locale_prepared = prepare_opts(
        json!({
            "interpretation": { "kind": "native" },
            "format": "locale",
            "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
        }),
        PrepareOptions {
            environment: PresentationEnvironment { locale: "", local_time_zone: "Asia/Shanghai" },
            ..PrepareOptions::default()
        },
    );
    assert_unchanged(&locale_prepared.present(&json!("2026-01-15T14:30:00Z")), "invalid-locale");
}

#[test]
fn present_answers_result_kinds_across_odd_values() {
    let prepared = prepare_with(native_shanghai_iso_tz());
    let odd = [
        json!(null),
        json!(true),
        json!(false),
        json!(0),
        json!(-1i64),
        json!(9007199254740992u64),
        json!(""),
        json!(" "),
        json!("2026-02-30T00:00:00Z"),
        json!([]),
        json!([null]),
        json!({}),
        json!({ "toString": 1 }),
    ];
    for value in odd {
        let result = prepared.present(&value);
        assert!(
            matches!(
                result,
                PresentationResult::Text { .. } | PresentationResult::Unchanged { .. }
            ),
            "value {value} must answer a result kind, got {result:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// timestampOptionsToConfigValue + versioned helper
// ---------------------------------------------------------------------------

#[test]
fn options_to_config_value_shapes_canonical_json() {
    use crate::support::value_presentation::timestamp_config::{
        FormatSerde, TimestampInterpretation, TimestampOptions, UnixUnitSerde,
    };
    use crate::support::value_presentation::timestamp_format::TimeZoneSpec;

    let options = TimestampOptions {
        interpretation: TimestampInterpretation::Unix {
            unit: UnixUnitSerde::Milliseconds,
        },
        format: FormatSerde::IsoTz,
        display_time_zone: TimeZoneSpec::Iana { name: "Asia/Shanghai".to_string() },
        source_time_zone: None,
    };
    assert_eq!(
        timestamp_options_to_config_value(&options),
        json!({
            "interpretation": { "kind": "unix", "unit": "milliseconds" },
            "format": "iso-tz",
            "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
        })
    );
    let with_source = TimestampOptions {
        source_time_zone: Some(TimeZoneSpec::Iana { name: "America/New_York".to_string() }),
        ..options
    };
    let value = timestamp_options_to_config_value(&with_source);
    assert_eq!(value["sourceTimeZone"], json!({ "kind": "iana", "name": "America/New_York" }));
    // The helper wires the declared version.
    assert_eq!(versioned_config(json!({})).config_version, TIMESTAMP_CONTRIBUTION_CONFIG_VERSION);
}

// ---------------------------------------------------------------------------
// Registry integration (register / release / duplicate — the surface the
// contribution's host actually uses)
// ---------------------------------------------------------------------------

#[test]
fn registry_register_prepare_and_release() {
    let mut registry = ValuePresentationRegistry::new();
    let handle = registry
        .register(timestamp_contribution())
        .expect("first registration succeeds");
    let registered = registry.get("datazen.timestamp").expect("registered");
    let prepared = registered.prepare(&make_input(
        json!({
            "interpretation": { "kind": "native" },
            "format": "iso-tz",
            "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
        }),
        PrepareOptions::default(),
    ));
    assert_text(
        &prepared.present(&json!("2026-01-15T14:30:00Z")),
        "2026-01-15T22:30:00+08:00",
        "native",
    );
    // The release handle removes exactly this registration.
    registry.release(handle);
    assert!(registry.get("datazen.timestamp").is_none());
}

#[test]
fn registry_rejects_duplicate_ids_and_keeps_original() {
    let mut registry = ValuePresentationRegistry::new();
    registry
        .register(timestamp_contribution())
        .expect("first registration succeeds");
    let error = registry
        .register(timestamp_contribution())
        .expect_err("duplicate must be rejected");
    assert!(
        error.contains("duplicate contribution id 'datazen.timestamp'"),
        "error message: {error}"
    );
    assert!(registry.get("datazen.timestamp").is_some());
}

#[test]
fn registry_revision_and_listener_semantics() {
    let mut registry = ValuePresentationRegistry::new();
    assert_eq!(registry.revision(), 0);
    let mut notifications = 0;
    let subscription = registry.subscribe(Box::new(|| {
        // Counting through a shared cell would need interior mutability;
        // the flag below uses a thread-local counter instead.
        NOTIFICATIONS.with(|cell| cell.set(cell.get() + 1));
    }));
    let handle = registry.register(timestamp_contribution()).expect("register");
    assert_eq!(registry.revision(), 1);
    registry.unregister("unknown-id"); // no-op: no bump, no notify
    assert_eq!(registry.revision(), 1);
    registry.release(handle);
    assert_eq!(registry.revision(), 2);
    registry.unsubscribe(subscription);
    registry.register(timestamp_contribution()).expect("register after release");
    assert_eq!(registry.revision(), 3);
    let count = NOTIFICATIONS.with(|cell| cell.get());
    assert_eq!(count, 2, "listeners fired for the two real changes only");
    let listed = registry.list();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id(), "datazen.timestamp");
}

thread_local! {
    static NOTIFICATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

// Silence unused warnings for helper aliases kept for audit readability.
#[allow(dead_code)]
fn _unused(_: Option<Rc<dyn ValuePresentationContribution>>) {}

#[test]
fn type_aliases_compile_at_expected_shapes() {
    // The override's serde spellings are the settings-storage contract.
    assert_eq!(
        serde_json::to_value(ColumnPresentationOverride::Raw).unwrap(),
        json!({ "mode": "raw" })
    );
    assert_eq!(
        serde_json::to_value(ColumnPresentationOverride::Custom {
            renderer_id: "datazen.timestamp".to_string(),
            options: json!({ "a": 1 }),
        })
        .unwrap(),
        json!({ "mode": "custom", "rendererId": "datazen.timestamp", "options": { "a": 1 } })
    );
    let parsed: ColumnPresentationOverride =
        serde_json::from_value(json!({ "mode": "inherit" })).unwrap();
    assert_eq!(parsed, ColumnPresentationOverride::Inherit);
}
