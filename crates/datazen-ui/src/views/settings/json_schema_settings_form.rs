//! Port of `packages/frontend/src/windows/settings/JsonSchemaSettingsForm.tsx`
//! (T304) — the generic JSON-schema settings form.
//!
//! A driver that contributes no custom `SettingsSection` gets this form:
//! every **boolean** property of its JSON schema renders as one toggle
//! row, with the schema's `title` (falling back to the key) as the
//! label, its `description` as the hint, and `default` as the value a
//! missing key reads as. The stored value only counts when it is a
//! plain object; anything else (an array, a scalar, `null`) starts from
//! `{}` exactly as React's `Array.isArray` guard did.
//!
//! ## What maps where
//!
//! - `applySchemaDefaults` / `listSchemaPropertyEntries` /
//!   `readBooleanField` come from `@datazen/driver-sdk`
//!   (`driver-sdk/src/driverSettings.ts`) — carried here until that
//!   file's own port absorbs them (the `lib_helpers` pattern), 1:1 in
//!   rule: defaults fill only **absent** keys (`undefined`, not
//!   `null`), only `type: 'boolean'` properties are listed, and a
//!   non-boolean stored value reads as the default.
//! - `ToggleRow` (`settingsUi.tsx`, T399) is unported; the row is the
//!   kit [`Checkbox`] with label + description — the `ai_settings_section`
//!   precedent for the same React component, so the two sections' rows
//!   cannot drift apart. An on-toggle press is React's
//!   `onChange(!checked)`; the row-level change is
//!   `onChange({ ...current, [key]: next })` merged at build time (the
//!   snapshot the closure captures is the one the user saw).
//! - `fields.length === 0` renders nothing: [`Self::build`] answers
//!   `None` (React's `null`), so the caller skips the block entirely.
//!
//! Known deltas (documented, not silent): JS object key order is
//! insertion order; `serde_json`'s map is dictionary-ordered, so a
//! re-serialized draft lists keys sorted (the `to_edit_string` port's
//! own recorded delta — no consumer of this form reads key order).
//!
//! ## Keys
//!
//! Rows derive from the form's key base (default
//! `"json-schema-settings-form"`): one `<base>-<property>` toggle per
//! boolean property.

#![allow(dead_code)] // consumers: the settings view's driver section (the T265 seam)

use std::rc::Rc;

use rune::{col, El};
use serde_json::Value;

use crate::ui::kit::kit;
use crate::ui::toggle::Checkbox;

// ---------------------------------------------------------------------------
// driver-sdk/src/driverSettings.ts (carried until that file's port)
// ---------------------------------------------------------------------------

/// One `type: 'boolean'` property of the schema, with the fields the
/// form renders.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SchemaPropertyEntry {
    pub key: String,
    /// `prop.title` — the label; falls back to the key when absent.
    pub title: Option<String>,
    /// `prop.description` — the hint.
    pub description: Option<String>,
    /// `prop.default` — the value a missing key reads as.
    pub default_value: Option<Value>,
}

/// `readBooleanField`: a boolean stored value, else the default.
pub(crate) fn read_boolean_field(
    obj: &serde_json::Map<String, Value>,
    key: &str,
    default_value: bool,
) -> bool {
    match obj.get(key) {
        Some(Value::Bool(flag)) => *flag,
        _ => default_value,
    }
}

/// `applySchemaDefaults`: copy the value, then fill every **absent**
/// key (`undefined` — a stored `null` is a value and stays) with the
/// schema property's `default`. A schema that is not
/// `{"type": "object"}` with `properties` returns the value unchanged.
pub(crate) fn apply_schema_defaults(
    schema: &Value,
    value: &serde_json::Map<String, Value>,
) -> serde_json::Map<String, Value> {
    let mut out = value.clone();
    let Some(properties) = schema_properties(schema) else {
        return out;
    };
    for (key, prop) in properties {
        if !out.contains_key(key) {
            if let Some(default_value) = prop.get("default") {
                if !default_value.is_null() {
                    out.insert(key.clone(), default_value.clone());
                }
            }
        }
    }
    out
}

/// `listSchemaPropertyEntries`: the schema's `type: 'boolean'`
/// properties, in the schema's own (dictionary-ordered) listing.
pub(crate) fn list_schema_property_entries(schema: &Value) -> Vec<SchemaPropertyEntry> {
    let Some(properties) = schema_properties(schema) else {
        return Vec::new();
    };
    properties
        .iter()
        .filter(|(_, prop)| prop.get("type") == Some(&Value::String("boolean".to_owned())))
        .map(|(key, prop)| SchemaPropertyEntry {
            key: key.clone(),
            title: prop.get("title").and_then(Value::as_str).map(str::to_owned),
            description: prop.get("description").and_then(Value::as_str).map(str::to_owned),
            default_value: prop.get("default").cloned().filter(|v| !v.is_null()),
        })
        .collect()
}

/// The schema's `properties` map, when the schema is an object schema.
fn schema_properties(schema: &Value) -> Option<&serde_json::Map<String, Value>> {
    let object = schema.as_object()?;
    if object.get("type") != Some(&Value::String("object".to_owned())) {
        return None;
    }
    object.get("properties")?.as_object()
}

// ---------------------------------------------------------------------------
// The form (React `JsonSchemaSettingsForm`)
// ---------------------------------------------------------------------------

/// The generic JSON-schema settings form.
///
/// Generic over the host state `S` (the toggle closures write through
/// `on_change`, exactly as React's `onChange` prop did).
pub(crate) struct JsonSchemaSettingsForm<'a, S: 'static> {
    key: &'a str,
    schema: &'a Value,
    /// The stored settings value (`unknown` in React: only a plain
    /// object counts; anything else starts from `{}`).
    value: &'a Value,
    on_change: Option<Rc<dyn Fn(&mut S, Value)>>,
}

impl<'a, S: 'static> JsonSchemaSettingsForm<'a, S> {
    pub(crate) fn new(schema: &'a Value, value: &'a Value) -> Self {
        Self { key: "json-schema-settings-form", schema, value, on_change: None }
    }

    /// Override the key base (one form per driver entry: the caller
    /// passes the section's block key so rows stay unique).
    pub(crate) fn key(mut self, key: &'a str) -> Self {
        self.key = key;
        self
    }

    /// `onChange(next)`: the whole merged object, one call per toggle.
    pub(crate) fn on_change(mut self, f: impl Fn(&mut S, Value) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Build the form. `None` when the schema lists no boolean
    /// properties (React renders `null`).
    pub(crate) fn build(self, dark: bool) -> Option<El> {
        let fields = list_schema_property_entries(self.schema);
        if fields.is_empty() {
            return None;
        }

        // React's `Array.isArray` guard: only a plain object counts as
        // the stored value.
        let raw: serde_json::Map<String, Value> = match self.value {
            Value::Object(map) => map.clone(),
            _ => serde_json::Map::new(),
        };
        let current = apply_schema_defaults(self.schema, &raw);

        let k = kit(dark);
        let mut form = col().key(self.key.to_owned()).w_full().gap(12.0);
        for field in fields {
            // `typeof prop.default === 'boolean' ? prop.default : false`.
            let default_value = matches!(field.default_value, Some(Value::Bool(true)));
            let checked = read_boolean_field(&current, &field.key, default_value);
            let label = field.title.clone().unwrap_or_else(|| field.key.clone());

            let mut row = Checkbox::new(format!("{}-{}", self.key, field.key), checked)
                .label(label.clone());
            if let Some(description) = field.description.clone() {
                row = row.description(description);
            }
            if let Some(on_change) = &self.on_change {
                let on_change = Rc::clone(on_change);
                let next = merged_toggle_value(&current, &field.key, !checked);
                row = row.on_toggle(move |s: &mut S| on_change(s, next.clone()));
            }
            form = form.child(row.build(&k));
        }
        Some(form)
    }
}

/// `handleFieldChange`: `onChange({ ...current, [key]: nextVal })` —
/// the whole object, one toggle at a time.
fn merged_toggle_value(
    current: &serde_json::Map<String, Value>,
    key: &str,
    next: bool,
) -> Value {
    let mut merged = current.clone();
    merged.insert(key.to_owned(), Value::Bool(next));
    Value::Object(merged)
}

// ---------------------------------------------------------------------------
// The tests (React has none for this component; the carried helpers'
// rules and the form's decisions are re-derived from the source)
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    fn bool_schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "cache": {
                    "type": "boolean",
                    "title": "Cache results",
                    "description": "Reuse the last result when the query is unchanged",
                    "default": true,
                },
                "verbose": { "type": "boolean", "title": "Verbose logs", "default": false },
                "name": { "type": "string", "title": "A non-boolean property" },
            },
        })
    }

    #[test]
    fn defaults_fill_only_absent_keys() {
        let schema = bool_schema();
        // A stored null is a value, not an absence: it stays.
        let raw = serde_json::from_value::<serde_json::Map<String, Value>>(json!({
            "verbose": null,
            "extra": 7,
        }))
        .expect("a plain object");
        let merged = apply_schema_defaults(&schema, &raw);
        assert_eq!(merged.get("cache"), Some(&json!(true)), "absent → the default");
        assert_eq!(merged.get("verbose"), Some(&Value::Null), "a stored null stays null");
        assert_eq!(merged.get("extra"), Some(&json!(7)), "unknown keys pass through");
        // A present value is never overwritten by a default.
        let raw = serde_json::from_value::<serde_json::Map<String, Value>>(json!({
            "cache": false
        }))
        .expect("a plain object");
        let merged = apply_schema_defaults(&schema, &raw);
        assert_eq!(merged.get("cache"), Some(&json!(false)));
    }

    #[test]
    fn a_non_object_schema_or_a_schema_without_properties_is_untouched() {
        let raw = serde_json::from_value::<serde_json::Map<String, Value>>(json!({ "a": 1 }))
            .expect("a plain object");
        assert_eq!(
            apply_schema_defaults(&json!({ "type": "string" }), &raw).get("a"),
            Some(&json!(1))
        );
        assert_eq!(
            apply_schema_defaults(&json!({ "type": "object" }), &raw).get("a"),
            Some(&json!(1))
        );
    }

    #[test]
    fn only_boolean_properties_are_listed_with_their_faces() {
        let entries = list_schema_property_entries(&bool_schema());
        assert_eq!(
            entries,
            vec![
                SchemaPropertyEntry {
                    key: "cache".to_owned(),
                    title: Some("Cache results".to_owned()),
                    description: Some(
                        "Reuse the last result when the query is unchanged".to_owned()
                    ),
                    default_value: Some(json!(true)),
                },
                SchemaPropertyEntry {
                    key: "verbose".to_owned(),
                    title: Some("Verbose logs".to_owned()),
                    description: None,
                    default_value: Some(json!(false)),
                },
            ]
        );
        assert!(list_schema_property_entries(&json!({ "type": "object" })).is_empty());
        assert!(list_schema_property_entries(&json!(42)).is_empty());
    }

    #[test]
    fn a_boolean_field_reads_the_default_only_for_non_booleans() {
        let obj = serde_json::from_value::<serde_json::Map<String, Value>>(json!({
            "on": true,
            "off": false,
            "weird": "yes",
        }))
        .expect("a plain object");
        assert!(read_boolean_field(&obj, "on", false));
        assert!(!read_boolean_field(&obj, "off", true));
        assert!(read_boolean_field(&obj, "weird", true), "a non-boolean reads the default");
        assert!(!read_boolean_field(&obj, "missing", false));
    }

    #[test]
    fn a_schema_without_boolean_properties_builds_nothing() {
        struct S;
        let schema = json!({
            "type": "object",
            "properties": { "name": { "type": "string" } },
        });
        let empty = json!({});
        let form = JsonSchemaSettingsForm::<S>::new(&schema, &empty);
        assert!(form.build(false).is_none(), "React renders null");
    }

    #[test]
    fn a_non_object_stored_value_starts_from_the_empty_object() {
        struct S;
        // An array value: React's Array.isArray guard starts from {} —
        // the form still builds, and the merge input is the
        // defaults-applied empty object.
        let schema = bool_schema();
        let array_value = json!([1, 2]);
        let form = JsonSchemaSettingsForm::<S>::new(&schema, &array_value);
        assert!(form.build(false).is_some(), "the schema lists boolean properties");
        let raw = serde_json::Map::new();
        let current = apply_schema_defaults(&schema, &raw);
        assert_eq!(current.get("cache"), Some(&json!(true)));
        assert_eq!(current.get("verbose"), Some(&json!(false)));
    }

    #[test]
    fn one_toggle_merges_into_the_whole_object() {
        let current = serde_json::from_value::<serde_json::Map<String, Value>>(json!({
            "cache": true,
            "verbose": false,
        }))
        .expect("a plain object");
        // Toggling verbose off→on keeps cache and rewrites verbose.
        assert_eq!(
            merged_toggle_value(&current, "verbose", true),
            json!({ "cache": true, "verbose": true })
        );
        // Toggling a key the object never held adds it.
        assert_eq!(
            merged_toggle_value(&current, "new", false),
            json!({ "cache": true, "verbose": false, "new": false })
        );
    }
}
