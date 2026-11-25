//! Independent audit for T037 (the columnOverrides port).
//!
//! Every assertion comes from re-reading
//! `packages/frontend/src/lib/valuePresentation/columnOverrides.ts` and
//! its React test `__tests__/columnOverrides.test.ts` — key-shape and
//! value validation (malformed keys / invalid overrides / inherit
//! entries drop; the 200-entry cap), withColumnOverride's pure semantics
//! (write / delete / no-change / unknown-version / cap-refusal with
//! replace-allowed), and the save road (persist + session clear on
//! success, session retention on failure, the refusal matrix, the
//! no-change save, and the T11 updater applying to a fresh authoritative
//! value).
//!
//! The React file's final two describe blocks (the isolation matrix and
//! the override-beats-rule / restart-restore simulations) go through
//! `buildValuePresentationSnapshot` (settings.ts, T182) and move to that
//! task's audit. Elisions: React's reference-identity checks (`toBe`)
//! become structural equality — the pinned behavior is which value comes
//! back, and the no-change paths return the input's content verbatim.

use std::collections::BTreeMap;

use serde_json::json;

use datazen_ui::support::value_presentation::column_overrides::{
    MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES, SaveColumnOverrideInput, SaveColumnOverrideReason,
    validate_stored_column_overrides, with_column_override,
};
use datazen_ui::support::value_presentation::column_presentation::ColumnSourceBase;
use datazen_ui::support::value_presentation::column_presentation_session::ColumnPresentationSession;
use datazen_ui::support::value_presentation::contracts::ColumnPresentationOverride;
use datazen_ui::support::value_presentation::presentation_plan::column_override_key;
use datazen_ui::support::value_presentation::settings::{
    StoredRendererSettings, StoredValuePresentationSettings,
};
use datazen_ui::support::value_presentation::value_presentation_save::{
    ValuePresentationSettingsUpdate, apply_value_presentation_update,
};

fn source_base() -> ColumnSourceBase<'static> {
    ColumnSourceBase {
        connection_id: "conn-1",
        database: Some("shop"),
        namespace: Some("public"),
        table: "orders",
    }
}

fn native_utc_iso() -> serde_json::Value {
    json!({
        "interpretation": { "kind": "native" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
    })
}

fn custom_ms() -> ColumnPresentationOverride {
    ColumnPresentationOverride::Custom {
        renderer_id: "datazen.timestamp".to_string(),
        options: json!({
            "interpretation": { "kind": "unix", "unit": "milliseconds" },
            "format": "iso-tz",
            "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
        }),
    }
}

fn custom_seconds() -> ColumnPresentationOverride {
    ColumnPresentationOverride::Custom {
        renderer_id: "datazen.timestamp".to_string(),
        options: json!({
            "interpretation": { "kind": "unix", "unit": "seconds" },
            "format": "iso",
            "displayTimeZone": { "kind": "utc" },
        }),
    }
}

fn key_for(column: &str) -> String {
    column_override_key(&source_base().with_column(column))
}

fn stored_with_overrides(
    overrides: Vec<(String, ColumnPresentationOverride)>,
) -> StoredValuePresentationSettings {
    let mut renderers = BTreeMap::new();
    renderers.insert(
        "datazen.timestamp".to_string(),
        StoredRendererSettings { enabled: true, options: native_utc_iso() },
    );
    StoredValuePresentationSettings {
        version: 1,
        renderers,
        rules: None,
        column_overrides: Some(overrides.into_iter().collect()),
        extra: serde_json::Map::new(),
    }
}

fn stored_json(stored: &StoredValuePresentationSettings) -> serde_json::Value {
    serde_json::to_value(stored).expect("serializable")
}

// ---------------------------------------------------------------------------
// validateStoredColumnOverrides
// ---------------------------------------------------------------------------

#[test]
fn stored_parse_keeps_valid_raw_and_custom_entries() {
    let input = json!({
        key_for("epoch_ms"): { "mode": "raw" },
        key_for("created_at"): custom_ms(),
    });
    let mut expected = BTreeMap::new();
    expected.insert(key_for("epoch_ms"), ColumnPresentationOverride::Raw);
    expected.insert(key_for("created_at"), custom_ms());
    assert_eq!(validate_stored_column_overrides(&input), expected);
}

#[test]
fn stored_parse_returns_empty_for_non_objects() {
    for bad in [json!(null), json!("x"), json!(42), json!(true), json!([]), json!(["a"])] {
        assert!(
            validate_stored_column_overrides(&bad).is_empty(),
            "input {bad}"
        );
    }
}

#[test]
fn stored_parse_drops_malformed_keys() {
    let input = json!({
        "not-a-key": { "mode": "raw" },
        r#"["conn-1"]"#: { "mode": "raw" },
        r#"["conn-1","shop","public","orders"]"#: { "mode": "raw" },
        r#"[1,"shop","public","orders","c"]"#: { "mode": "raw" },
        r#"[null,"shop","public","orders","c"]"#: { "mode": "raw" },
        key_for("epoch_ms"): { "mode": "raw" },
    });
    let parsed = validate_stored_column_overrides(&input);
    assert_eq!(parsed.len(), 1);
    assert_eq!(
        parsed.get(&key_for("epoch_ms")),
        Some(&ColumnPresentationOverride::Raw)
    );
}

#[test]
fn stored_parse_drops_invalid_overrides_and_inherit() {
    let input = json!({
        key_for("a"): { "mode": "inherit" },
        key_for("b"): { "mode": "bogus" },
        key_for("c"): { "mode": "custom", "rendererId": "acme.other", "options": {} },
        key_for("d"): "not-an-object",
        key_for("e"): { "mode": "raw" },
    });
    let parsed = validate_stored_column_overrides(&input);
    assert_eq!(parsed.len(), 1);
    assert_eq!(
        parsed.get(&key_for("e")),
        Some(&ColumnPresentationOverride::Raw)
    );
}

#[test]
fn stored_parse_caps_at_max_entries() {
    assert_eq!(MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES, 200);
    let mut input = serde_json::Map::new();
    for i in 0..(MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES + 1) {
        input.insert(
            format!(r#"["conn-{i}","db","ns","t","c"]"#),
            json!({ "mode": "raw" }),
        );
    }
    let parsed = validate_stored_column_overrides(&serde_json::Value::Object(input));
    assert_eq!(parsed.len(), MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES);
}

// ---------------------------------------------------------------------------
// withColumnOverride
// ---------------------------------------------------------------------------

#[test]
fn with_override_adds_raw_and_preserves_other_fields() {
    let stored = StoredValuePresentationSettings {
        version: 1,
        renderers: {
            let mut map = BTreeMap::new();
            map.insert(
                "datazen.timestamp".to_string(),
                StoredRendererSettings { enabled: true, options: native_utc_iso() },
            );
            map
        },
        rules: Some(Vec::new()),
        column_overrides: None,
        extra: serde_json::Map::new(),
    };
    let next = with_column_override(&stored, source_base(), "epoch_ms", &ColumnPresentationOverride::Raw);
    assert_eq!(
        next.column_overrides.as_ref().and_then(|o| o.get(&key_for("epoch_ms"))),
        Some(&ColumnPresentationOverride::Raw)
    );
    // Renderers and rules pass through untouched.
    assert_eq!(next.renderers, stored.renderers);
    assert_eq!(next.rules, stored.rules);
    // The input is not mutated.
    assert!(stored.column_overrides.is_none());
}

#[test]
fn with_override_adds_custom_by_full_source_identity() {
    let stored = StoredValuePresentationSettings {
        version: 1,
        renderers: BTreeMap::new(),
        rules: None,
        column_overrides: None,
        extra: serde_json::Map::new(),
    };
    let next = with_column_override(&stored, source_base(), "created_at", &custom_ms());
    assert_eq!(
        next.column_overrides.and_then(|o| o.get(&key_for("created_at")).cloned()),
        Some(custom_ms())
    );
}

#[test]
fn with_override_deletes_for_inherit_and_noops_without_entry() {
    let stored =
        stored_with_overrides(vec![(key_for("epoch_ms"), ColumnPresentationOverride::Raw)]);
    let next = with_column_override(
        &stored,
        source_base(),
        "epoch_ms",
        &ColumnPresentationOverride::Inherit,
    );
    assert_eq!(next.column_overrides, Some(BTreeMap::new()));
    // Deleting a missing entry returns the input verbatim.
    let no_change =
        with_column_override(&stored, source_base(), "other", &ColumnPresentationOverride::Inherit);
    assert_eq!(stored_json(&no_change), stored_json(&stored));
}

#[test]
fn with_override_returns_input_for_unknown_version() {
    let stored = StoredValuePresentationSettings {
        version: 99,
        renderers: BTreeMap::new(),
        rules: None,
        column_overrides: None,
        extra: serde_json::Map::new(),
    };
    let next = with_column_override(&stored, source_base(), "epoch_ms", &ColumnPresentationOverride::Raw);
    assert_eq!(stored_json(&next), stored_json(&stored));
}

#[test]
fn with_override_returns_input_when_unchanged() {
    let stored =
        stored_with_overrides(vec![(key_for("epoch_ms"), ColumnPresentationOverride::Raw)]);
    let next =
        with_column_override(&stored, source_base(), "epoch_ms", &ColumnPresentationOverride::Raw);
    assert_eq!(stored_json(&next), stored_json(&stored));
}

#[test]
fn with_override_returns_input_for_invalid_override() {
    let stored = StoredValuePresentationSettings {
        version: 1,
        renderers: BTreeMap::new(),
        rules: None,
        column_overrides: None,
        extra: serde_json::Map::new(),
    };
    // An invalid custom (illegal options — unix without a unit) is the
    // representable invalid-override form; bogus modes are rejected at
    // the serde boundary itself (pinned below).
    let invalid_custom = ColumnPresentationOverride::Custom {
        renderer_id: "datazen.timestamp".to_string(),
        options: json!({
            "interpretation": { "kind": "unix" },
            "format": "iso-tz",
            "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
        }),
    };
    let next = with_column_override(&stored, source_base(), "epoch_ms", &invalid_custom);
    assert_eq!(stored_json(&next), stored_json(&stored));
    // The bogus-mode guard is the serde boundary (type-level in Rust).
    assert!(
        serde_json::from_value::<ColumnPresentationOverride>(json!({ "mode": "bogus" })).is_err()
    );
}

#[test]
fn with_override_refuses_new_past_cap_but_allows_replacement() {
    let mut overrides = BTreeMap::new();
    for i in 0..MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES {
        overrides.insert(
            format!(r#"["conn-{i}","db","ns","t","c"]"#),
            ColumnPresentationOverride::Raw,
        );
    }
    let stored = StoredValuePresentationSettings {
        version: 1,
        renderers: BTreeMap::new(),
        rules: None,
        column_overrides: Some(overrides),
        extra: serde_json::Map::new(),
    };
    // A new key past the cap is refused.
    let new_source = ColumnSourceBase {
        connection_id: "conn-new",
        database: None,
        namespace: None,
        table: "t",
    };
    let refused = with_column_override(&stored, new_source, "c", &ColumnPresentationOverride::Raw);
    assert_eq!(stored_json(&refused), stored_json(&stored));
    // Replacing an existing entry is still allowed.
    let existing_source = ColumnSourceBase {
        connection_id: "conn-0",
        database: Some("db"),
        namespace: Some("ns"),
        table: "t",
    };
    let replaced = with_column_override(&stored, existing_source, "c", &custom_ms());
    assert_eq!(
        replaced
            .column_overrides
            .and_then(|o| o.get(r#"["conn-0","db","ns","t","c"]"#).cloned()),
        Some(custom_ms())
    );
}

// ---------------------------------------------------------------------------
// saveColumnOverride
// ---------------------------------------------------------------------------

/// Run one save; returns (result, persist-called, captured-update-applied
/// to `fresh`, session-state-after).
struct SaveOutcome {
    result: Result<(), SaveColumnOverrideReason>,
    persist_called: bool,
    persisted_next: Option<StoredValuePresentationSettings>,
}

fn run_save(
    stored: &StoredValuePresentationSettings,
    source: Option<ColumnSourceBase<'_>>,
    column_name: &str,
    override_value: &ColumnPresentationOverride,
    persist_fails: bool,
    fresh: StoredValuePresentationSettings,
) -> (SaveOutcome, ColumnPresentationSession) {
    let mut session = ColumnPresentationSession::new();
    let mut persist_called = false;
    let mut persisted_next: Option<StoredValuePresentationSettings> = None;
    let result = {
        let mut persist = |update: ValuePresentationSettingsUpdate<'_>| {
            persist_called = true;
            persisted_next = Some(apply_value_presentation_update(fresh.clone(), update));
            if persist_fails {
                Err(())
            } else {
                Ok(())
            }
        };
        let mut input = SaveColumnOverrideInput {
            stored,
            source,
            column_name,
            override_value,
            session: &mut session,
            persist: &mut persist,
        };
        datazen_ui::support::value_presentation::column_overrides::save_column_override(&mut input)
    };
    let outcome = SaveOutcome { result, persist_called, persisted_next };
    // The closure borrowed session; return the session by re-reading.
    // (session was mutated through the input borrow.)
    let _ = &outcome;
    (outcome, session)
}

#[test]
fn save_persists_and_clears_the_session_entry() {
    let stored = StoredValuePresentationSettings {
        version: 1,
        renderers: BTreeMap::new(),
        rules: None,
        column_overrides: None,
        extra: serde_json::Map::new(),
    };
    let fresh = stored.clone();
    let (outcome, session) = run_save(
        &stored,
        Some(source_base()),
        "epoch_ms",
        &ColumnPresentationOverride::Raw,
        false,
        fresh,
    );
    assert_eq!(outcome.result, Ok(()));
    assert!(outcome.persist_called);
    // The persisted update applies this column's override to fresh.
    let next = outcome.persisted_next.expect("persist ran");
    let mut expected = BTreeMap::new();
    expected.insert(key_for("epoch_ms"), ColumnPresentationOverride::Raw);
    assert_eq!(next.column_overrides, Some(expected));
    // The session entry is cleared after success.
    assert!(session.state().overrides.is_empty());
}

#[test]
fn save_failure_keeps_the_session_entry() {
    let stored = StoredValuePresentationSettings {
        version: 1,
        renderers: BTreeMap::new(),
        rules: None,
        column_overrides: None,
        extra: serde_json::Map::new(),
    };
    let (outcome, session) = run_save(
        &stored,
        Some(source_base()),
        "epoch_ms",
        &custom_ms(),
        true,
        stored.clone(),
    );
    assert_eq!(outcome.result, Err(SaveColumnOverrideReason::SaveFailed));
    assert!(outcome.persist_called);
    // The session entry survives: the user's fresh choice is not lost.
    assert_eq!(
        session.state().overrides.get(&key_for("epoch_ms")),
        Some(&custom_ms())
    );
}

#[test]
fn save_inherit_deletes_the_entry_and_clears_the_session() {
    let stored = stored_with_overrides(vec![
        (key_for("epoch_ms"), custom_ms()),
        (key_for("other"), ColumnPresentationOverride::Raw),
    ]);
    let (outcome, session) = run_save(
        &stored,
        Some(source_base()),
        "epoch_ms",
        &ColumnPresentationOverride::Inherit,
        false,
        stored.clone(),
    );
    assert_eq!(outcome.result, Ok(()));
    let next = outcome.persisted_next.expect("persist ran");
    let mut expected = BTreeMap::new();
    expected.insert(key_for("other"), ColumnPresentationOverride::Raw);
    assert_eq!(next.column_overrides, Some(expected));
    assert!(session.state().overrides.is_empty());
}

#[test]
fn save_refusal_matrix_never_touches_persistence_or_session() {
    // no-source
    let stored = StoredValuePresentationSettings {
        version: 1,
        renderers: BTreeMap::new(),
        rules: None,
        column_overrides: None,
        extra: serde_json::Map::new(),
    };
    let (outcome, session) = run_save(
        &stored,
        None,
        "epoch_ms",
        &ColumnPresentationOverride::Raw,
        false,
        stored.clone(),
    );
    assert_eq!(outcome.result, Err(SaveColumnOverrideReason::NoSource));
    assert!(!outcome.persist_called);
    assert!(session.state().overrides.is_empty());

    // invalid override (illegal custom options; bogus modes fail at the
    // serde boundary)
    let invalid_custom = ColumnPresentationOverride::Custom {
        renderer_id: "datazen.timestamp".to_string(),
        options: json!({
            "interpretation": { "kind": "unix" },
            "format": "iso-tz",
            "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
        }),
    };
    let (outcome, session) = run_save(
        &stored,
        Some(source_base()),
        "epoch_ms",
        &invalid_custom,
        false,
        stored.clone(),
    );
    assert_eq!(outcome.result, Err(SaveColumnOverrideReason::InvalidOverride));
    assert!(!outcome.persist_called);
    assert!(session.state().overrides.is_empty());

    // unknown version
    let v99 = StoredValuePresentationSettings { version: 99, ..stored.clone() };
    let (outcome, session) = run_save(
        &v99,
        Some(source_base()),
        "epoch_ms",
        &ColumnPresentationOverride::Raw,
        false,
        v99.clone(),
    );
    assert_eq!(outcome.result, Err(SaveColumnOverrideReason::UnknownVersion));
    assert!(!outcome.persist_called);
    assert!(session.state().overrides.is_empty());
}

#[test]
fn save_refuses_new_entries_past_the_cap() {
    let mut overrides = BTreeMap::new();
    for i in 0..MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES {
        overrides.insert(
            format!(r#"["conn-{i}","db","ns","t","c"]"#),
            ColumnPresentationOverride::Raw,
        );
    }
    let stored = StoredValuePresentationSettings {
        version: 1,
        renderers: BTreeMap::new(),
        rules: None,
        column_overrides: Some(overrides),
        extra: serde_json::Map::new(),
    };
    let new_source = ColumnSourceBase {
        connection_id: "conn-new",
        database: None,
        namespace: None,
        table: "t",
    };
    let (outcome, _session) = run_save(
        &stored,
        Some(new_source),
        "c",
        &ColumnPresentationOverride::Raw,
        false,
        stored.clone(),
    );
    assert_eq!(outcome.result, Err(SaveColumnOverrideReason::TooManyOverrides));
    assert!(!outcome.persist_called);
}

#[test]
fn save_succeeds_without_persist_when_deleting_a_missing_entry() {
    let stored = StoredValuePresentationSettings {
        version: 1,
        renderers: BTreeMap::new(),
        rules: None,
        column_overrides: None,
        extra: serde_json::Map::new(),
    };
    let (outcome, _session) = run_save(
        &stored,
        Some(source_base()),
        "epoch_ms",
        &ColumnPresentationOverride::Inherit,
        false,
        stored.clone(),
    );
    assert_eq!(outcome.result, Ok(()));
    assert!(!outcome.persist_called);
}

#[test]
fn save_clears_the_session_entry_on_no_change_t10_bug001() {
    let stored = stored_with_overrides(vec![(key_for("epoch_ms"), custom_ms())]);
    let (outcome, session) = run_save(
        &stored,
        Some(source_base()),
        "epoch_ms",
        &custom_ms(),
        false,
        stored.clone(),
    );
    assert_eq!(outcome.result, Ok(()));
    // No change → no persistence write, but the optimistic session entry
    // must clear (a residual entry equal to the stored value would
    // inflate the merged snapshot revision).
    assert!(!outcome.persist_called);
    assert!(session.state().overrides.is_empty());
    assert_eq!(session.state().revision, 2, "set + clear both bump");
}

#[test]
fn save_updater_applies_to_the_fresh_authoritative_value_t11() {
    // This window's stale base: no rules, no other overrides.
    let stored = StoredValuePresentationSettings {
        version: 1,
        renderers: BTreeMap::new(),
        rules: None,
        column_overrides: None,
        extra: serde_json::Map::new(),
    };
    // Another window's simultaneous saves live in the fresh value.
    let fresh = stored_with_overrides(vec![(key_for("other"), ColumnPresentationOverride::Raw)]);
    let fresh = StoredValuePresentationSettings {
        rules: Some(vec![match datazen_ui::support::value_presentation::value_presentation_rules::
            validate_value_presentation_rule(&json!({
                "name": "unix-ms",
                "enabled": true,
                "match": { "column": { "kind": "exact", "value": "epoch_ms" } },
                "options": {
                    "interpretation": { "kind": "unix", "unit": "milliseconds" },
                    "format": "iso-tz",
                    "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
                },
            })) {
            datazen_ui::support::value_presentation::value_presentation_rules::ValuePresentationRuleValidation::Ok(rule) => *rule,
            other => panic!("fixture invalid: {other:?}"),
        }]),
        ..fresh
    };
    let (outcome, _session) = run_save(
        &stored,
        Some(source_base()),
        "epoch_ms",
        &custom_ms(),
        false,
        fresh.clone(),
    );
    assert_eq!(outcome.result, Ok(()));
    let next = outcome.persisted_next.expect("persist ran");
    let overrides = next.column_overrides.expect("overrides present");
    assert_eq!(
        overrides.get(&key_for("epoch_ms")),
        Some(&custom_ms()),
        "this column's override is written"
    );
    assert_eq!(
        overrides.get(&key_for("other")),
        Some(&ColumnPresentationOverride::Raw),
        "the other window's override survives"
    );
    assert_eq!(next.rules.as_ref().map(Vec::len), Some(1), "the other window's rules survive");
    assert_eq!(next.renderers, fresh.renderers, "the fresh renderers survive");
}

#[test]
fn cap_constant_is_pinned() {
    // The React suite pins this constant explicitly.
    assert_eq!(MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES, 200);
    assert_ne!(custom_ms(), custom_seconds());
}
