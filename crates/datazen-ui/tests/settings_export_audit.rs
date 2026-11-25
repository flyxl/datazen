//! Independent audit for T327: `settingsExport.ts` port.
//!
//! Fill branches the coder does not drive: the all-absent export, every valid
//! enum literal on import, and the defensive drop of unknown/sensitive fields.

use serde_json::{json, Value};

use datazen_ui::session::settings_export::{
    export_editor_settings, import_editor_settings, EditorSettings,
};

/// React always emits `editor` and `sqlSnippets` in the payload; with no
/// preferences set those are `{}` and `[]`, and no sensitive key appears.
#[test]
fn exporting_default_settings_emits_empty_editor_and_snippets() {
    let json_str = export_editor_settings(&EditorSettings::default(), "2026-11-20T00:00:00.000Z");
    let parsed: Value = serde_json::from_str(&json_str).expect("valid export JSON");
    assert_eq!(parsed["version"], json!(1));
    assert_eq!(parsed["editor"], json!({}));
    assert_eq!(parsed["sqlSnippets"], json!([]));
    for sensitive in ["theme", "language", "aiApiKey", "password", "connectionString"] {
        assert!(parsed.get(sensitive).is_none(), "{sensitive} leaked into export");
    }
}

/// React accepts exactly three quote policies, three keymap presets and four
/// execution strategies. The coder only exercised one value of each via the
/// exported mock; pin the full accepted sets here.
#[test]
fn import_accepts_every_valid_enum_literal() {
    for policy in ["unquoted", "always", "both"] {
        let input = json!({ "editor": { "completionQuotePolicy": policy } }).to_string();
        let patch = import_editor_settings(&input).expect("valid JSON");
        assert_eq!(
            patch.completion_quote_policy.as_deref(),
            Some(policy),
            "quote policy {policy} is accepted"
        );
    }
    for preset in ["default", "dbeaver", "navicat"] {
        let input = json!({ "editor": { "keymapPreset": preset } }).to_string();
        let patch = import_editor_settings(&input).expect("valid JSON");
        assert_eq!(
            patch.keymap_preset.as_deref(),
            Some(preset),
            "keymap preset {preset} is accepted"
        );
    }
    for strategy in [
        "current_statement",
        "entire_script",
        "largest_statement",
        "ask",
    ] {
        let input = json!({ "editor": { "sqlExecutionStrategy": strategy } }).to_string();
        let patch = import_editor_settings(&input).expect("valid JSON");
        assert_eq!(
            patch.sql_execution_strategy.as_deref(),
            Some(strategy),
            "execution strategy {strategy} is accepted"
        );
    }
}

/// Import maps only whitelisted fields; unrelated and credential-shaped keys
/// produce an empty patch rather than being carried into the host settings.
#[test]
fn import_drops_unknown_and_sensitive_fields() {
    let input = json!({
        "theme": { "mode": "dark" },
        "password": "secret",
        "aiApiKey": "sk-secret",
        "mcpAllowedConnectionIds": ["conn-1"],
        "editor": { "language": "en", "editorFontSize": 14 },
        "sqlSnippets": "not-an-array"
    })
    .to_string();
    let patch = import_editor_settings(&input).expect("valid JSON");
    assert_eq!(
        patch.font_size, None,
        "fontSize is only read from the editor object's own key"
    );
    assert_eq!(patch.font_family, None);
    assert_eq!(patch.completion_quote_policy, None);
    assert_eq!(patch.keymap_preset, None);
    assert_eq!(patch.custom_keymap, None);
    assert_eq!(patch.sql_format_options, None);
    assert_eq!(patch.sql_execution_strategy, None);
    assert_eq!(patch.sql_snippets, None, "a non-array sqlSnippets is ignored");
}
