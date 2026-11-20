//! Skills settings / session-metadata normalizers — the Rust home of React
//! `packages/frontend/src/lib/aiSkills.ts` (task T291, 35 lines).
//!
//! ## Ownership: continue, do not fork
//!
//! This React file has exactly two exports, and both are already ported in
//! this crate by sibling tasks:
//!
//! | React export | Canonical Rust home | Owner |
//! | --- | --- | --- |
//! | `normalizeAiSkillsSettings` | [`normalize_ai_skills_settings`] (`ai_types/skills.rs`) | T228 dependency leg |
//! | `normalizeLoadedSkills` | [`normalize_loaded_skills`] (`ai_types/skills.rs`), mirrored in `ai_sessions` for restore | T228 / T230 |
//! | host-side `~/.agents/skills` directory scanning | not in this React file | host-side skills task |
//!
//! T291 therefore does **not** redefine either function — that would create
//! two sources of truth for the same persistence normalizers. This module is
//! the file-level surface: it re-exports the canonical implementations and
//! pins the full React contract with tests that go beyond the earlier
//! suites (dirty booleans, non-string `customDir`, duplicate `selected`
//! ids, blank names, wrongly typed `id`/`source`/`loadedAt`, non-array and
//! non-object entries). The directory-scanning half of the feature stays
//! with its host-side owner; nothing here touches the filesystem or holds
//! state.
//!
//! ## Mirror rules (already applied by the canonical implementations)
//!
//! - `Partial<AiSkillsSettings> | null | undefined` → `Option<&Value>`:
//!   a missing/non-object input yields the exact `defaultAiSkillsSettings`.
//! - `skills?.enabled === true` / `autoUse === true` are strict: only the
//!   boolean `true` turns them on; `1`, `"true"`, `null` stay off.
//! - `customDir` is `string | null`: only a JSON string survives, empty
//!   string included; every other spelling becomes `None` (explicit null).
//! - `selected` is a `new Set` over the string-only filter: first occurrence
//!   of each id wins, input order is kept, numbers/null/objects die.
//! - `normalizeLoadedSkills` keeps only object entries with a non-blank
//!   string `name` (the name itself is **not** trimmed); `id`/`source` fall
//!   back to `None` unless strings; `loadedAt` defaults to the injected
//!   `now_iso` (React's `new Date().toISOString()` — this crate has no clock).

pub use crate::ai_types::{normalize_ai_skills_settings, normalize_loaded_skills};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_types::{AiLoadedSkill, AiSkillsSettings, DEFAULT_AI_SKILLS_SETTINGS};
    use serde_json::{json, Value};

    fn decode_settings(wire: Value) -> AiSkillsSettings {
        normalize_ai_skills_settings(Some(&wire))
    }

    // React's `defaultAiSkillsSettings` literal, pinned through the normalizer
    // (a missing/partial/dirty input all land on exactly this object).
    #[test]
    fn a_missing_or_non_object_input_yields_the_react_default_literal() {
        for input in [None, Some(&Value::Null), Some(&json!(42)), Some(&json!([]))] {
            let normalized = normalize_ai_skills_settings(input);
            assert_eq!(normalized, DEFAULT_AI_SKILLS_SETTINGS);
        }
        assert!(!DEFAULT_AI_SKILLS_SETTINGS.enabled);
        assert!(!DEFAULT_AI_SKILLS_SETTINGS.auto_use);
        assert_eq!(DEFAULT_AI_SKILLS_SETTINGS.custom_dir, None);
        assert!(DEFAULT_AI_SKILLS_SETTINGS.selected.is_empty());
        assert_eq!(
            serde_json::to_value(DEFAULT_AI_SKILLS_SETTINGS.clone()).ok(),
            Some(json!({"enabled": false, "customDir": null, "autoUse": false, "selected": []}))
        );
    }

    // React: `enabled: skills?.enabled === true` — strict boolean, not truthy.
    #[test]
    fn booleans_trust_only_the_literal_true() {
        for value in [json!(true)] {
            let settings = decode_settings(json!({
                "enabled": value,
                "autoUse": value,
            }));
            assert!(settings.enabled);
            assert!(settings.auto_use);
        }
        for dirty in [json!(false), json!(1), json!("true"), json!(null), json!({})] {
            let settings = decode_settings(json!({"enabled": dirty.clone(), "autoUse": dirty}));
            assert!(!settings.enabled, "enabled was {settings:?}");
            assert!(!settings.auto_use, "autoUse was {settings:?}");
        }
    }

    // React: `typeof skills?.customDir === 'string' ? skills.customDir : null`.
    #[test]
    fn custom_dir_keeps_any_string_verbatim_and_drops_everything_else() {
        for dir in ["", "/opt/skills", "  spaces  ", "relative/path"] {
            let settings = decode_settings(json!({"customDir": dir}));
            assert_eq!(settings.custom_dir.as_deref(), Some(dir));
        }
        for dirty in [json!(null), json!(0), json!(true), json!([]), json!({})] {
            let settings = decode_settings(json!({"customDir": dirty}));
            assert_eq!(settings.custom_dir, None, "customDir was {settings:?}");
        }
        assert_eq!(decode_settings(json!({})).custom_dir, None);
    }

    // React: `new Set(selected.filter(id => typeof id === 'string'))`.
    #[test]
    fn selected_keeps_first_string_occurrence_in_order_and_drops_junk() {
        let settings = decode_settings(json!({
            "selected": ["b", 1, "a", "b", null, "c", "a", {"id": "d"}, true, ""],
        }));
        assert_eq!(
            settings.selected,
            vec!["b".to_owned(), "a".to_owned(), "c".to_owned(), String::new()]
        );
        for dirty in [json!(null), json!("b"), json!(3), json!({})] {
            let settings = decode_settings(json!({"selected": dirty}));
            assert!(settings.selected.is_empty(), "selected was {settings:?}");
        }
        assert!(decode_settings(json!({})).selected.is_empty());
    }

    // Unknown keys from older/newer configs never change the normalized shape.
    #[test]
    fn unknown_fields_are_ignored_without_changing_known_fields() {
        let settings = decode_settings(json!({
            "enabled": true,
            "futureFlag": 42,
            "customDir": "x",
            "selected": ["s"],
        }));
        assert_eq!(
            settings,
            AiSkillsSettings {
                enabled: true,
                auto_use: false,
                custom_dir: Some("x".to_owned()),
                selected: vec!["s".to_owned()],
            }
        );
    }

    // React: `if (!Array.isArray(value)) return []`.
    #[test]
    fn loaded_skills_rejects_non_arrays() {
        let now = "2026-06-01T00:00:00.000Z";
        for value in [
            Value::Null,
            json!({}),
            json!("skills"),
            json!(3),
            json!(true),
        ] {
            assert!(normalize_loaded_skills(&value, now).is_empty(), "{value:?}");
        }
    }

    // React drops non-object/null entries and blank names, keeps name verbatim.
    #[test]
    fn loaded_skills_keeps_only_object_entries_with_non_blank_names() {
        let now = "2026-06-01T00:00:00.000Z";
        let stored = json!([
            null,
            7,
            "junk",
            [],
            { "id": "bad", "name": "  " },
            { "name": "" },
            { "name": 3 },
            { "id": "keep", "name": "  spaced name  ", "source": "user", "loadedAt": "old" },
        ]);
        let loaded = normalize_loaded_skills(&stored, now);
        assert_eq!(loaded.len(), 1);
        assert_eq!(
            loaded[0],
            AiLoadedSkill {
                id: Some("keep".to_owned()),
                // The name is checked trimmed but stored verbatim.
                name: "  spaced name  ".to_owned(),
                source: Some("user".to_owned()),
                loaded_at: "old".to_owned(),
            }
        );
    }

    // React: non-string id/source → undefined; loadedAt → now ISO.
    #[test]
    fn loaded_skills_backfill_wrongly_typed_and_missing_legs() {
        let now = "2026-06-01T00:00:00.000Z";
        let stored = json!([
            { "id": 5, "name": "one", "source": false, "loadedAt": 9 },
            { "name": "two" },
            { "id": "", "name": "three", "source": "", "loadedAt": "" },
        ]);
        let loaded = normalize_loaded_skills(&stored, now);
        assert_eq!(
            loaded,
            vec![
                AiLoadedSkill {
                    id: None,
                    name: "one".to_owned(),
                    source: None,
                    loaded_at: now.to_owned(),
                },
                AiLoadedSkill {
                    id: None,
                    name: "two".to_owned(),
                    source: None,
                    loaded_at: now.to_owned(),
                },
                AiLoadedSkill {
                    id: Some(String::new()),
                    name: "three".to_owned(),
                    source: Some(String::new()),
                    loaded_at: String::new(),
                },
            ]
        );
    }

    // The sessions mirror (T230) must agree with this file's canonical
    // implementation — both restore paths read the same localStorage blob.
    #[test]
    fn the_sessions_mirror_agrees_with_the_canonical_normalizer() {
        let now = "2026-06-01T00:00:00.000Z";
        let stored = json!([
            { "id": "a", "name": "alpha", "source": "user", "loadedAt": "old" },
            { "name": "  " },
            { "id": 4, "name": "beta" },
        ]);
        let canonical = normalize_loaded_skills(&stored, now);
        let restored = crate::normalize_loaded_skills(Some(&stored), now);
        assert_eq!(canonical, restored);
    }

    #[test]
    fn the_react_surface_is_reexported_at_the_crate_root() {
        use crate::normalize_ai_skills_settings;
        let settings = normalize_ai_skills_settings(Some(&json!({"enabled": true})));
        assert!(settings.enabled);
        assert_eq!(settings.selected, Vec::<String>::new());
    }
}
