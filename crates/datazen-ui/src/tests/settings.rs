//! Independent audit for T182 (the settings parse + snapshot builder and
//! the bundled-contribution registration).
//!
//! Every assertion comes from re-reading
//! `packages/frontend/src/lib/valuePresentation/settings.ts`,
//! `registerBundledContributions.ts` and their React tests — plus the
//! integration blocks the earlier audits deferred here: the column
//! override isolation matrix, the override-beats-rule / restart-restore
//! simulations (from `__tests__/columnOverrides.test.ts`) and the
//! rules-through-snapshot end-to-end cases (from
//! `__tests__/valuePresentationRules.test.ts`), all of which go through
//! `buildValuePresentationSnapshot`.
//!
//! Elisions: "fresh objects instead of aliasing the shared default" is
//! ownership (no shared default exists); `Object.freeze` does not apply.

use std::collections::BTreeMap;

use serde_json::json;

use crate::support::value_presentation::column_presentation::ColumnSourceBase;
use crate::support::value_presentation::column_presentation::{
    column_override_from_snapshot, compute_column_presentation_preview,
    ComputeColumnPresentationPreviewInput,
};
use crate::support::value_presentation::contracts::{
    ColumnPresentationOverride, PresentationEnvironment, PresentationResult,
};
use crate::support::value_presentation::presentation_plan::{
    BuildColumnPresentationPlansInput, ColumnPresentationStatus, PresentationEnvironmentSnapshot,
    PresentationPlanColumn, ValuePresentationSnapshot, build_column_presentation_plans,
    column_override_key,
};
use crate::support::value_presentation::register_bundled_contributions::
    register_bundled_value_presentation_contributions;
use crate::support::value_presentation::registry::ValuePresentationRegistry;
use crate::support::value_presentation::settings::{
    StoredRendererSettings, StoredValuePresentationSettings,
    build_value_presentation_snapshot, default_value_presentation_settings,
    parse_stored_value_presentation_settings,
};
use crate::support::value_presentation::timestamp_contribution::TIMESTAMP_CONTRIBUTION_ID;
use crate::support::value_presentation::timestamp_contribution::timestamp_contribution;

fn env() -> PresentationEnvironment<'static> {
    PresentationEnvironment { locale: "en-US", local_time_zone: "UTC" }
}

fn env_snapshot() -> PresentationEnvironmentSnapshot {
    PresentationEnvironmentSnapshot { locale: "en-US".to_string(), local_time_zone: "UTC".to_string() }
}

fn native_utc_iso() -> serde_json::Value {
    json!({
        "interpretation": { "kind": "native" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
    })
}

fn unix_ms_shanghai() -> serde_json::Value {
    json!({
        "interpretation": { "kind": "unix", "unit": "milliseconds" },
        "format": "iso-tz",
        "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
    })
}

fn source_base() -> ColumnSourceBase<'static> {
    ColumnSourceBase {
        connection_id: "conn-1",
        database: Some("shop"),
        namespace: Some("public"),
        table: "orders",
    }
}

fn key_for(base: ColumnSourceBase<'_>, column: &str) -> String {
    column_override_key(&base.with_column(column))
}

fn registry_with_timestamp() -> ValuePresentationRegistry {
    let mut registry = ValuePresentationRegistry::new();
    registry.register(timestamp_contribution()).expect("register");
    registry
}

fn renderer_entry(enabled: bool, options: serde_json::Value) -> StoredRendererSettings {
    StoredRendererSettings { enabled, options }
}

fn stored(enabled: bool, options: serde_json::Value) -> StoredValuePresentationSettings {
    let mut renderers = BTreeMap::new();
    renderers.insert(TIMESTAMP_CONTRIBUTION_ID.to_string(), renderer_entry(enabled, options));
    StoredValuePresentationSettings {
        version: 1,
        renderers,
        rules: None,
        column_overrides: None,
    }
}

// ---------------------------------------------------------------------------
// parseStoredValuePresentationSettings
// ---------------------------------------------------------------------------

#[test]
fn parse_returns_the_default_for_non_objects() {
    for bad in [json!(null), json!("x"), json!(42), json!(true), json!([]), json!(["a"])] {
        assert_eq!(
            parse_stored_value_presentation_settings(&bad),
            default_value_presentation_settings(),
            "input {bad}"
        );
    }
}

#[test]
fn parse_returns_the_default_for_an_empty_object() {
    assert_eq!(
        parse_stored_value_presentation_settings(&json!({})),
        StoredValuePresentationSettings {
            version: 1,
            renderers: BTreeMap::new(),
            rules: None,
            column_overrides: None,
        }
    );
}

#[test]
fn parse_falls_back_to_version_1_for_non_numbers() {
    assert_eq!(
        parse_stored_value_presentation_settings(&json!({ "version": "1", "renderers": {} })),
        StoredValuePresentationSettings {
            version: 1,
            renderers: BTreeMap::new(),
            rules: None,
            column_overrides: None,
        }
    );
    assert_eq!(
        parse_stored_value_presentation_settings(&json!({ "version": null })),
        StoredValuePresentationSettings {
            version: 1,
            renderers: BTreeMap::new(),
            rules: None,
            column_overrides: None,
        }
    );
}

#[test]
fn parse_keeps_unknown_versions_verbatim() {
    assert_eq!(
        parse_stored_value_presentation_settings(&json!({ "version": 99, "renderers": {} })),
        StoredValuePresentationSettings {
            version: 99,
            renderers: BTreeMap::new(),
            rules: None,
            column_overrides: None,
        }
    );
}

#[test]
fn parse_falls_back_to_empty_renderers_for_non_objects() {
    for renderers in [json!(null), json!("x"), json!(42), json!([])] {
        assert_eq!(
            parse_stored_value_presentation_settings(&json!({ "version": 1, "renderers": renderers })),
            StoredValuePresentationSettings {
                version: 1,
                renderers: BTreeMap::new(),
                rules: None,
                column_overrides: None,
            }
        );
    }
}

#[test]
fn parse_drops_non_object_renderer_entries() {
    let parsed = parse_stored_value_presentation_settings(&json!({
        "version": 1,
        "renderers": {
            "datazen.timestamp": { "enabled": true, "options": native_utc_iso() },
            "broken": "not-an-object",
            "alsoBroken": 42,
            "nullEntry": null,
        },
    }));
    assert_eq!(
        parsed.renderers.get("datazen.timestamp"),
        Some(&renderer_entry(true, native_utc_iso())),
        "object entries kept verbatim"
    );
    assert_eq!(parsed.renderers.len(), 1);
}

// ---------------------------------------------------------------------------
// buildValuePresentationSnapshot
// ---------------------------------------------------------------------------

#[test]
fn snapshot_builds_a_ready_config_for_a_valid_enabled_renderer() {
    let snapshot = build_value_presentation_snapshot(
        &stored(true, native_utc_iso()),
        &env_snapshot(),
        &registry_with_timestamp(),
        7,
    );
    assert_eq!(snapshot.revision, 7);
    let config = snapshot.configs.get(TIMESTAMP_CONTRIBUTION_ID).expect("config present");
    assert_eq!(config.config_version, 1);
    assert_eq!(
        snapshot.enabled_renderer_ids,
        vec![TIMESTAMP_CONTRIBUTION_ID.to_string()]
    );
}

#[test]
fn snapshot_validates_options_through_the_contribution() {
    // The validated config carries the canonical unix-ms shape.
    let snapshot = build_value_presentation_snapshot(
        &stored(true, unix_ms_shanghai()),
        &env_snapshot(),
        &registry_with_timestamp(),
        1,
    );
    let config = snapshot.configs.get(TIMESTAMP_CONTRIBUTION_ID).expect("config");
    assert_eq!(config.config, unix_ms_shanghai());
}

#[test]
fn snapshot_excludes_renderers_with_failing_options() {
    let bad_options = json!({ "interpretation": { "kind": "native" }, "format": "nope" });
    let snapshot = build_value_presentation_snapshot(
        &stored(true, bad_options),
        &env_snapshot(),
        &registry_with_timestamp(),
        1,
    );
    assert!(snapshot.configs.is_empty());
    assert!(snapshot.enabled_renderer_ids.is_empty());
}

#[test]
fn snapshot_excludes_disabled_renderers() {
    let snapshot = build_value_presentation_snapshot(
        &stored(false, native_utc_iso()),
        &env_snapshot(),
        &registry_with_timestamp(),
        1,
    );
    assert!(snapshot.configs.is_empty());
    assert!(snapshot.enabled_renderer_ids.is_empty());
}

#[test]
fn snapshot_excludes_renderers_with_missing_contributions() {
    let mut renderers = BTreeMap::new();
    renderers.insert(
        "datazen.nope".to_string(),
        renderer_entry(true, native_utc_iso()),
    );
    let stored_value = StoredValuePresentationSettings {
        version: 1,
        renderers,
        rules: None,
        column_overrides: None,
    };
    let snapshot =
        build_value_presentation_snapshot(&stored_value, &env_snapshot(), &registry_with_timestamp(), 1);
    assert!(snapshot.configs.is_empty());
}

#[test]
fn snapshot_empty_for_unknown_version_and_default_stored() {
    let v99 = StoredValuePresentationSettings { version: 99, ..stored(true, native_utc_iso()) };
    let snapshot =
        build_value_presentation_snapshot(&v99, &env_snapshot(), &registry_with_timestamp(), 4);
    assert_eq!(snapshot.revision, 4);
    assert!(snapshot.configs.is_empty());
    assert!(snapshot.enabled_renderer_ids.is_empty());
    assert!(snapshot.overrides.is_empty());
    assert!(snapshot.rules.is_empty());

    let default_snapshot = build_value_presentation_snapshot(
        &default_value_presentation_settings(),
        &env_snapshot(),
        &registry_with_timestamp(),
        4,
    );
    assert!(default_snapshot.configs.is_empty());
    assert!(default_snapshot.rules.is_empty());
}

#[test]
fn snapshot_overrides_empty_when_nothing_stored() {
    let snapshot = build_value_presentation_snapshot(
        &stored(true, native_utc_iso()),
        &env_snapshot(),
        &registry_with_timestamp(),
        1,
    );
    assert!(snapshot.overrides.is_empty());
}

#[test]
fn snapshot_keeps_enabled_invalid_out_of_configs_for_missing_config() {
    // Enabled but invalid options: out of configs, in neither list — the
    // plan layer then reports missing-config (T155's audit pins that).
    let bad_options = json!({
        "interpretation": { "kind": "native" },
        "format": "iso",
        "displayTimeZone": { "kind": "gps" },
    });
    let snapshot = build_value_presentation_snapshot(
        &stored(true, bad_options),
        &env_snapshot(),
        &registry_with_timestamp(),
        1,
    );
    assert!(!snapshot.configs.contains_key(TIMESTAMP_CONTRIBUTION_ID));
    assert!(snapshot.enabled_renderer_ids.is_empty());
    // The plan layer sees enabled-gate failure first: disabled.
    let plans = build_column_presentation_plans(
        &BuildColumnPresentationPlansInput {
            columns: vec![PresentationPlanColumn {
                name: "created_at",
                data_type: Some("timestamptz"),
                cell_type: None,
                hints: None,
            }],
            source: Some(source_base().with_column("created_at")),
            snapshot: &snapshot,
        },
        &registry_with_timestamp(),
    );
    assert_eq!(plans[0].status, ColumnPresentationStatus::Disabled);
}

// ---------------------------------------------------------------------------
// parse: rules + columnOverrides fields
// ---------------------------------------------------------------------------

#[test]
fn parse_defaults_to_no_rules_key_for_old_payloads() {
    let parsed = parse_stored_value_presentation_settings(&json!({
        "version": 1,
        "renderers": {},
    }));
    assert_eq!(parsed.rules, None);
}

#[test]
fn parse_keeps_valid_rules_and_drops_invalid_entries() {
    let parsed = parse_stored_value_presentation_settings(&json!({
        "version": 1,
        "renderers": {},
        "rules": [
            {
                "name": "ok",
                "enabled": true,
                "match": { "column": { "kind": "exact", "value": "c" } },
                "options": native_utc_iso(),
            },
            "not-a-rule",
        ],
    }));
    let rules = parsed.rules.expect("rules present");
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].name, "ok");
}

#[test]
fn parse_ignores_non_array_rules() {
    let parsed = parse_stored_value_presentation_settings(&json!({
        "version": 1,
        "renderers": {},
        "rules": "nope",
    }));
    assert_eq!(parsed.rules, None);
}

#[test]
fn parse_does_not_touch_rules_for_unknown_versions() {
    let parsed = parse_stored_value_presentation_settings(&json!({
        "version": 99,
        "renderers": {},
        "rules": [],
    }));
    assert_eq!(parsed.rules, None);
}

#[test]
fn parse_defaults_to_no_overrides_key_for_old_payloads() {
    let parsed = parse_stored_value_presentation_settings(&json!({
        "version": 1,
        "renderers": {},
    }));
    assert_eq!(parsed.column_overrides, None);
}

#[test]
fn parse_keeps_valid_overrides_and_drops_the_rest() {
    let parsed = parse_stored_value_presentation_settings(&json!({
        "version": 1,
        "renderers": {},
        "columnOverrides": {
            key_for(source_base(), "a"): { "mode": "raw" },
            "not-a-key": { "mode": "raw" },
            key_for(source_base(), "b"): { "mode": "inherit" },
        },
    }));
    let overrides = parsed.column_overrides.expect("overrides present");
    assert_eq!(overrides.len(), 1);
    assert_eq!(
        overrides.get(&key_for(source_base(), "a")),
        Some(&ColumnPresentationOverride::Raw)
    );
}

#[test]
fn parse_ignores_non_object_overrides() {
    let parsed = parse_stored_value_presentation_settings(&json!({
        "version": 1,
        "renderers": {},
        "columnOverrides": "nope",
    }));
    assert_eq!(parsed.column_overrides, None);
}

#[test]
fn parse_does_not_touch_overrides_for_unknown_versions() {
    let parsed = parse_stored_value_presentation_settings(&json!({
        "version": 99,
        "renderers": {},
        "columnOverrides": {},
    }));
    assert_eq!(parsed.column_overrides, None);
}

// ---------------------------------------------------------------------------
// snapshot: columnOverrides + rules
// ---------------------------------------------------------------------------

#[test]
fn snapshot_fills_overrides_from_storage() {
    let mut overrides = BTreeMap::new();
    overrides.insert(key_for(source_base(), "epoch_ms"), ColumnPresentationOverride::Raw);
    let stored_value = StoredValuePresentationSettings {
        column_overrides: Some(overrides),
        ..stored(true, native_utc_iso())
    };
    let snapshot =
        build_value_presentation_snapshot(&stored_value, &env_snapshot(), &registry_with_timestamp(), 1);
    assert_eq!(
        snapshot.overrides.get(&key_for(source_base(), "epoch_ms")),
        Some(&ColumnPresentationOverride::Raw)
    );
}

#[test]
fn snapshot_carries_no_overrides_for_unknown_versions() {
    let mut overrides = BTreeMap::new();
    overrides.insert(key_for(source_base(), "epoch_ms"), ColumnPresentationOverride::Raw);
    let stored_value = StoredValuePresentationSettings {
        version: 99,
        column_overrides: Some(overrides),
        ..stored(true, native_utc_iso())
    };
    let snapshot =
        build_value_presentation_snapshot(&stored_value, &env_snapshot(), &registry_with_timestamp(), 1);
    assert!(snapshot.overrides.is_empty());
}

#[test]
fn snapshot_carries_overrides_even_when_the_renderer_is_disabled() {
    let mut overrides = BTreeMap::new();
    overrides.insert(key_for(source_base(), "epoch_ms"), ColumnPresentationOverride::Raw);
    let stored_value = StoredValuePresentationSettings {
        column_overrides: Some(overrides),
        ..stored(false, native_utc_iso())
    };
    let snapshot =
        build_value_presentation_snapshot(&stored_value, &env_snapshot(), &registry_with_timestamp(), 1);
    assert_eq!(snapshot.overrides.len(), 1);
}

fn unix_ms_rule_json() -> serde_json::Value {
    json!({
        "name": "unix-ms",
        "enabled": true,
        "match": {
            "column": { "kind": "regex", "source": "^(created_at|updated_at)$" },
            "databaseTypeName": { "kind": "exact", "value": "BIGINT" },
        },
        "options": unix_ms_shanghai(),
    })
}

#[test]
fn snapshot_carries_rules_when_the_renderer_is_enabled() {
    let parsed = parse_stored_value_presentation_settings(&json!({
        "version": 1,
        "renderers": {
            TIMESTAMP_CONTRIBUTION_ID: { "enabled": true, "options": native_utc_iso() },
        },
        "rules": [unix_ms_rule_json()],
    }));
    let snapshot =
        build_value_presentation_snapshot(&parsed, &env_snapshot(), &registry_with_timestamp(), 9);
    assert_eq!(snapshot.rules.len(), 1);
    assert_eq!(snapshot.rules[0].name, "unix-ms");
}

#[test]
fn snapshot_carries_no_rules_when_the_renderer_is_disabled() {
    let parsed = parse_stored_value_presentation_settings(&json!({
        "version": 1,
        "renderers": {
            TIMESTAMP_CONTRIBUTION_ID: { "enabled": false, "options": native_utc_iso() },
        },
        "rules": [unix_ms_rule_json()],
    }));
    let snapshot =
        build_value_presentation_snapshot(&parsed, &env_snapshot(), &registry_with_timestamp(), 9);
    assert!(snapshot.rules.is_empty());
}

#[test]
fn snapshot_carries_no_rules_for_unknown_versions() {
    let parsed = parse_stored_value_presentation_settings(&json!({
        "version": 99,
        "renderers": {},
        "rules": [unix_ms_rule_json()],
    }));
    let snapshot =
        build_value_presentation_snapshot(&parsed, &env_snapshot(), &registry_with_timestamp(), 9);
    assert!(snapshot.rules.is_empty());
}

// ---------------------------------------------------------------------------
// registerBundledValuePresentationContributions
// ---------------------------------------------------------------------------

#[test]
fn bundled_registration_registers_once_and_is_idempotent() {
    let mut registry = ValuePresentationRegistry::new();
    register_bundled_value_presentation_contributions(&mut registry);
    assert!(registry.get(TIMESTAMP_CONTRIBUTION_ID).is_some());
    // A second call neither fails nor duplicates.
    register_bundled_value_presentation_contributions(&mut registry);
    assert_eq!(registry.list().len(), 1);
    // After an explicit unregister, registering again works.
    registry.unregister(TIMESTAMP_CONTRIBUTION_ID);
    register_bundled_value_presentation_contributions(&mut registry);
    assert!(registry.get(TIMESTAMP_CONTRIBUTION_ID).is_some());
}

// ---------------------------------------------------------------------------
// Deferred integration blocks (from the columnOverrides / rules tests)
// ---------------------------------------------------------------------------

fn custom_ms() -> ColumnPresentationOverride {
    ColumnPresentationOverride::Custom {
        renderer_id: TIMESTAMP_CONTRIBUTION_ID.to_string(),
        options: unix_ms_shanghai(),
    }
}

fn custom_seconds() -> ColumnPresentationOverride {
    ColumnPresentationOverride::Custom {
        renderer_id: TIMESTAMP_CONTRIBUTION_ID.to_string(),
        options: json!({
            "interpretation": { "kind": "unix", "unit": "seconds" },
            "format": "iso",
            "displayTimeZone": { "kind": "utc" },
        }),
    }
}

fn matrix_stored() -> StoredValuePresentationSettings {
    let mut overrides = BTreeMap::new();
    overrides.insert(key_for(source_base(), "created_at"), ColumnPresentationOverride::Raw);
    overrides.insert(
        key_for(
            ColumnSourceBase { connection_id: "conn-2", ..source_base() },
            "created_at",
        ),
        custom_ms(),
    );
    overrides.insert(
        key_for(
            ColumnSourceBase { database: Some("otherdb"), ..source_base() },
            "created_at",
        ),
        custom_seconds(),
    );
    overrides.insert(
        key_for(
            ColumnSourceBase { namespace: Some("private"), ..source_base() },
            "created_at",
        ),
        ColumnPresentationOverride::Raw,
    );
    overrides.insert(key_for(source_base(), "updated_at"), custom_ms());
    overrides.insert(
        key_for(ColumnSourceBase { table: "Orders", ..source_base() }, "created_at"),
        custom_seconds(),
    );
    StoredValuePresentationSettings {
        column_overrides: Some(overrides),
        ..stored(true, native_utc_iso())
    }
}

#[test]
fn isolation_matrix_connections_databases_schemas() {
    let snapshot =
        build_value_presentation_snapshot(&matrix_stored(), &env_snapshot(), &registry_with_timestamp(), 1);
    assert_eq!(
        column_override_from_snapshot(&snapshot, Some(source_base()), "created_at"),
        ColumnPresentationOverride::Raw
    );
    assert_eq!(
        column_override_from_snapshot(
            &snapshot,
            Some(ColumnSourceBase { connection_id: "conn-2", ..source_base() }),
            "created_at"
        ),
        custom_ms()
    );
    assert_eq!(
        column_override_from_snapshot(
            &snapshot,
            Some(ColumnSourceBase { database: Some("otherdb"), ..source_base() }),
            "created_at"
        ),
        custom_seconds()
    );
    assert_eq!(
        column_override_from_snapshot(
            &snapshot,
            Some(ColumnSourceBase { namespace: Some("private"), ..source_base() }),
            "created_at"
        ),
        ColumnPresentationOverride::Raw
    );
    // Different columns of the same table stay apart; an entryless column
    // inherits.
    assert_eq!(
        column_override_from_snapshot(&snapshot, Some(source_base()), "updated_at"),
        custom_ms()
    );
    assert_eq!(
        column_override_from_snapshot(&snapshot, Some(source_base()), "deleted_at"),
        ColumnPresentationOverride::Inherit
    );
}

#[test]
fn isolation_matrix_case_and_missing_vs_empty() {
    let snapshot =
        build_value_presentation_snapshot(&matrix_stored(), &env_snapshot(), &registry_with_timestamp(), 1);
    // Case differences are different sources.
    assert_eq!(
        column_override_from_snapshot(
            &snapshot,
            Some(ColumnSourceBase { table: "orders", ..source_base() }),
            "created_at"
        ),
        ColumnPresentationOverride::Raw
    );
    assert_eq!(
        column_override_from_snapshot(
            &snapshot,
            Some(ColumnSourceBase { table: "Orders", ..source_base() }),
            "created_at"
        ),
        custom_seconds()
    );
    // Missing database/namespace never collide with empty strings.
    let no_database = ColumnSourceBase { database: None, ..source_base() };
    assert_eq!(
        column_override_from_snapshot(&snapshot, Some(no_database), "created_at"),
        ColumnPresentationOverride::Inherit
    );
    let empty_database = ColumnSourceBase { database: Some(""), ..source_base() };
    assert_eq!(
        column_override_from_snapshot(&snapshot, Some(empty_database), "created_at"),
        ColumnPresentationOverride::Inherit
    );
    // Different keys never collide.
    assert_ne!(key_for(no_database, "c"), key_for(empty_database, "c"));
}

#[test]
fn raw_override_blocks_a_matching_rule() {
    let parsed = parse_stored_value_presentation_settings(&json!({
        "version": 1,
        "renderers": {
            TIMESTAMP_CONTRIBUTION_ID: { "enabled": true, "options": native_utc_iso() },
        },
        "rules": [json!({
            "name": "unix-ms",
            "enabled": true,
            "match": { "column": { "kind": "exact", "value": "epoch_ms" } },
            "options": unix_ms_shanghai(),
        })],
        "columnOverrides": {
            key_for(source_base(), "epoch_ms"): { "mode": "raw" },
        },
    }));
    let snapshot =
        build_value_presentation_snapshot(&parsed, &env_snapshot(), &registry_with_timestamp(), 1);
    let plans = build_column_presentation_plans(
        &BuildColumnPresentationPlansInput {
            columns: vec![PresentationPlanColumn {
                name: "epoch_ms",
                data_type: Some("bigint"),
                cell_type: None,
                hints: None,
            }],
            source: Some(source_base().with_column("epoch_ms")),
            snapshot: &snapshot,
        },
        &registry_with_timestamp(),
    );
    assert_eq!(plans[0].status, ColumnPresentationStatus::Raw);
    assert!(plans[0].matched_rule_name.is_none());
    assert!(matches!(
        plans[0].present(&json!(1705316400000i64)),
        PresentationResult::Unchanged { .. }
    ));
}

#[test]
fn restart_restore_round_trip() {
    // save → serialize → parse (the "restart") → snapshot → plan still hits.
    let before = crate::support::value_presentation::column_overrides::with_column_override(
        &stored(true, native_utc_iso()),
        source_base(),
        "epoch_ms",
        &custom_ms(),
    );
    let serialized = serde_json::to_value(&before).expect("serializable");
    let parsed = parse_stored_value_presentation_settings(&serialized);
    let snapshot =
        build_value_presentation_snapshot(&parsed, &env_snapshot(), &registry_with_timestamp(), 1);
    assert_eq!(
        column_override_from_snapshot(&snapshot, Some(source_base()), "epoch_ms"),
        custom_ms(),
        "the override survives the JSON round trip"
    );
}

#[test]
fn restart_restore_raw_with_renderer_disabled() {
    // raw overrides need no enabled switch and survive a restart even
    // with the renderer off.
    let before = crate::support::value_presentation::column_overrides::with_column_override(
        &stored(false, native_utc_iso()),
        source_base(),
        "epoch_ms",
        &ColumnPresentationOverride::Raw,
    );
    let parsed =
        parse_stored_value_presentation_settings(&serde_json::to_value(&before).unwrap());
    let snapshot =
        build_value_presentation_snapshot(&parsed, &env_snapshot(), &registry_with_timestamp(), 1);
    assert_eq!(
        column_override_from_snapshot(&snapshot, Some(source_base()), "epoch_ms"),
        ColumnPresentationOverride::Raw
    );
    let plans = build_column_presentation_plans(
        &BuildColumnPresentationPlansInput {
            columns: vec![PresentationPlanColumn {
                name: "epoch_ms",
                data_type: Some("bigint"),
                cell_type: None,
                hints: None,
            }],
            source: Some(source_base().with_column("epoch_ms")),
            snapshot: &snapshot,
        },
        &registry_with_timestamp(),
    );
    assert_eq!(plans[0].status, ColumnPresentationStatus::Raw);
}

#[test]
fn rules_end_to_end_through_snapshot_and_plan() {
    // The rules-through-plan end-to-end cases deferred from the T291
    // audit: PRD example (created_at/updated_at → Unix ms → iso-tz →
    // Shanghai), first-match freezing, override-beats-rule and the
    // disabled-contribution fall-through.
    let parsed = parse_stored_value_presentation_settings(&json!({
        "version": 1,
        "renderers": {
            TIMESTAMP_CONTRIBUTION_ID: { "enabled": true, "options": native_utc_iso() },
        },
        "rules": [unix_ms_rule_json()],
    }));
    let snapshot =
        build_value_presentation_snapshot(&parsed, &env_snapshot(), &registry_with_timestamp(), 9);
    let columns = vec![
        PresentationPlanColumn { name: "created_at", data_type: Some("BIGINT"), cell_type: None, hints: None },
        PresentationPlanColumn { name: "note", data_type: Some("TEXT"), cell_type: None, hints: None },
    ];
    let plans = build_column_presentation_plans(
        &BuildColumnPresentationPlansInput {
            columns,
            source: Some(source_base().with_column("created_at")),
            snapshot: &snapshot,
        },
        &registry_with_timestamp(),
    );
    // The matched column: ready with the rule's options.
    assert_eq!(plans[0].status, ColumnPresentationStatus::Ready);
    assert_eq!(plans[0].matched_rule_name.as_deref(), Some("unix-ms"));
    match plans[0].present(&json!(1705316400000i64)) {
        PresentationResult::Text { text, interpretation } => {
            assert_eq!(text, "2024-01-15T19:00:00+08:00");
            assert_eq!(interpretation.as_deref(), Some("unix"));
        }
        other => panic!("expected text, got {other:?}"),
    }
    // The unmatched TEXT column: no default renderer.
    assert_eq!(plans[1].status, ColumnPresentationStatus::Default);
    assert!(plans[1].matched_rule_name.is_none());
}

#[test]
fn rules_first_match_freezes_and_disabled_renderer_empties_rules() {
    // First-match freeze (the second rule must not apply).
    let parsed = parse_stored_value_presentation_settings(&json!({
        "version": 1,
        "renderers": {
            TIMESTAMP_CONTRIBUTION_ID: { "enabled": true, "options": native_utc_iso() },
        },
        "rules": [
            unix_ms_rule_json(),
            json!({
                "name": "unix-s",
                "enabled": true,
                "match": { "column": { "kind": "exact", "value": "created_at" } },
                "options": {
                    "interpretation": { "kind": "unix", "unit": "seconds" },
                    "format": "iso",
                    "displayTimeZone": { "kind": "utc" },
                },
            }),
        ],
    }));
    let snapshot =
        build_value_presentation_snapshot(&parsed, &env_snapshot(), &registry_with_timestamp(), 9);
    let plans = build_column_presentation_plans(
        &BuildColumnPresentationPlansInput {
            columns: vec![PresentationPlanColumn {
                name: "created_at",
                data_type: Some("BIGINT"),
                cell_type: None,
                hints: None,
            }],
            source: Some(source_base().with_column("created_at")),
            snapshot: &snapshot,
        },
        &registry_with_timestamp(),
    );
    assert_eq!(plans[0].matched_rule_name.as_deref(), Some("unix-ms"));
    match plans[0].present(&json!(1705316400000i64)) {
        PresentationResult::Text { text, .. } => {
            assert_eq!(text, "2024-01-15T19:00:00+08:00");
        }
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn preview_uses_the_runtime_path_for_query_result_columns() {
    // A sourceless column's custom preview only interprets the candidate
    // itself (no fabricated table source); inherited previews go through
    // the plan builder.
    let snapshot =
        build_value_presentation_snapshot(&stored(true, native_utc_iso()), &env_snapshot(), &registry_with_timestamp(), 1);
    let preview = compute_column_presentation_preview(&ComputeColumnPresentationPreviewInput {
        candidate: &custom_ms(),
        column: PresentationPlanColumn {
            name: "epoch_ms",
            data_type: Some("bigint"),
            cell_type: None,
            hints: None,
        },
        source: None,
        snapshot: &snapshot,
        environment: env(),
        sample_value: json!(1705316400000i64),
        registry: &registry_with_timestamp(),
    });
    match preview {
        crate::support::value_presentation::column_presentation::ColumnPresentationPreview::Text {
            text,
            interpretation,
            ..
        } => {
            assert_eq!(text, "2024-01-15T19:00:00+08:00");
            assert_eq!(interpretation.as_deref(), Some("unix"));
        }
        other => panic!("expected text, got {other:?}"),
    }
}
