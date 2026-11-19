//! Independent audit for T310 (the valuePresentationSave path helpers).
//!
//! Every assertion comes from re-reading
//! `packages/frontend/src/lib/valuePresentation/valuePresentationSave.ts`
//! and its React test `__tests__/valuePresentationSave.test.ts` — the
//! whole-object passthrough, updater application, the timestamp-renderer
//! path (rules/overrides/other renderers/unknown-version fields all
//! preserved), the rule-list path (version 1 only, wholesale
//! replacement) and the concurrent-saves merge.
//!
//! The update type and `apply_value_presentation_update` landed with
//! T037; this audit completes their coverage too (the three
//! applyValuePresentationUpdate cases). React's reference-identity
//! checks (`toBe`) become structural equality.

use std::collections::BTreeMap;

use serde_json::json;

use crate::support::value_presentation::column_presentation::ColumnSourceBase;
use crate::support::value_presentation::contracts::ColumnPresentationOverride;
use crate::support::value_presentation::presentation_plan::column_override_key;
use crate::support::value_presentation::settings::{
    StoredRendererSettings, StoredValuePresentationSettings,
};
use crate::support::value_presentation::value_presentation_rules::{
    ValuePresentationRule, validate_value_presentation_rule,
    ValuePresentationRuleValidation,
};
use crate::support::value_presentation::value_presentation_save::{
    ValuePresentationSettingsUpdate, apply_value_presentation_update,
    with_timestamp_renderer, with_value_presentation_rules,
};

fn native_utc_iso() -> serde_json::Value {
    json!({
        "interpretation": { "kind": "native" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
    })
}

fn native_iso_tz_shanghai() -> serde_json::Value {
    json!({
        "interpretation": { "kind": "native" },
        "format": "iso-tz",
        "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
    })
}

fn rule() -> ValuePresentationRule {
    match validate_value_presentation_rule(&json!({
        "name": "unix-ms",
        "enabled": true,
        "match": { "column": { "kind": "exact", "value": "epoch_ms" } },
        "options": {
            "interpretation": { "kind": "unix", "unit": "milliseconds" },
            "format": "iso-tz",
            "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
        },
    })) {
        ValuePresentationRuleValidation::Ok(rule) => *rule,
        ValuePresentationRuleValidation::Err(problems) => {
            panic!("fixture invalid: {problems:#?}")
        }
    }
}

fn stored_with_rule_and_override() -> StoredValuePresentationSettings {
    let mut renderers = BTreeMap::new();
    renderers.insert(
        "datazen.timestamp".to_string(),
        StoredRendererSettings { enabled: true, options: native_utc_iso() },
    );
    let mut overrides = BTreeMap::new();
    overrides.insert(
        column_override_key(
            &ColumnSourceBase {
                connection_id: "conn-1",
                database: Some("shop"),
                namespace: None,
                table: "orders",
            }
            .with_column("epoch_ms"),
        ),
        ColumnPresentationOverride::Raw,
    );
    StoredValuePresentationSettings {
        version: 1,
        renderers,
        rules: Some(vec![rule()]),
        column_overrides: Some(overrides),
        extra: serde_json::Map::new(),
    }
}

// ---------------------------------------------------------------------------
// applyValuePresentationUpdate
// ---------------------------------------------------------------------------

#[test]
fn update_whole_object_passes_through() {
    let fresh = stored_with_rule_and_override();
    let next = StoredValuePresentationSettings {
        version: 1,
        renderers: {
            let mut map = BTreeMap::new();
            map.insert(
                "datazen.timestamp".to_string(),
                StoredRendererSettings { enabled: false, options: native_utc_iso() },
            );
            map
        },
        rules: None,
        column_overrides: None,
        extra: serde_json::Map::new(),
    };
    let result = apply_value_presentation_update(
        fresh,
        ValuePresentationSettingsUpdate::Whole(next.clone()),
    );
    assert_eq!(result, next);
}

#[test]
fn update_updater_applies_to_fresh() {
    let fresh = stored_with_rule_and_override();
    let result = apply_value_presentation_update(
        fresh.clone(),
        ValuePresentationSettingsUpdate::Apply(Box::new(|value| {
            StoredValuePresentationSettings { renderers: BTreeMap::new(), ..value }
        })),
    );
    assert!(result.renderers.is_empty());
    assert_eq!(result.rules.as_ref().map(|rules| rules.len()), Some(1));
    // The input fresh value is not consumed by the wrapper.
    assert!(fresh.renderers.contains_key("datazen.timestamp"));
}

#[test]
fn update_identity_updater_is_a_noop() {
    let fresh = stored_with_rule_and_override();
    let result = apply_value_presentation_update(
        fresh.clone(),
        ValuePresentationSettingsUpdate::Apply(Box::new(|value| value)),
    );
    assert_eq!(result, fresh);
}

// ---------------------------------------------------------------------------
// withTimestampRenderer
// ---------------------------------------------------------------------------

#[test]
fn timestamp_renderer_path_replaces_only_that_entry() {
    let fresh = stored_with_rule_and_override();
    let next = with_timestamp_renderer(&fresh, true, &native_iso_tz_shanghai());
    assert_eq!(
        next.renderers.get("datazen.timestamp"),
        Some(&StoredRendererSettings { enabled: true, options: native_iso_tz_shanghai() })
    );
    assert_eq!(next.rules.as_ref().map(|rules| rules.len()), Some(1), "rules preserved");
    assert_eq!(next.column_overrides, fresh.column_overrides, "overrides preserved");
    assert_eq!(next.version, 1);
    // The input is untouched.
    assert_eq!(
        fresh.renderers.get("datazen.timestamp").map(|entry| &entry.options),
        Some(&native_utc_iso())
    );
}

#[test]
fn timestamp_renderer_path_preserves_other_renderers() {
    let mut renderers = BTreeMap::new();
    renderers.insert(
        "other.renderer".to_string(),
        StoredRendererSettings { enabled: true, options: native_utc_iso() },
    );
    let fresh = StoredValuePresentationSettings {
        version: 1,
        renderers,
        rules: None,
        column_overrides: None,
        extra: serde_json::Map::new(),
    };
    let next = with_timestamp_renderer(&fresh, false, &native_utc_iso());
    assert_eq!(
        next.renderers.get("other.renderer"),
        Some(&StoredRendererSettings { enabled: true, options: native_utc_iso() })
    );
    assert_eq!(
        next.renderers.get("datazen.timestamp"),
        Some(&StoredRendererSettings { enabled: false, options: native_utc_iso() })
    );
}

#[test]
fn timestamp_renderer_path_preserves_unknown_version_fields() {
    // The future field rides in the flattened extra map (React's spread).
    let mut extra = serde_json::Map::new();
    extra.insert("futureField".to_string(), json!({ "kept": true }));
    let fresh = StoredValuePresentationSettings {
        version: 99,
        renderers: BTreeMap::new(),
        rules: None,
        column_overrides: None,
        extra,
    };
    let next = with_timestamp_renderer(&fresh, true, &native_utc_iso());
    assert_eq!(next.version, 99);
    assert_eq!(
        next.renderers.get("datazen.timestamp"),
        Some(&StoredRendererSettings { enabled: true, options: native_utc_iso() })
    );
    // The unknown field survives the path update (and serialization).
    assert_eq!(next.extra.get("futureField"), Some(&json!({ "kept": true })));
    let serialized = serde_json::to_value(&next).expect("serializable");
    assert_eq!(serialized["futureField"], json!({ "kept": true }));
    assert_eq!(serialized["version"], json!(99));
}

// ---------------------------------------------------------------------------
// withValuePresentationRules
// ---------------------------------------------------------------------------

#[test]
fn rules_path_replaces_the_list_on_version_1() {
    let fresh = stored_with_rule_and_override();
    let new_rule = rule();
    let next = with_value_presentation_rules(&fresh, &[new_rule.clone()]);
    assert_eq!(next.rules, Some(vec![new_rule]));
    assert_eq!(next.renderers, fresh.renderers, "renderers preserved");
    assert_eq!(next.column_overrides, fresh.column_overrides, "overrides preserved");
}

#[test]
fn rules_path_returns_fresh_for_unknown_versions() {
    let fresh = StoredValuePresentationSettings {
        version: 99,
        rules: Some(vec![rule()]),
        ..stored_with_rule_and_override()
    };
    let next = with_value_presentation_rules(&fresh, &[]);
    assert_eq!(next, fresh, "unknown version: fresh verbatim");
}

// ---------------------------------------------------------------------------
// Concurrent saves merge by path (T11)
// ---------------------------------------------------------------------------

#[test]
fn concurrent_saves_never_lose_each_others_fields() {
    // Window A changes the global format; window B changes rules and a
    // column override. Both apply their path to the same fresh value:
    // neither loses the other's fields.
    let fresh = stored_with_rule_and_override();
    let a = with_timestamp_renderer(&fresh, true, &native_iso_tz_shanghai());
    let mut overrides = fresh.column_overrides.clone().unwrap_or_default();
    overrides.insert(
        column_override_key(
            &ColumnSourceBase {
                connection_id: "conn-1",
                database: Some("shop"),
                namespace: None,
                table: "orders",
            }
            .with_column("other"),
        ),
        ColumnPresentationOverride::Raw,
    );
    let new_rule = rule();
    let b_rules = vec![new_rule];
    let b = crate::support::value_presentation::column_overrides::with_column_override(
        &StoredValuePresentationSettings { rules: Some(b_rules.clone()), ..fresh.clone() },
        ColumnSourceBase {
            connection_id: "conn-1",
            database: Some("shop"),
            namespace: None,
            table: "orders",
        },
        "other",
        &ColumnPresentationOverride::Raw,
    );

    // A's write and B's write each applied to the same authoritative
    // base produce values that keep their own paths; the store applies
    // them in sequence on fresh re-reads:
    let after_a_then_b = crate::support::value_presentation::column_overrides::with_column_override(
        &with_timestamp_renderer(&fresh, true, &native_iso_tz_shanghai()),
        ColumnSourceBase {
            connection_id: "conn-1",
            database: Some("shop"),
            namespace: None,
            table: "orders",
        },
        "other",
        &ColumnPresentationOverride::Raw,
    );
    assert_eq!(
        after_a_then_b.renderers.get("datazen.timestamp").map(|entry| &entry.options),
        Some(&native_iso_tz_shanghai()),
        "A's global format survives B's save"
    );
    assert_eq!(
        after_a_then_b.rules.as_ref().map(Vec::len),
        Some(1),
        "the rule list survives"
    );
    assert_eq!(
        after_a_then_b.column_overrides.as_ref().map(|o| o.len()),
        Some(2),
        "both overrides survive"
    );
    // Both single-path writes are self-consistent too.
    assert_eq!(a.rules.as_ref().map(Vec::len), Some(1));
    assert!(b.column_overrides.as_ref().is_some_and(|o| o.len() == 2));
    let _ = b_rules;
}
