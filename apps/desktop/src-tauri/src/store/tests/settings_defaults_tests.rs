//! The default value of each individual `AppSettings` flag, and what a settings
//! file that predates the flag must deserialize to.
//!
//! The pattern is the same in every test: drop the key from a serialized default,
//! parse it back, assert the same value as a fresh install. A flag introduced
//! without a serde default would silently become whatever `bool`/`String` zero-initializes
//! to, which is how features turn themselves on for every existing user.

use super::super::*;

#[test]
fn editor_completion_quote_policy_defaults_to_unquoted() {
    let mut value = serde_json::to_value(AppSettings::default()).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .remove("editorCompletionQuotePolicy");
    let parsed: AppSettings = serde_json::from_value(value).unwrap();
    assert_eq!(parsed.editor_completion_quote_policy, "unquoted");
}

#[test]
fn fk_prediction_defaults_to_off() {
    // A prediction is a guess, so it is opt-in: a fresh install must not present
    // inferred relationships with the same weight as a declared constraint.
    assert!(!AppSettings::default().enable_fk_prediction);
    assert!(!AppSettings::default_for_first_run().enable_fk_prediction);
}

#[test]
fn missing_enable_fk_prediction_defaults_to_off() {
    // Settings written before the feature existed have no key at all; they must
    // come back with prediction off rather than silently on.
    let mut value = serde_json::to_value(AppSettings::default()).unwrap();
    value.as_object_mut().unwrap().remove("enableFkPrediction");
    let parsed: AppSettings = serde_json::from_value(value).unwrap();
    assert!(!parsed.enable_fk_prediction);
}

#[test]
fn enable_fk_prediction_roundtrip() {
    let settings = AppSettings {
        enable_fk_prediction: true,
        ..AppSettings::default()
    };
    let json = serde_json::to_string(&settings).unwrap();
    assert!(json.contains("enableFkPrediction"));
    let parsed: AppSettings = serde_json::from_str(&json).unwrap();
    assert!(parsed.enable_fk_prediction);
}

#[test]
fn editor_completion_quote_policy_roundtrip() {
    let settings = AppSettings {
        editor_completion_quote_policy: "both".to_string(),
        ..AppSettings::default()
    };
    let json = serde_json::to_string(&settings).unwrap();
    assert!(json.contains("editorCompletionQuotePolicy"));
    let parsed: AppSettings = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.editor_completion_quote_policy, "both");
}

#[test]
fn first_run_language_is_supported() {
    let settings = AppSettings::default_for_first_run();
    const OK: &[&str] = &["en", "zh-CN"];
    assert!(
        OK.contains(&settings.language.as_str()),
        "unexpected {}",
        settings.language
    );
}

#[test]
fn sql_execution_strategy_defaults_to_current_statement() {
    assert_eq!(
        AppSettings::default().sql_execution_strategy,
        "current_statement"
    );
    assert_eq!(
        AppSettings::default_for_first_run().sql_execution_strategy,
        "current_statement"
    );
    let mut value = serde_json::to_value(AppSettings::default()).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .remove("sqlExecutionStrategy");
    let parsed: AppSettings = serde_json::from_value(value).unwrap();
    assert_eq!(parsed.sql_execution_strategy, "current_statement");
}

#[test]
fn sql_execution_strategy_preserves_explicit_saved_choices() {
    for strategy in [
        "entire_script",
        "current_statement",
        "largest_statement",
        "ask",
    ] {
        let settings = AppSettings {
            sql_execution_strategy: strategy.into(),
            ..AppSettings::default()
        };
        let parsed: AppSettings =
            serde_json::from_value(serde_json::to_value(settings).unwrap()).unwrap();
        assert_eq!(parsed.sql_execution_strategy, strategy);
    }
}
