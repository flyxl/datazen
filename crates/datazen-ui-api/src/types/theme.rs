//! Port of `packages/frontend/src/types/theme.ts` (T276) — the theme
//! preference vocabulary: the `SettingsThemePreference` twin re-export plus
//! the app-level default and the legacy-migration normalizer the settings
//! store feeds any persisted junk through.
//!
//! React's file is a facade over `@datazen/backend-client`'s
//! `SettingsThemePreference` (the type lives there; this file adds the
//! default and the normalizer). The port mirrors that: the struct is
//! `crate::types::client_settings`'s ([`SettingsThemePreference`] — the
//! one definition, no fork), and this module owns the three members the
//! React file declares on top of it — `ThemePreference`/`ThemeMode` (type
//! aliases — Rust's type-alias law: the aggregate family documents, not
//! redefines), `DEFAULT_THEME_PREFERENCE`, and `normalizeThemePreference`.
//!
//! The file is NOT on the `types/index.ts` aggregate's re-export surface
//! (React's `types/theme.ts` has its own importers — the settings store —
//! and the index does not carry it), so the family root does not
//! re-export this module either: reachability rides the module path
//! `datazen_ui_api::types::theme`, the twin of React's `'../types/theme'`.

use serde_json::Value;

use crate::types::client_settings::{SettingsThemePreference, ThemePreferenceMode};

/// React `ThemePreference` — the alias leg: the backend-client struct.
pub type ThemePreference = SettingsThemePreference;

/// React `ThemeMode` — the alias leg: the struct's `mode` member's type.
pub type ThemeMode = ThemePreferenceMode;

/// React `DEFAULT_THEME_PREFERENCE` — `{ mode: 'dark', packId: null }`:
/// the app ships dark unless the settings say otherwise.
pub const DEFAULT_THEME_PREFERENCE: ThemePreference = SettingsThemePreference {
    mode: ThemePreferenceMode::Dark,
    pack_id: None,
};

/// React's mode set (`new Set(['light', 'dark', 'system'])`): the only
/// strings that can be a mode.
const MODES: [ThemePreferenceMode; 3] = [
    ThemePreferenceMode::Light,
    ThemePreferenceMode::Dark,
    ThemePreferenceMode::System,
];

fn is_mode(candidate: &str) -> Option<ThemePreferenceMode> {
    MODES.into_iter().find(|mode| mode.as_str() == candidate)
}

/// React `normalizeThemePreference(input: unknown)` — migrate whatever the
/// persistence layer kept into a well-formed preference, or fall back to the
/// default:
///
/// - a bare mode string (the legacy shape) becomes `{ mode, packId: null }`;
/// - an object with a valid `mode` keeps its `packId` when that is a
///   non-empty string (anything else is `null`);
/// - anything else (garbage, a bad mode, no mode at all) returns the default.
///
/// The `unknown` argument ports as the JSON value it always parses from.
pub fn normalize_theme_preference(input: &Value) -> ThemePreference {
    if let Value::String(mode) = input {
        if let Some(mode) = is_mode(mode) {
            return SettingsThemePreference {
                mode,
                pack_id: None,
            };
        }
    }
    if let Value::Object(obj) = input {
        if let Some(Value::String(mode)) = obj.get("mode") {
            if let Some(mode) = is_mode(mode) {
                let pack_id = match obj.get("packId") {
                    Some(Value::String(pack)) if !pack.is_empty() => Some(pack.clone()),
                    _ => None,
                };
                return SettingsThemePreference {
                    mode,
                    pack_id,
                };
            }
        }
    }
    DEFAULT_THEME_PREFERENCE
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pref(mode: ThemePreferenceMode, pack_id: Option<&str>) -> ThemePreference {
        SettingsThemePreference {
            mode,
            pack_id: pack_id.map(str::to_owned),
        }
    }

    /// The React suite (`__tests__/theme.test.ts`) ports below.
    #[test]
    fn migrates_legacy_string() {
        assert_eq!(
            normalize_theme_preference(&serde_json::json!("dark")),
            pref(ThemePreferenceMode::Dark, None)
        );
        assert_eq!(
            normalize_theme_preference(&serde_json::json!("light")),
            pref(ThemePreferenceMode::Light, None)
        );
        assert_eq!(
            normalize_theme_preference(&serde_json::json!("system")),
            pref(ThemePreferenceMode::System, None)
        );
    }

    #[test]
    fn keeps_object_shape() {
        assert_eq!(
            normalize_theme_preference(&serde_json::json!({
                "mode": "light",
                "packId": "community.slate-blue"
            })),
            pref(ThemePreferenceMode::Light, Some("community.slate-blue"))
        );
    }

    #[test]
    fn falls_back_on_garbage() {
        assert_eq!(
            normalize_theme_preference(&serde_json::json!(null)),
            DEFAULT_THEME_PREFERENCE
        );
        assert_eq!(
            normalize_theme_preference(&serde_json::json!({ "mode": "neon" })),
            DEFAULT_THEME_PREFERENCE
        );
    }

    /// Beyond the React suite — the normalizer's object arms the suite does
    /// not reach: an empty-string packId dies (the `length > 0` leg) and a
    /// non-string packId dies too.
    #[test]
    fn an_empty_or_non_string_pack_id_becomes_none() {
        assert_eq!(
            normalize_theme_preference(&serde_json::json!({
                "mode": "dark",
                "packId": ""
            })),
            pref(ThemePreferenceMode::Dark, None)
        );
        assert_eq!(
            normalize_theme_preference(&serde_json::json!({
                "mode": "dark",
                "packId": 7
            })),
            pref(ThemePreferenceMode::Dark, None)
        );
    }

    /// The default's own literal: dark, no pack — the pin the fallback
    /// tests lean on.
    #[test]
    fn the_default_is_dark_with_no_pack() {
        assert_eq!(DEFAULT_THEME_PREFERENCE.mode, ThemePreferenceMode::Dark);
        assert_eq!(DEFAULT_THEME_PREFERENCE.pack_id, None);
    }
}
