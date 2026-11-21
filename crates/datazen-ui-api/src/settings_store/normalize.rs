use serde_json::{json, Value};

use super::{
    DEFAULT_CONNECTION_POOL_SIZE, DEFAULT_EDITOR_FONT_SIZE, DEFAULT_LANGUAGE, DEFAULT_LOG_LEVEL,
    DEFAULT_PAGE_SIZE, DEFAULT_QUERY_RESULT_LIMIT, VALUE_PRESENTATION_VERSION,
};

fn normalize_locale_tag(locale: &str) -> String {
    locale.trim().replace('_', "-")
}

fn builtin_language(normalized: &str) -> Option<&'static str> {
    let lower = normalized.to_lowercase();
    if lower == "en" {
        return Some("en");
    }
    if lower == "zh-cn" {
        return Some("zh-CN");
    }
    if lower.starts_with("zh") {
        return Some("zh-CN");
    }
    if lower.starts_with("en") {
        return Some("en");
    }
    None
}

pub fn normalize_settings_language(language: &str) -> String {
    let normalized = normalize_locale_tag(language);
    if normalized.is_empty() {
        return DEFAULT_LANGUAGE.to_owned();
    }
    builtin_language(&normalized)
        .unwrap_or(&normalized)
        .to_owned()
}

pub fn resolve_ui_language(system_locale: &str) -> String {
    normalize_settings_language(system_locale)
}

fn normalize_theme(raw: Option<&Value>) -> Value {
    match raw {
        Some(Value::String(mode)) if matches!(mode.as_str(), "light" | "dark" | "system") => {
            json!({ "mode": mode, "packId": null })
        }
        Some(Value::Object(object)) => {
            let mode = object.get("mode").and_then(Value::as_str);
            let mode = match mode {
                Some(mode) if matches!(mode, "light" | "dark" | "system") => mode,
                _ => "dark",
            };
            let pack_id = object
                .get("packId")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty());
            json!({ "mode": mode, "packId": pack_id })
        }
        _ => json!({ "mode": "dark", "packId": null }),
    }
}

pub fn parse_stored_value_presentation(raw: Option<&Value>) -> Value {
    match raw {
        Some(Value::Object(object)) => {
            let mut result = object.clone();
            if !result.contains_key("version") {
                result.insert("version".to_owned(), json!(VALUE_PRESENTATION_VERSION));
            }
            if !result.contains_key("renderers") {
                result.insert("renderers".to_owned(), json!({}));
            }
            Value::Object(result)
        }
        _ => json!({ "version": VALUE_PRESENTATION_VERSION, "renderers": {} }),
    }
}

pub fn normalize_loaded_settings(raw: &Value) -> Value {
    let mut result = json!({
        "theme": { "mode": "dark", "packId": null },
        "language": DEFAULT_LANGUAGE,
        "limitSelectResults": false,
        "queryResultLimit": DEFAULT_QUERY_RESULT_LIMIT,
        "editorFontSize": DEFAULT_EDITOR_FONT_SIZE,
        "confirmOnDelete": true,
        "autoCommit": true,
        "safeMode": true,
        "confirmDangerousExecution": true,
        "defaultPageSize": DEFAULT_PAGE_SIZE,
        "connectionPoolSize": DEFAULT_CONNECTION_POOL_SIZE,
        "logLevel": DEFAULT_LOG_LEVEL,
        "logPath": "",
        "checkForUpdatesOnStartup": false,
        "autoChartOnQuery": false,
        "mcpPermissionMode": "safe_write",
        "mcpDisabledTools": [],
        "monitor": {}
    });
    if let Value::Object(overrides) = raw {
        for (key, value) in overrides {
            result[key.clone()] = value.clone();
        }
    }
    let language = raw
        .get("language")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_LANGUAGE);
    result["language"] = json!(normalize_settings_language(language));
    result["theme"] = normalize_theme(raw.get("theme"));
    result["connectionPoolSize"] = json!(raw
        .get("connectionPoolSize")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_CONNECTION_POOL_SIZE));
    let monitor = json!({});
    let merged_monitor = match raw.get("monitor") {
        Some(Value::Object(object)) => {
            let mut merged = monitor.as_object().cloned().unwrap_or_default();
            for (key, value) in object {
                merged.insert(key.clone(), value.clone());
            }
            Value::Object(merged)
        }
        _ => monitor,
    };
    result["monitor"] = merged_monitor;
    result["valuePresentation"] = parse_stored_value_presentation(raw.get("valuePresentation"));
    result
}

pub fn next_value_presentation_revision(prev: &Value, next: &Value, revision: u64) -> u64 {
    if prev.to_string() == next.to_string() {
        revision
    } else {
        revision + 1
    }
}

pub fn is_unknown_version_value_presentation(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Object(object)) => match object.get("version") {
            Some(Value::Number(number)) => number
                .as_u64()
                .is_some_and(|version| version != VALUE_PRESENTATION_VERSION),
            _ => false,
        },
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn language_normalization_canonicalizes_builtins_and_preserves_packs() {
        assert_eq!(normalize_settings_language(""), "en");
        assert_eq!(normalize_settings_language("en_US"), "en");
        assert_eq!(normalize_settings_language(" zh "), "zh-CN");
        assert_eq!(normalize_settings_language("zh_TW"), "zh-CN");
        assert_eq!(normalize_settings_language("ja-JP"), "ja-JP");
        assert_eq!(resolve_ui_language("de-DE"), "de-DE");
    }

    #[test]
    fn loaded_settings_fill_defaults_and_normalize_special_fields() {
        let settings = normalize_loaded_settings(&json!({
            "language": "zh",
            "theme": { "mode": "light", "packId": "" },
            "connectionPoolSize": 7,
            "monitor": { "refreshSec": 10 }
        }));
        assert_eq!(settings["language"], "zh-CN");
        assert_eq!(settings["theme"]["mode"], "light");
        assert!(settings["theme"]["packId"].is_null());
        assert_eq!(settings["connectionPoolSize"], 7);
        assert_eq!(settings["monitor"]["refreshSec"], 10);
        assert_eq!(settings["queryResultLimit"], 5000);
        assert_eq!(settings["valuePresentation"]["version"], 1);
    }

    #[test]
    fn value_presentation_parsing_is_tolerant() {
        assert_eq!(parse_stored_value_presentation(None)["version"], 1);
        assert_eq!(
            parse_stored_value_presentation(Some(&json!(null)))["renderers"],
            json!({})
        );
        let unknown = parse_stored_value_presentation(Some(&json!({"version": 2, "rules": [1]})));
        assert_eq!(unknown["version"], 2);
        assert_eq!(unknown["rules"], json!([1]));
    }

    #[test]
    fn revision_changes_only_when_json_changes() {
        let value = json!({"version": 1});
        assert_eq!(next_value_presentation_revision(&value, &value, 4), 4);
        assert_eq!(
            next_value_presentation_revision(&value, &json!({"version": 2}), 4),
            5
        );
    }

    #[test]
    fn unknown_version_requires_a_numeric_non_one_version() {
        assert!(is_unknown_version_value_presentation(Some(
            &json!({"version": 2})
        )));
        assert!(!is_unknown_version_value_presentation(Some(
            &json!({"version": 1})
        )));
        assert!(!is_unknown_version_value_presentation(Some(
            &json!({"version": "2"})
        )));
        assert!(!is_unknown_version_value_presentation(Some(&json!([]))));
        assert!(!is_unknown_version_value_presentation(None));
    }
}
