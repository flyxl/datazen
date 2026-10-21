//! The on-disk shape of `AppSettings`: field names as camelCase, and what a
//! settings file written by an older build deserializes into today.
//!
//! Every assertion here is about the *file format*, not about any one setting's
//! behaviour — which is why a missing key has to fall back to the same value
//! `AppSettings::default()` reports.

use super::super::*;
use settings::{deserialize_theme, ThemePreference};

#[test]
fn theme_deserializes_legacy_string_and_object() {
    #[derive(Deserialize)]
    struct ThemeField {
        #[serde(deserialize_with = "deserialize_theme", default)]
        theme: ThemePreference,
    }

    let legacy: ThemeField = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
    assert_eq!(
        legacy.theme,
        ThemePreference {
            mode: "dark".into(),
            pack_id: None,
        }
    );

    let nested: ThemeField =
        serde_json::from_str(r#"{"theme":{"mode":"dark","packId":null}}"#).unwrap();
    assert_eq!(
        nested.theme,
        ThemePreference {
            mode: "dark".into(),
            pack_id: None,
        }
    );
}

#[test]
fn default_language_is_english() {
    assert_eq!(AppSettings::default().language, "en");
    assert_eq!(AppSettings::default().connection_pool_size, 10);
    assert!(!AppSettings::default().auto_chart_on_query);
}

#[test]
fn missing_auto_chart_on_query_defaults_to_false() {
    let mut value = serde_json::to_value(AppSettings::default()).unwrap();
    value.as_object_mut().unwrap().remove("autoChartOnQuery");
    let parsed: AppSettings = serde_json::from_value(value).unwrap();
    assert!(!parsed.auto_chart_on_query);
}

#[test]
fn settings_json_roundtrip_preserves_language() {
    let settings = AppSettings {
        language: "de".into(),
        ..AppSettings::default()
    };
    let json = serde_json::to_string(&settings).unwrap();
    let parsed: AppSettings = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.language, "de");
}

#[test]
fn monitor_settings_json_roundtrip() {
    use datazen_feature_core::dashboard::types::MonitorSettings;

    let settings = AppSettings {
        monitor: MonitorSettings {
            max_concurrent_queries: 4,
            run_retention_count: 100,
            tray_enabled: false,
            ..MonitorSettings::default()
        },
        ..AppSettings::default()
    };
    let json = serde_json::to_string(&settings).unwrap();
    let parsed: AppSettings = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.monitor.max_concurrent_queries, 4);
    assert_eq!(parsed.monitor.run_retention_count, 100);
    assert!(!parsed.monitor.tray_enabled);
    assert!(parsed.monitor.close_to_tray);
}

#[test]
fn theme_deserializes_light_and_system_strings() {
    #[derive(Deserialize)]
    struct ThemeField {
        #[serde(deserialize_with = "deserialize_theme", default)]
        theme: ThemePreference,
    }

    for mode in ["light", "system"] {
        let legacy: ThemeField = serde_json::from_str(&format!(r#"{{"theme":"{mode}"}}"#)).unwrap();
        assert_eq!(legacy.theme.mode, mode);
        assert!(legacy.theme.pack_id.is_none());
    }
}
