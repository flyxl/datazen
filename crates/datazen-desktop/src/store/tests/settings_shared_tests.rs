use super::super::*;

#[test]
fn portable_snapshot_excludes_native_settings_and_credentials() {
    let mut local = AppSettings::default();
    local.context_dir = "/private/context".into();
    local.log_path = "/private/logs".into();
    let value = serde_json::to_value(local.shared_settings()).unwrap();
    assert!(value.get("logPath").is_none());
    // monitor 自 F4 起是共享行为字段（跨形态行为一致），必须出现在便携快照里；
    // 旧断言（is_none）属于 monitor 还是 host catch-all 的前一个世界观。
    assert!(value.get("monitor").is_some());
    assert!(!value.to_string().contains("/private"));
}

#[test]
fn portable_update_preserves_native_settings_and_new_preferences_roundtrip() {
    let mut local = AppSettings::default();
    local.context_dir = "/private/context".into();
    let mut shared = local.shared_settings();
    shared.preferences.sql_format_options.keyword_case = "lower".into();
    shared.preferences.workflow_step_result_order = "asc".into();
    shared.preferences.sql_snippets.push(settings::SqlSnippet {
        id: "one".into(),
        prefix: "sel".into(),
        description_key: "Select".into(),
        template: "SELECT 1".into(),
    });
    shared.execution.connection_pool_size = 200;
    local.apply_shared_settings(shared).unwrap();
    let roundtrip: AppSettings =
        serde_json::from_value(serde_json::to_value(&local).unwrap()).unwrap();
    assert_eq!(roundtrip.context_dir, "/private/context");
    assert_eq!(roundtrip.connection_pool_size, 100);
    assert_eq!(roundtrip.sql_format_options.keyword_case, "lower");
    assert_eq!(roundtrip.workflow_step_result_order, "asc");
    assert_eq!(roundtrip.sql_snippets.len(), 1);
}

#[test]
fn old_files_receive_format_snippet_and_workflow_defaults() {
    let mut value = serde_json::to_value(AppSettings::default()).unwrap();
    for key in ["sqlFormatOptions", "sqlSnippets", "workflowStepResultOrder"] {
        value.as_object_mut().unwrap().remove(key);
    }
    let parsed: AppSettings = serde_json::from_value(value).unwrap();
    assert_eq!(parsed.sql_format_options.keyword_case, "upper");
    assert_eq!(parsed.sql_format_options.lines_between_queries, 1);
    assert_eq!(parsed.workflow_step_result_order, "desc");
    assert!(parsed.sql_snippets.is_empty());
}

#[test]
fn unknown_fields_survive_a_legacy_settings_write() {
    let mut value = serde_json::to_value(AppSettings::default()).unwrap();
    value["futurePreference"] = serde_json::json!({ "enabled": true });
    let parsed: AppSettings = serde_json::from_value(value).unwrap();
    assert_eq!(
        serde_json::to_value(parsed).unwrap()["futurePreference"]["enabled"],
        true
    );
}

/// Shared `value_presentation` fixture: an enabled `datazen.timestamp`
/// renderer entry (the canonical entry shape — `{ enabled, options }` —
/// with realistic timestamp options; Rust stores the options opaquely).
fn value_presentation_with_timestamp() -> datazen::settings::ValuePresentationSettings {
    let mut renderers = serde_json::Map::new();
    renderers.insert(
        "datazen.timestamp".into(),
        serde_json::json!({
            "enabled": true,
            "options": {
                "interpretation": "instant",
                "format": "iso",
                "displayTimeZone": "UTC",
            },
        }),
    );
    datazen::settings::ValuePresentationSettings {
        version: 1,
        renderers,
        rules: vec![],
        column_overrides: serde_json::Map::new(),
    }
}

/// Issue-32 T06 desktop promotion (F4-style): `value_presentation` is a
/// first-class `AppSettings` field, so it round-trips through the shared
/// projection instead of being reported as the default.
#[test]
fn value_presentation_round_trips_through_shared_settings() {
    let mut local = AppSettings::default();
    local.value_presentation = value_presentation_with_timestamp();
    let shared = local.shared_settings();
    assert_eq!(
        shared.value_presentation, local.value_presentation,
        "shared_settings() must surface the host value, not the default"
    );
    assert_eq!(
        serde_json::to_value(&shared).unwrap()["valuePresentation"]["renderers"]
            ["datazen.timestamp"]["enabled"],
        true
    );

    let mut fresh = AppSettings::default();
    fresh.apply_shared_settings(shared).unwrap();
    assert_eq!(fresh.value_presentation, local.value_presentation);
}

/// Issue-32 T08: `value_presentation.rules` is an opaque JSON array on the
/// Rust side (the frontend validates rule entries); it must survive a
/// shared-settings round trip and a JSON round trip instead of being
/// dropped by the typed struct — otherwise the next load would lose the
/// rules the user just saved.
#[test]
fn value_presentation_rules_round_trip_through_shared_settings() {
    let mut local = AppSettings::default();
    local.value_presentation = datazen::settings::ValuePresentationSettings {
        version: 1,
        renderers: serde_json::Map::new(),
        rules: vec![serde_json::json!({
            "name": "unix-ms",
            "enabled": true,
            "match": {
                "column": { "kind": "regex", "source": "^(created_at|updated_at)$" },
                "databaseTypeName": { "kind": "exact", "value": "BIGINT" },
            },
            "options": {
                "interpretation": { "kind": "unix", "unit": "milliseconds" },
                "format": "iso-tz",
                "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
            },
        })],
        column_overrides: serde_json::Map::new(),
    };
    let shared = local.shared_settings();
    assert_eq!(shared.value_presentation, local.value_presentation);
    assert_eq!(
        serde_json::to_value(&shared).unwrap()["valuePresentation"]["rules"][0]["name"],
        "unix-ms"
    );

    let mut fresh = AppSettings::default();
    fresh.apply_shared_settings(shared).unwrap();
    assert_eq!(fresh.value_presentation, local.value_presentation);
    assert_eq!(fresh.value_presentation.rules.len(), 1);

    // JSON round trip keeps the rules too.
    let reparsed: AppSettings =
        serde_json::from_value(serde_json::to_value(&local).unwrap()).unwrap();
    assert_eq!(reparsed.value_presentation, local.value_presentation);
}

/// Issue-32 T10: `value_presentation.column_overrides` is an opaque JSON
/// object on the Rust side (the frontend validates keys and entries); it
/// must survive a shared-settings round trip and a JSON round trip
/// instead of being dropped by the typed struct — otherwise the next
/// load would lose the column overrides the user just saved.
#[test]
fn value_presentation_column_overrides_round_trip_through_shared_settings() {
    let mut column_overrides = serde_json::Map::new();
    column_overrides.insert(
        "[\"conn-1\",\"shop\",\"public\",\"orders\",\"epoch_ms\"]".into(),
        serde_json::json!({"mode": "raw"}),
    );
    let mut local = AppSettings::default();
    local.value_presentation = datazen::settings::ValuePresentationSettings {
        version: 1,
        renderers: serde_json::Map::new(),
        rules: vec![],
        column_overrides,
    };
    let shared = local.shared_settings();
    assert_eq!(shared.value_presentation, local.value_presentation);
    assert_eq!(
        serde_json::to_value(&shared).unwrap()["valuePresentation"]["columnOverrides"]
            ["[\"conn-1\",\"shop\",\"public\",\"orders\",\"epoch_ms\"]"]["mode"],
        "raw"
    );

    let mut fresh = AppSettings::default();
    fresh.apply_shared_settings(shared).unwrap();
    assert_eq!(fresh.value_presentation, local.value_presentation);
    assert_eq!(fresh.value_presentation.column_overrides.len(), 1);

    // JSON round trip keeps the overrides too.
    let reparsed: AppSettings =
        serde_json::from_value(serde_json::to_value(&local).unwrap()).unwrap();
    assert_eq!(reparsed.value_presentation, local.value_presentation);
}

/// Old settings files carry no `valuePresentation` key; they must load as
/// the default (version 1, no renderers), not fail.
#[test]
fn old_settings_file_without_value_presentation_defaults() {
    let mut value = serde_json::to_value(AppSettings::default()).unwrap();
    value.as_object_mut().unwrap().remove("valuePresentation");
    let parsed: AppSettings = serde_json::from_value(value).unwrap();
    assert_eq!(
        parsed.value_presentation,
        datazen::settings::ValuePresentationSettings::default()
    );
    assert_eq!(parsed.value_presentation.version, 1);
    assert!(parsed.value_presentation.renderers.is_empty());
    // T08: old files carry no rules either — they load as an empty list.
    assert!(parsed.value_presentation.rules.is_empty());
}

/// Pre-promotion builds stored the value opaquely in the `extra` catch-all,
/// so on disk it is a top-level `valuePresentation` key. After promotion
/// that key must land in the field (and leave `extra`), not stay opaque —
/// otherwise the next shared read would report the default and the next
/// save would overwrite the real value with it.
#[test]
fn value_presentation_promoted_out_of_extra_catch_all() {
    let mut value = serde_json::to_value(AppSettings::default()).unwrap();
    value["valuePresentation"] = serde_json::to_value(value_presentation_with_timestamp()).unwrap();
    let parsed: AppSettings = serde_json::from_value(value).unwrap();
    assert_eq!(
        parsed.value_presentation,
        value_presentation_with_timestamp()
    );
    assert!(parsed.extra.get("valuePresentation").is_none());
}
