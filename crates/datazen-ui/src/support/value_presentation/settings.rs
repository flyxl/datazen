//! Port of `packages/frontend/src/lib/valuePresentation/settings.ts`
//! (issue-32 T06) — the persisted valuePresentation shape, its lenient
//! parse and the in-memory snapshot builder.
//!
//! The shape landed with T037 (columnOverrides applies to it); this task
//! (T182) fills in the parse layer and the snapshot builder, plus
//! `registerBundledContributions.ts` (the startup registration point).
//!
//! ## Stored shape (React, versioned, renderer-id namespaced)
//!
//! ```text
//! StoredValuePresentationSettings = {
//!   version: number,
//!   renderers: Record<string, StoredRendererSettings>,
//!   rules?: ValuePresentationRule[],
//!   columnOverrides?: Record<string, ColumnPresentationOverride>,
//! }
//! StoredRendererSettings = { enabled: boolean, options: TimestampOptions }
//! ```
//!
//! Version adjudication (T08/T10): version stays 1 with optional
//! additive fields; an unknown top-level version keeps its `version`
//! value, is never parsed into rules/columnOverrides and is never written
//! back with defaults.
//!
//! # Parse layer ([`parse_stored_value_presentation_settings`])
//!
//! Lenient at the host boundary (old backend payloads and corrupted local
//! values must read, not crash): non-objects → the default; a non-number
//! version → 1; non-object renderers → empty; renderer entries keep
//! plain objects verbatim (no deep options validation — that happens at
//! snapshot build, per contribution); `rules` / `columnOverrides` parse
//! only at version 1 and only when the field is present (an old payload
//! never gains an invented `rules: []`); an unknown version parses
//! neither field.
//!
//! # Snapshot builder ([`build_value_presentation_snapshot`])
//!
//! - Unknown version → the empty snapshot (the store keeps its original;
//!    never written back with defaults).
//! - Per renderer entry: a missing contribution skips; `enabled !==
//!   true` skips; the contribution's `validate_config` gates the entry —
//!   ok puts `{config_version, config}` into `configs` and the id into
//!   `enabled_renderer_ids` (enabled-but-invalid stays out of `configs`,
//!   so the plan layer reports missing-config); a failed validation
//!   skips (the store keeps the original).
//! - `rules` ride the snapshot only while the timestamp renderer is
//!   enabled (capability gate; rules carry their own options, so the
//!   global options' validity is irrelevant).
//! - `overrides` fill in without any enabled gate: a custom override
//!   bypasses the switch by design (T04 adjudication), and raw needs no
//!   switch either.
//!
//! # Known deltas (type-level)
//!
//! "Fresh objects instead of aliasing the shared default" is ownership:
//! every parse returns a newly built value; there is no shared default to
//! alias. `Object.freeze` likewise does not apply.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::support::value_presentation::column_overrides::validate_stored_column_overrides;
use crate::support::value_presentation::contracts::ColumnPresentationOverride;
use crate::support::value_presentation::contracts::ConfigValidationResult;
use crate::support::value_presentation::presentation_plan::{
    PresentationEnvironmentSnapshot, ValuePresentationSnapshot,
};
use crate::support::value_presentation::registry::ValuePresentationRegistry;
use crate::support::value_presentation::timestamp_contribution::TIMESTAMP_CONTRIBUTION_ID;
use crate::support::value_presentation::value_presentation_rules::ValuePresentationRule;
use crate::support::value_presentation::value_presentation_rules::validate_value_presentation_rules;

/// One renderer's stored settings: the global switch plus its options in
/// the T02 TimestampOptions JSON shape (kept opaque here; deep validation
/// happens at snapshot-build time per contribution).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct StoredRendererSettings {
    pub(crate) enabled: bool,
    pub(crate) options: serde_json::Value,
}

/// The versioned valuePresentation storage shape (T06 v1 + T08 rules +
/// T10 columnOverrides, all optional additions — no version bump).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct StoredValuePresentationSettings {
    pub(crate) version: u64,
    pub(crate) renderers: BTreeMap<String, StoredRendererSettings>,
    /// Optional; absent in old payloads (consumers treat it as empty).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) rules: Option<Vec<ValuePresentationRule>>,
    /// Optional; keys are [`crate::support::value_presentation::presentation_plan::column_override_key`]
    /// strings, values store raw/custom only (inherit means no entry).
    #[serde(
        rename = "columnOverrides",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) column_overrides: Option<BTreeMap<String, ColumnPresentationOverride>>,
    /// Unknown fields ride along verbatim (React's `{...fresh}` spread):
    /// an unknown top-level version's future fields must survive path
    /// updates instead of being written back away.
    #[serde(flatten)]
    pub(crate) extra: serde_json::Map<String, serde_json::Value>,
}

/// The v1 default stored value: everything off (a missing fields default
/// to off, design §6). A fresh value every call — no shared default to
/// accidentally mutate.
pub(crate) fn default_value_presentation_settings() -> StoredValuePresentationSettings {
    StoredValuePresentationSettings {
        version: 1,
        renderers: BTreeMap::new(),
        rules: None,
        column_overrides: None,
        extra: serde_json::Map::new(),
    }
}

// ---------------------------------------------------------------------------
// Lenient parse (host boundary)
// ---------------------------------------------------------------------------

/// Leniently parse unknown input into the stored shape (never panics):
/// see the module header's parse-layer rules.
pub(crate) fn parse_stored_value_presentation_settings(
    input: &serde_json::Value,
) -> StoredValuePresentationSettings {
    let Some(object) = input.as_object() else {
        return default_value_presentation_settings();
    };
    let version = match object.get("version") {
        Some(serde_json::Value::Number(number)) if number.is_u64() || number.is_i64() => {
            number.as_u64().unwrap_or(1)
        }
        _ => 1,
    };
    let mut renderers: BTreeMap<String, StoredRendererSettings> = BTreeMap::new();
    if let Some(serde_json::Value::Object(entries)) = object.get("renderers") {
        for (id, entry) in entries {
            // Keep plain-object entries verbatim; contents (enabled /
            // options) are interpreted at snapshot build, not here.
            if entry.is_object() {
                if let Ok(parsed) =
                    serde_json::from_value::<StoredRendererSettings>(entry.clone())
                {
                    renderers.insert(id.clone(), parsed);
                }
            }
        }
    }
    if version != 1 {
        // Unknown version: keep the version verbatim, parse neither
        // rules nor columnOverrides (their future shape is unknown).
        return StoredValuePresentationSettings {
            version,
            renderers,
            rules: None,
            column_overrides: None,
            extra: serde_json::Map::new(),
        };
    }
    let rules = match object.get("rules") {
        Some(serde_json::Value::Array(entries)) => {
            Some(validate_value_presentation_rules(&serde_json::Value::Array(entries.clone())))
        }
        _ => None,
    };
    let column_overrides = match object.get("columnOverrides") {
        Some(value @ serde_json::Value::Object(_)) => {
            Some(validate_stored_column_overrides(value))
        }
        _ => None,
    };
    let extra = object
        .iter()
        .filter(|(key, _)| !matches!(key.as_str(), "version" | "renderers" | "rules" | "columnOverrides"))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    StoredValuePresentationSettings { version, renderers, rules, column_overrides, extra }
}

// ---------------------------------------------------------------------------
// Snapshot builder
// ---------------------------------------------------------------------------

/// Build the in-memory presentation snapshot from a stored value:
/// see the module header's builder rules.
pub(crate) fn build_value_presentation_snapshot(
    stored: &StoredValuePresentationSettings,
    environment: &PresentationEnvironmentSnapshot,
    registry: &ValuePresentationRegistry,
    revision: u64,
) -> ValuePresentationSnapshot {
    // Unknown version: the empty snapshot; the original stays in the
    // store and is never written back with defaults.
    if stored.version != 1 {
        return ValuePresentationSnapshot {
            revision,
            environment: environment.clone(),
            configs: BTreeMap::new(),
            enabled_renderer_ids: Vec::new(),
            overrides: BTreeMap::new(),
            rules: Vec::new(),
        };
    }
    let mut configs: BTreeMap<String, crate::support::value_presentation::contracts::VersionedPresentationConfig> =
        BTreeMap::new();
    let mut enabled_renderer_ids: Vec<String> = Vec::new();
    for (id, entry) in &stored.renderers {
        let Some(contribution) = registry.get(id) else {
            continue;
        };
        if !entry.enabled {
            continue;
        }
        match contribution.validate_config(&entry.options) {
            ConfigValidationResult::Ok(config) => {
                configs.insert(
                    id.clone(),
                    crate::support::value_presentation::contracts::VersionedPresentationConfig {
                        config_version: contribution.config_version(),
                        config,
                    },
                );
                enabled_renderer_ids.push(id.clone());
            }
            ConfigValidationResult::Err(_) => {
                // Enabled but invalid: out of configs (the plan layer
                // reports missing-config); the store keeps the original.
            }
        }
    }
    // Rules ride the snapshot only while the timestamp renderer is
    // enabled (capability gate). Rules carry their own options, so the
    // global options' validity is irrelevant.
    let rules = match stored.renderers.get(TIMESTAMP_CONTRIBUTION_ID) {
        Some(entry) if entry.enabled => {
            validate_value_presentation_rules(&serde_json::to_value(stored.rules.clone().unwrap_or_default()).unwrap_or(serde_json::Value::Array(Vec::new())))
        }
        _ => Vec::new(),
    };
    // Overrides fill in with no enabled gate: custom bypasses the switch
    // (T04 adjudication), raw needs no switch.
    let overrides = validate_stored_column_overrides(
        &serde_json::to_value(stored.column_overrides.clone().unwrap_or_default())
            .unwrap_or(serde_json::Value::Object(Default::default())),
    );
    ValuePresentationSnapshot {
        revision,
        environment: environment.clone(),
        configs,
        enabled_renderer_ids,
        overrides,
        rules,
    }
}
