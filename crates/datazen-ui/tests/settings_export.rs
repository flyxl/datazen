//! T327: `lib/settingsExport.ts` port + `targetCommandPhase.ts` X-class ruling.
//!
//! Pinned values come from `settingsExport.test.ts`: the mock settings object,
//! the whitelist assertions, the invalid-prefix table and the two parse
//! failures. The phase ruling follows T045/T094: a type-only React shell gets
//! no fake Rune file — the equivalent already lives in `target_command.rs`.

use serde_json::{json, Map, Number, Value};

use datazen_ui::session::settings_export::{
    export_editor_settings, import_editor_settings, EditorSettings, SettingsImportError, SqlSnippet,
};
use datazen_ui::session::target_command::{
    ResponseKind, TargetApiError, TargetCommandEffect, TargetCommandInput, TargetRun,
};
use datazen_ui::session::editor_session_controller::{CommandCall, ExecutionTarget, NamespaceTarget};

fn editor_settings() -> EditorSettings {
    let mut custom_keymap = Map::new();
    custom_keymap.insert("editor.execute".to_owned(), Value::String("ctrl-enter".to_owned()));
    let mut sql_format_options = Map::new();
    sql_format_options.insert("keywordCase".to_owned(), Value::String("upper".to_owned()));
    EditorSettings {
        font_size: Some(Number::from(14)),
        font_family: Some("Fira Code".to_owned()),
        completion_quote_policy: Some("always".to_owned()),
        completion_include_table_prefix: None,
        keymap_preset: Some("dbeaver".to_owned()),
        custom_keymap: Some(custom_keymap),
        sql_format_options: Some(sql_format_options),
        sql_execution_strategy: Some("entire_script".to_owned()),
        sql_snippets: vec![SqlSnippet {
            id: "user-s1".to_owned(),
            prefix: "selc".to_owned(),
            description_key: Some("Select count".to_owned()),
            template: "SELECT COUNT(*) FROM #{1:table};".to_owned(),
        }],
    }
}

fn exported() -> String {
    export_editor_settings(&editor_settings(), "2026-11-20T00:00:00.000Z")
}

#[test]
fn exports_only_whitelisted_safe_editor_and_snippet_properties() {
    let json_str = exported();
    let parsed: Value = serde_json::from_str(&json_str).expect("valid exported JSON");
    assert_eq!(parsed["version"], json!(1));
    assert_eq!(parsed["exportedAt"], json!("2026-11-20T00:00:00.000Z"));
    assert_eq!(parsed["editor"]["fontSize"], json!(14));
    assert_eq!(parsed["editor"]["fontFamily"], json!("Fira Code"));
    assert_eq!(parsed["editor"]["sqlExecutionStrategy"], json!("entire_script"));
    assert_eq!(parsed["sqlSnippets"].as_array().map(Vec::len), Some(1));
    assert_eq!(parsed["sqlSnippets"][0]["prefix"], json!("selc"));
    for sensitive in [
        "theme",
        "mcpAllowedConnectionIds",
        "driverSettings",
        "wappSettings",
        "aiStrictEgress",
    ] {
        assert!(
            parsed.get(sensitive).is_none(),
            "sensitive setting {sensitive} must never be exported"
        );
    }
    assert!(
        json_str.contains("\"fontSize\": 14"),
        "pretty JSON uses a 2-space indent like JSON.stringify(null, 2)"
    );
}

#[test]
fn imports_a_valid_exported_document() {
    let patch = import_editor_settings(&exported()).expect("valid export imports");
    assert_eq!(patch.font_size, Some(Number::from(14)));
    assert_eq!(patch.font_family.as_deref(), Some("Fira Code"));
    assert_eq!(patch.sql_execution_strategy.as_deref(), Some("entire_script"));
    let snippets = patch.sql_snippets.expect("snippets imported");
    assert_eq!(snippets.len(), 1);
    assert_eq!(snippets[0].prefix, "selc");
}

#[test]
fn round_trips_the_table_prefix_preference() {
    for enabled in [true, false] {
        let mut settings = editor_settings();
        settings.completion_include_table_prefix = Some(enabled);
        let json_str = export_editor_settings(&settings, "2026-11-20T00:00:00.000Z");
        let parsed: Value = serde_json::from_str(&json_str).expect("valid JSON");
        assert_eq!(parsed["editor"]["completionIncludeTablePrefix"], json!(enabled));
        let patch = import_editor_settings(&json_str).expect("valid import");
        assert_eq!(patch.completion_include_table_prefix, Some(enabled));
    }
}

#[test]
fn importing_older_settings_leaves_the_prefix_preference_unset() {
    let patch = import_editor_settings(&exported()).expect("valid import");
    assert_eq!(
        patch.completion_include_table_prefix, None,
        "an absent field means 'do not change the current preference'"
    );
}

#[test]
fn ignores_invalid_table_prefix_preferences() {
    for value in [json!("false"), json!(0), Value::Null, json!({})] {
        let input = json!({ "editor": { "completionIncludeTablePrefix": value } }).to_string();
        let patch = import_editor_settings(&input).expect("the file itself is valid JSON");
        assert_eq!(
            patch.completion_include_table_prefix, None,
            "invalid prefix value {value} must be ignored"
        );
    }
}

#[test]
fn rejects_invalid_json_and_a_non_object_root() {
    assert_eq!(
        import_editor_settings("invalid json").err(),
        Some(SettingsImportError::InvalidJson)
    );
    assert_eq!(
        SettingsImportError::InvalidJson.message(),
        "Invalid JSON format"
    );
    assert_eq!(
        import_editor_settings("123").err(),
        Some(SettingsImportError::RootMustBeObject)
    );
    assert_eq!(
        SettingsImportError::RootMustBeObject.message(),
        "Root must be an object"
    );
    for falsy in ["null", "false", "0", "\"text\""] {
        assert_eq!(
            import_editor_settings(falsy).err(),
            Some(SettingsImportError::RootMustBeObject),
            "{falsy} is not an object root"
        );
    }
}

#[test]
fn react_accepts_an_array_root_and_returns_an_empty_patch() {
    let patch = import_editor_settings("[]").expect("React accepts an array root and returns an empty patch");
    assert_eq!(
        patch,
        datazen_ui::session::settings_export::EditorSettingsPatch::default(),
        "an array root has no named editor/snippet fields"
    );
}

#[test]
fn validates_enum_fields_and_filters_snippets() {
    let input = json!({
        "editor": {
            "completionQuotePolicy": "bogus",
            "keymapPreset": "emacs",
            "sqlExecutionStrategy": "drop_database",
            "customKeymap": { "a": "b" }
        },
        "sqlSnippets": [
            { "id": "ok", "prefix": "s", "template": "SELECT 1" },
            { "id": "bad", "prefix": "x" },
            7
        ]
    })
    .to_string();
    let patch = import_editor_settings(&input).expect("valid JSON");
    assert_eq!(patch.completion_quote_policy, None);
    assert_eq!(patch.keymap_preset, None);
    assert_eq!(patch.sql_execution_strategy, None);
    assert!(patch.custom_keymap.is_some(), "object fields are mapped as-is");
    let snippets = patch.sql_snippets.expect("snippets array present");
    assert_eq!(snippets.len(), 1, "only rows with id/prefix/template survive");
    assert_eq!(snippets[0].id, "ok");
    assert_eq!(
        snippets[0].description_key, None,
        "descriptionKey is optional per React's guard"
    );
}

// --- targetCommandPhase.ts ruling -------------------------------------------

#[test]
fn target_command_phase_has_no_rune_body_only_fixed_boundaries() {
    // React's targetCommandPhase.ts is a type-only re-export shell (X-class).
    // Rune has no file for it; TargetCommandStep.phases already carries the
    // fixed boundary strings. The probe below pins their full set: no request
    // data, idempotency key or error text ever enters a phase value.
    let target = ExecutionTarget {
        connection_id: "cfg-1".to_owned(),
        namespace: NamespaceTarget::default(),
        object: None,
    };
    let call = CommandCall {
        command: "explain".to_owned(),
        input: "token-1".to_owned(),
    };
    let (mut run, start) = TargetRun::new(target, call, ResponseKind::CommandResult);
    assert_eq!(start.phases, ["profiles"]);
    let token = run.step(TargetCommandInput::Token(datazen_ui::session::editor_session_controller::SubmissionToken {
        idempotency_key: "token-1".to_owned(),
        expires_at: 0,
    }));
    let dispose = run.step(TargetCommandInput::DisposeFailed(TargetApiError::new("Boom", "busy")));
    for step in [&start, &token, &dispose] {
        for phase in &step.phases {
            assert!(
                matches!(
                    *phase,
                    "profiles"
                        | "submission-token"
                        | "target-admission"
                        | "execution-view"
                        | "artifact-read"
                        | "dispose"
                ),
                "phase {phase:?} is a fixed boundary"
            );
            assert!(
                !phase.contains("token-1") && !phase.contains("busy") && !phase.contains("explain"),
                "phase {phase:?} leaked request or error content"
            );
        }
    }
    assert_eq!(
        dispose.effects, Vec::<TargetCommandEffect>::new(),
        "a dispose failure emits no new work"
    );
}
