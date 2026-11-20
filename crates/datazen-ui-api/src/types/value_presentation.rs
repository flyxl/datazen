//! The persisted value-presentation shape `AppSettings` embeds — ported
//! from React `packages/frontend/src/lib/valuePresentation/settings.ts`
//! via `packages/frontend/src/types/index.ts` (tasklist T211).
//!
//! React's `types/index.ts` imports `StoredValuePresentationSettings`
//! (import-only, not re-exported) as the type of `AppSettings`'s
//! `valuePresentation` field — the versioned, per-renderer storage shape
//! (issue-32 T06, design §5/§6). The frontend module's own header records
//! the Rust-side ruling this mirror follows verbatim: "Rust 侧只做存储
//! 透传（`ValuePresentationSettings`，renderers 值为 `serde_json::Value`，
//! rules 与 columnOverrides 为不透明 JSON），解释与验证在前端宿主边界
//! 做" — storage pass-through only; interpretation and validation happen
//! at the frontend host boundary, so `rules` (whose TS type
//! `ValuePresentationRule` lives in `lib/valuePresentationRules.ts`, out
//! of this family's reach) and `columnOverrides` (whose TS type
//! `ColumnPresentationOverride` lives in extension-points) ride as opaque
//! JSON, and a renderer entry's `options` (the T02 `TimestampOptions`
//! JSON shape) is never structurally decoded here either.
//!
//! The version ruling (T08, the TS file's own header): keep version 1 with
//! the additive optional `rules`/`columnOverrides` fields — an old payload
//! without them reads as their absence, an unknown top-level version is
//! preserved verbatim (never rewritten with defaults).
//!
//! No Rust home existed, so this module is the mirror. Pure contract — no
//! IO, no state — so the module compiles for wasm32 exactly like its
//! `chart_types` sibling.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::connection_types::Value;

/// React `StoredRendererSettings` — one renderer's stored settings: the
/// global switch plus its options (the T02 `TimestampOptions` JSON shape,
/// kept opaque per the module docs' pass-through ruling).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredRendererSettings {
    pub enabled: bool,
    /// The renderer's own options JSON — validated by the frontend host
    /// boundary, never structurally decoded here.
    pub options: Value,
}

/// React `StoredValuePresentationSettings` — the versioned storage shape
/// `AppSettings.valuePresentation` carries, namespaced per renderer id.
///
/// `renderers` is `Record<string, StoredRendererSettings>` (name-keyed,
/// never order-observed — the `ai_types` rule); `rules` and
/// `columnOverrides` are the additive optional fields of the T08/T10
/// extension, opaque JSON here (see the module docs' pass-through ruling).
/// The default of v1 is no renderers — everything off; the settings-store
/// port owns materializing that.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredValuePresentationSettings {
    /// Storage schema version (v1 today; unknown versions are preserved
    /// verbatim, never rewritten).
    pub version: u32,
    pub renderers: BTreeMap<String, StoredRendererSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_overrides: Option<BTreeMap<String, Value>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire)
            .unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    fn encode<T: serde::Serialize>(value: &T) -> serde_json::Value {
        serde_json::to_value(value)
            .unwrap_or_else(|error| panic!("wire encode failed: {error}"))
    }

    #[test]
    fn the_stored_shape_round_trips_opaquely() {
        let stored = StoredValuePresentationSettings {
            version: 1,
            renderers: {
                let mut map = BTreeMap::new();
                map.insert(
                    "datazen.timestamp".into(),
                    StoredRendererSettings {
                        enabled: true,
                        options: serde_json::json!({
                            "interpretation": { "kind": "unix", "unit": "milliseconds" },
                            "format": "iso",
                            "displayTimeZone": { "kind": "utc" },
                        }),
                    },
                );
                map
            },
            rules: None,
            column_overrides: None,
        };
        let wire = encode(&stored);
        assert_eq!(wire["version"], 1);
        assert_eq!(wire["renderers"]["datazen.timestamp"]["enabled"], true);
        // The renderer options stay the opaque JSON they arrived as.
        assert_eq!(
            wire["renderers"]["datazen.timestamp"]["options"]["format"],
            "iso"
        );
        // The additive optional fields stay absent (the v1 default).
        assert!(wire.get("rules").is_none());
        assert!(wire.get("columnOverrides").is_none());
        let back: StoredValuePresentationSettings = decode(wire);
        assert_eq!(back, stored);
    }

    #[test]
    fn additive_fields_and_unknown_versions_read_back() {
        // A T08/T10 payload: rules and columnOverrides present.
        let full: StoredValuePresentationSettings = decode(serde_json::json!({
            "version": 1,
            "renderers": {},
            "rules": [{ "name": "epoch columns", "enabled": true }],
            "columnOverrides": { "c1|db|ns|t|col": { "mode": "raw" } }
        }));
        assert_eq!(full.rules.as_ref().map(Vec::len), Some(1));
        assert_eq!(full.column_overrides.as_ref().map(BTreeMap::len), Some(1));
        // An old payload (pre-T08) reads as their absence…
        let old: StoredValuePresentationSettings =
            decode(serde_json::json!({ "version": 1, "renderers": {} }));
        assert!(old.rules.is_none());
        assert!(old.column_overrides.is_none());
        // …and an unknown top-level version is preserved verbatim — the
        // opaque legs keep whatever they carried, `version` is not rewritten.
        let unknown: StoredValuePresentationSettings =
            decode(serde_json::json!({ "version": 99, "renderers": {} }));
        assert_eq!(unknown.version, 99);
    }
}
