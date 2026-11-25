//! Independent audit for T085 (the columnPresentation port).
//!
//! Every assertion comes from re-reading
//! `packages/frontend/src/lib/valuePresentation/columnPresentation.ts`
//! and its React test `__tests__/columnPresentation.test.ts` — override
//! validation (unknown modes/renderers/fields, whitespace rulings,
//! normalized options), snapshot reads, the PRD sample anchors, sample
//! formatting, and the preview road (Unix seconds vs milliseconds as
//! explicit choices, raw blocking a matching rule, the T09-BUG-001
//! strip-a-copy inherit preview, invalid and unavailable previews).

use std::collections::BTreeMap;

use serde_json::json;

use datazen_ui::support::value_presentation::column_presentation::{
    ColumnPresentationOverrideValidation, ColumnPresentationPreview, ColumnSourceBase,
    ComputeColumnPresentationPreviewInput, column_override_from_snapshot,
    compute_column_presentation_preview, fallback_sample_for_column,
    format_column_presentation_sample_value, snapshot_without_column_override,
    validate_column_presentation_override,
};
use datazen_ui::support::value_presentation::contracts::{
    ColumnPresentationOverride, PresentationEnvironment,
};
use datazen_ui::support::value_presentation::presentation_plan::{
    PresentationPlanColumn, ValuePresentationSnapshot,
};
use datazen_ui::support::value_presentation::registry::ValuePresentationRegistry;
use datazen_ui::support::value_presentation::timestamp_contribution::{
    TIMESTAMP_CONTRIBUTION_ID, timestamp_contribution,
};
use datazen_ui::support::value_presentation::value_presentation_rules::{
    ValuePresentationRule, validate_value_presentation_rule, ValuePresentationRuleValidation,
};

fn source_base() -> ColumnSourceBase<'static> {
    ColumnSourceBase {
        connection_id: "conn-1",
        database: Some("shop"),
        namespace: Some("public"),
        table: "orders",
    }
}

fn environment() -> PresentationEnvironment<'static> {
    PresentationEnvironment { locale: "en-US", local_time_zone: "Asia/Shanghai" }
}

fn unix_ms_options() -> serde_json::Value {
    json!({
        "interpretation": { "kind": "unix", "unit": "milliseconds" },
        "format": "iso-tz",
        "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
    })
}

fn unix_seconds_options() -> serde_json::Value {
    json!({
        "interpretation": { "kind": "unix", "unit": "seconds" },
        "format": "iso-tz",
        "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
    })
}

fn registry_with_timestamp() -> ValuePresentationRegistry {
    let mut registry = ValuePresentationRegistry::new();
    registry.register(timestamp_contribution()).expect("register");
    registry
}

fn enabled_snapshot(
    overrides: BTreeMap<String, ColumnPresentationOverride>,
    rules: Vec<ValuePresentationRule>,
) -> ValuePresentationSnapshot {
    let mut configs = BTreeMap::new();
    configs.insert(
        TIMESTAMP_CONTRIBUTION_ID.to_string(),
        datazen_ui::support::value_presentation::contracts::VersionedPresentationConfig {
            config_version: 1,
            config: json!({
                "interpretation": { "kind": "native" },
                "format": "iso-tz",
                "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
            }),
        },
    );
    ValuePresentationSnapshot {
        revision: 1,
        environment: datazen_ui::support::value_presentation::presentation_plan::PresentationEnvironmentSnapshot {
            locale: "en-US".to_string(),
            local_time_zone: "Asia/Shanghai".to_string(),
        },
        configs,
        enabled_renderer_ids: vec![TIMESTAMP_CONTRIBUTION_ID.to_string()],
        overrides,
        rules,
    }
}

fn col<'a>(name: &'a str, data_type: Option<&'a str>) -> PresentationPlanColumn<'a> {
    PresentationPlanColumn { name, data_type, cell_type: None, hints: None }
}

fn rule(input: serde_json::Value) -> ValuePresentationRule {
    match validate_value_presentation_rule(&input) {
        ValuePresentationRuleValidation::Ok(rule) => *rule,
        ValuePresentationRuleValidation::Err(problems) => {
            panic!("test fixture invalid: {problems:#?}")
        }
    }
}

// ---------------------------------------------------------------------------
// validateColumnPresentationOverride
// ---------------------------------------------------------------------------

#[test]
fn validation_accepts_inherit_and_raw_without_extras() {
    assert_eq!(
        validate_column_presentation_override(&json!({ "mode": "inherit" })),
        ColumnPresentationOverrideValidation::Ok(ColumnPresentationOverride::Inherit)
    );
    assert_eq!(
        validate_column_presentation_override(&json!({ "mode": "raw" })),
        ColumnPresentationOverrideValidation::Ok(ColumnPresentationOverride::Raw)
    );
}

#[test]
fn validation_accepts_timestamp_custom_and_normalizes_options() {
    let validated = validate_column_presentation_override(&json!({
        "mode": "custom",
        "rendererId": "datazen.timestamp",
        "options": unix_ms_options(),
    }));
    match validated {
        ColumnPresentationOverrideValidation::Ok(
            ColumnPresentationOverride::Custom { renderer_id, options },
        ) => {
            assert_eq!(renderer_id, "datazen.timestamp");
            // Canonical (normalized) stored shape.
            assert_eq!(options, unix_ms_options());
        }
        other => panic!("expected ok custom, got {other:?}"),
    }
}

#[test]
fn validation_rejects_unknown_modes_and_fields() {
    assert!(matches!(
        validate_column_presentation_override(&json!({ "mode": "other" })),
        ColumnPresentationOverrideValidation::Err(_)
    ));
    assert!(matches!(
        validate_column_presentation_override(&json!({ "mode": "raw", "extra": 1 })),
        ColumnPresentationOverrideValidation::Err(_)
    ));
    assert!(matches!(
        validate_column_presentation_override(&json!(null)),
        ColumnPresentationOverrideValidation::Err(_)
    ));
}

#[test]
fn validation_rejects_unknown_renderers() {
    let validated = validate_column_presentation_override(&json!({
        "mode": "custom",
        "rendererId": "someone.else",
        "options": unix_ms_options(),
    }));
    match validated {
        ColumnPresentationOverrideValidation::Err(problems) => {
            assert!(
                problems.iter().any(|problem| problem.path == "rendererId"),
                "problems {problems:#?}"
            );
        }
        other => panic!("expected err, got {other:?}"),
    }
}

#[test]
fn validation_applies_the_spacing_ruling() {
    // Leading/trailing space trims to the known id: accepted.
    let trimmed = validate_column_presentation_override(&json!({
        "mode": "custom",
        "rendererId": " datazen.timestamp ",
        "options": unix_ms_options(),
    }));
    assert!(matches!(trimmed, ColumnPresentationOverrideValidation::Ok(_)));
    // Inner whitespace is rejected (T07 ruling) — and the rejection is the
    // spacing problem specifically, not the unknown-renderer fallback the
    // same string would also trip (the whitespace check runs first).
    let inner = validate_column_presentation_override(&json!({
        "mode": "custom",
        "rendererId": "datazen.time stamp",
        "options": unix_ms_options(),
    }));
    match inner {
        ColumnPresentationOverrideValidation::Err(problems) => {
            assert!(
                problems
                    .iter()
                    .any(|problem| problem.path == "rendererId"
                        && problem.message == "must not contain whitespace"),
                "the spacing problem must fire: {problems:#?}"
            );
        }
        other => panic!("expected err, got {other:?}"),
    }
}

#[test]
fn validation_rejects_illegal_options_instead_of_storing() {
    // Unix interpretation must not carry a source time zone (design §5).
    let validated = validate_column_presentation_override(&json!({
        "mode": "custom",
        "rendererId": "datazen.timestamp",
        "options": {
            "interpretation": { "kind": "unix", "unit": "milliseconds" },
            "format": "iso-tz",
            "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
            "sourceTimeZone": { "kind": "utc" },
        },
    }));
    match validated {
        ColumnPresentationOverrideValidation::Err(problems) => {
            assert!(
                problems.iter().any(|problem| problem.path == "options.sourceTimeZone"),
                "problems {problems:#?}"
            );
        }
        other => panic!("expected err, got {other:?}"),
    }
    // Unix without an explicit unit is illegal: no digit-count inference.
    let no_unit = validate_column_presentation_override(&json!({
        "mode": "custom",
        "rendererId": "datazen.timestamp",
        "options": {
            "interpretation": { "kind": "unix" },
            "format": "iso-tz",
            "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
        },
    }));
    assert!(matches!(no_unit, ColumnPresentationOverrideValidation::Err(_)));
}

// ---------------------------------------------------------------------------
// columnOverrideFromSnapshot
// ---------------------------------------------------------------------------

fn override_key_for(column: &str) -> String {
    datazen_ui::support::value_presentation::presentation_plan::column_override_key(
        &source_base().with_column(column),
    )
}

#[test]
fn snapshot_read_returns_stored_override_for_sourced_columns() {
    let mut overrides = BTreeMap::new();
    overrides.insert(override_key_for("epoch_ms"), ColumnPresentationOverride::Raw);
    let snapshot = enabled_snapshot(overrides, Vec::new());
    assert_eq!(
        column_override_from_snapshot(&snapshot, Some(source_base()), "epoch_ms"),
        ColumnPresentationOverride::Raw
    );
    assert_eq!(
        column_override_from_snapshot(&snapshot, Some(source_base()), "other"),
        ColumnPresentationOverride::Inherit
    );
}

#[test]
fn snapshot_read_returns_inherit_for_sourceless_columns() {
    let mut overrides = BTreeMap::new();
    overrides.insert(override_key_for("epoch_ms"), ColumnPresentationOverride::Raw);
    let snapshot = enabled_snapshot(overrides, Vec::new());
    assert_eq!(
        column_override_from_snapshot(&snapshot, None, "epoch_ms"),
        ColumnPresentationOverride::Inherit
    );
}

// ---------------------------------------------------------------------------
// Samples
// ---------------------------------------------------------------------------

#[test]
fn fallback_samples_follow_the_prd_anchors() {
    assert_eq!(
        fallback_sample_for_column(&col("epoch_ms", Some("bigint"))),
        Some(json!(1705316400000i64))
    );
    assert_eq!(
        fallback_sample_for_column(&col("created_at", Some("timestamp with time zone"))),
        Some(json!("2026-01-15T14:30:00Z"))
    );
    assert_eq!(
        fallback_sample_for_column(&col("ts", Some("timestamp without time zone"))),
        Some(json!("2026-01-15 14:30:00"))
    );
    assert_eq!(fallback_sample_for_column(&col("name", Some("varchar"))), None);
    assert_eq!(
        fallback_sample_for_column(&col("d", Some("date"))),
        Some(json!("2026-01-15"))
    );
    assert_eq!(
        fallback_sample_for_column(&col("t", Some("time"))),
        Some(json!("14:30:00"))
    );
}

#[test]
fn sample_formatting_null_objects_json() {
    assert_eq!(format_column_presentation_sample_value(&json!(null)), "NULL");
    assert_eq!(
        format_column_presentation_sample_value(&json!(1705316400000i64)),
        "1705316400000"
    );
    assert_eq!(
        format_column_presentation_sample_value(&json!({ "a": 1 })),
        "{\"a\":1}"
    );
}

// ---------------------------------------------------------------------------
// computeColumnPresentationPreview
// ---------------------------------------------------------------------------

fn preview<'a>(
    candidate: &'a ColumnPresentationOverride,
    column: PresentationPlanColumn<'a>,
    snapshot: &'a ValuePresentationSnapshot,
    registry: &'a ValuePresentationRegistry,
    sample: serde_json::Value,
) -> ColumnPresentationPreview {
    compute_column_presentation_preview(&ComputeColumnPresentationPreviewInput {
        candidate,
        column,
        source: Some(source_base()),
        snapshot,
        environment: environment(),
        sample_value: sample,
        registry,
    })
}

#[test]
fn preview_previews_unix_milliseconds_exactly() {
    let candidate = ColumnPresentationOverride::Custom {
        renderer_id: TIMESTAMP_CONTRIBUTION_ID.to_string(),
        options: unix_ms_options(),
    };
    let snapshot = enabled_snapshot(BTreeMap::new(), Vec::new());
    let result = preview(
        &candidate,
        col("epoch_ms", Some("bigint")),
        &snapshot,
        &registry_with_timestamp(),
        json!(1705316400000i64),
    );
    assert_eq!(
        result,
        ColumnPresentationPreview::Text {
            text: "2024-01-15T19:00:00+08:00".to_string(),
            interpretation: Some("unix".to_string()),
            sample: json!(1705316400000i64),
        }
    );
}

#[test]
fn preview_treats_seconds_and_milliseconds_as_explicit_choices() {
    let snapshot = enabled_snapshot(BTreeMap::new(), Vec::new());
    let registry = registry_with_timestamp();
    let ms = preview(
        &ColumnPresentationOverride::Custom {
            renderer_id: TIMESTAMP_CONTRIBUTION_ID.to_string(),
            options: unix_ms_options(),
        },
        col("epoch_ms", Some("bigint")),
        &snapshot,
        &registry,
        json!(1705316400000i64),
    );
    let seconds = preview(
        &ColumnPresentationOverride::Custom {
            renderer_id: TIMESTAMP_CONTRIBUTION_ID.to_string(),
            options: unix_seconds_options(),
        },
        col("epoch_ms", Some("bigint")),
        &snapshot,
        &registry,
        json!(1705316400000i64),
    );
    let (ColumnPresentationPreview::Text { text: ms_text, .. }, ColumnPresentationPreview::Text {
        text: seconds_text,
        ..
    }) = (ms, seconds)
    else {
        panic!("both previews must be text");
    };
    // Same digits, explicit unit only: the ms anchor vs a far-future instant.
    assert_eq!(ms_text, "2024-01-15T19:00:00+08:00");
    assert_ne!(seconds_text, ms_text);
    assert_eq!(seconds_text, "+56009-04-29T16:00:00+08:00");
}

#[test]
fn preview_raw_blocks_a_matching_rule() {
    let rule_snapshot = enabled_snapshot(
        BTreeMap::new(),
        vec![rule(json!({
            "name": "unix-ms",
            "enabled": true,
            "match": { "column": { "kind": "exact", "value": "epoch_ms" } },
            "options": unix_ms_options(),
        }))],
    );
    let registry = registry_with_timestamp();
    // Sanity: inherit follows the rule.
    let inherited = preview(
        &ColumnPresentationOverride::Inherit,
        col("epoch_ms", Some("bigint")),
        &rule_snapshot,
        &registry,
        json!(1705316400000i64),
    );
    assert_eq!(
        inherited,
        ColumnPresentationPreview::Text {
            text: "2024-01-15T19:00:00+08:00".to_string(),
            interpretation: Some("unix".to_string()),
            sample: json!(1705316400000i64),
        }
    );
    // Raw blocks it: the column keeps its raw value.
    let raw = preview(
        &ColumnPresentationOverride::Raw,
        col("epoch_ms", Some("bigint")),
        &rule_snapshot,
        &registry,
        json!(1705316400000i64),
    );
    assert_eq!(
        raw,
        ColumnPresentationPreview::Unchanged {
            reason: Some("raw-override".to_string()),
            sample: json!(1705316400000i64),
        }
    );
}

#[test]
fn preview_inherit_strips_the_pending_override_t09_bug001() {
    let mut overrides = BTreeMap::new();
    overrides.insert(
        override_key_for("epoch_ms"),
        ColumnPresentationOverride::Custom {
            renderer_id: TIMESTAMP_CONTRIBUTION_ID.to_string(),
            options: unix_ms_options(),
        },
    );
    let rule_snapshot = enabled_snapshot(
        overrides,
        vec![rule(json!({
            "name": "iso-rule",
            "enabled": true,
            "match": { "column": { "kind": "exact", "value": "epoch_ms" } },
            "options": {
                "interpretation": { "kind": "unix", "unit": "milliseconds" },
                "format": "iso",
                "displayTimeZone": { "kind": "utc" },
            },
        }))],
    );
    let result = preview(
        &ColumnPresentationOverride::Inherit,
        col("epoch_ms", Some("bigint")),
        &rule_snapshot,
        &registry_with_timestamp(),
        json!(1705316400000i64),
    );
    // The rule's iso/UTC output, not the stored override's iso-tz output.
    assert_eq!(
        result,
        ColumnPresentationPreview::Text {
            text: "2024-01-15T11:00:00Z".to_string(),
            interpretation: Some("unix".to_string()),
            sample: json!(1705316400000i64),
        }
    );
    // The strip works on a copy: the input snapshot still holds the override.
    assert!(rule_snapshot
        .overrides
        .contains_key(&override_key_for("epoch_ms")));
    assert_eq!(
        snapshot_without_column_override(&rule_snapshot, Some(source_base()), "epoch_ms")
            .overrides
            .len(),
        0
    );
}

#[test]
fn preview_reports_invalid_for_illegal_custom() {
    let candidate = ColumnPresentationOverride::Custom {
        renderer_id: TIMESTAMP_CONTRIBUTION_ID.to_string(),
        options: json!({
            "interpretation": { "kind": "unix" },
            "format": "iso-tz",
            "displayTimeZone": { "kind": "utc" },
        }),
    };
    let snapshot = enabled_snapshot(BTreeMap::new(), Vec::new());
    let result = preview(
        &candidate,
        col("epoch_ms", Some("bigint")),
        &snapshot,
        &registry_with_timestamp(),
        json!(1705316400000i64),
    );
    assert!(matches!(result, ColumnPresentationPreview::Invalid { .. }));
}

#[test]
fn preview_reports_unavailable_when_contribution_missing() {
    let candidate = ColumnPresentationOverride::Custom {
        renderer_id: TIMESTAMP_CONTRIBUTION_ID.to_string(),
        options: unix_ms_options(),
    };
    let snapshot = enabled_snapshot(BTreeMap::new(), Vec::new());
    let empty_registry = ValuePresentationRegistry::new();
    let result = preview(
        &candidate,
        col("epoch_ms", Some("bigint")),
        &snapshot,
        &empty_registry,
        json!(1705316400000i64),
    );
    assert_eq!(
        result,
        ColumnPresentationPreview::Unavailable { sample: json!(1705316400000i64) }
    );
}
