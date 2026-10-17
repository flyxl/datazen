use super::super::*;

#[test]
fn portable_snapshot_excludes_native_settings_and_credentials() {
    let mut local = AppSettings::default();
    local.context_dir = "/private/context".into();
    local.log_path = "/private/logs".into();
    let value = serde_json::to_value(local.shared_settings()).unwrap();
    assert!(value.get("logPath").is_none());
    assert!(value.get("monitor").is_none());
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
