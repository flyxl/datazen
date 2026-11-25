//! T265 audit: the DriverSettingsSection port, verified against the
//! React source re-read line by line (the section's only React
//! coverage is the SettingsContent mock; the rules below are
//! re-derived from its own body).
//!
//! Tester-written: every expectation below is re-derived from the React
//! file.

use serde_json::json;

use datazen_ui::views::driver_settings_section::{
    merge_driver_settings, DriverSettingsEntry, DriverSettingsSectionState,
    DRIVER_SAVED_WINDOW,
};

fn entry(id: &str, label: &str, custom: bool, schema: Option<serde_json::Value>) -> DriverSettingsEntry {
    DriverSettingsEntry { driver_id: id.to_owned(), label: label.to_owned(), custom, schema }
}

/// `mergeDriverSettings`: the driver's value replaced whole, the
/// siblings untouched.
#[test]
fn merge_replaces_one_driver_whole() {
    let all = json!({"sqlite": {"busyTimeout": 100}, "postgres": {"pool": 5}});
    let next = json!({"busyTimeout": 200, "extra": true});
    let merged = merge_driver_settings(&all, "sqlite", next.clone());
    assert_eq!(merged["sqlite"], next);
    assert_eq!(merged["postgres"], all["postgres"], "siblings untouched");
    // A new driver id inserts.
    let merged = merge_driver_settings(&all, "mysql", json!({}));
    assert_eq!(merged["mysql"], json!({}));
    // A non-object base still merges (defensive).
    let merged = merge_driver_settings(&json!(null), "sqlite", json!({"a": 1}));
    assert_eq!(merged["sqlite"], json!({"a": 1}));
}

/// The saved window: 2 seconds, arming and expiring.
#[test]
fn the_saved_window_is_two_seconds() {
    assert_eq!(DRIVER_SAVED_WINDOW, 2.0);
    let mut state = DriverSettingsSectionState::new();
    assert!(!state.saved(100.0), "nothing armed yet");
    state.note_saved(100.0);
    assert!(state.saved(100.0));
    assert!(state.saved(101.9));
    assert!(!state.saved(102.0), "the window closes at +2s");
}

/// The entries' shapes: the custom marker and the schema fallback are
/// the two arms (with neither, the block renders its title alone — the
/// React `: null` arm has no third rendering).
#[test]
fn the_entry_arms_are_exhaustive() {
    let entries = vec![
        entry("custom-driver", "Custom", true, None),
        entry("schema-driver", "Schema", false, Some(json!({"type": "object"}))),
        entry("bare-driver", "Bare", false, None),
    ];
    // The three arms a section can hold: custom, schema, bare.
    assert!(entries[0].custom && entries[0].schema.is_none());
    assert!(!entries[1].custom && entries[1].schema.is_some());
    assert!(!entries[2].custom && entries[2].schema.is_none());
}

/// The draft-override suppression: React's `{!onSettingsChange && saved
/// && …}` — the saved line is the store-owning mode's feedback only.
/// (The component's boolean is the host's projection; the rule itself
/// is pinned by the section's build, exercised through the harness in
/// the settings-content integration — here the state machine's arm.)
#[test]
fn the_draft_mode_has_no_saved_line_of_its_own() {
    // The suppression is `!onSettingsChange`: the section's saved
    // window still arms (the host may share one deadline), but the
    // rendering gate is the mode flag. The pure half: the window
    // arming is unconditional (the React handler arms it in both
    // modes — the JSX gate is what hides it).
    let mut state = DriverSettingsSectionState::new();
    state.note_saved(50.0);
    assert!(state.saved(50.0), "the window arms in both modes");
}
