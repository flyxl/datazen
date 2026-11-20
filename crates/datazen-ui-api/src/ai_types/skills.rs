//! Skills settings and skill listings — React `types/ai.ts` lines 85–116,
//! plus the two pure normalizers of `lib/aiSkills.ts` (T228's dependency
//! leg, ported with it: the directory-scanning half of that file stays with
//! the host-side port that owns the filesystem).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::chat::AiLoadedSkill;

/// React `AiSkillsSettings`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSkillsSettings {
    /// Master switch; skills remain invisible and untoolable when false.
    pub enabled: bool,
    /// One optional directory in addition to `~/.agents/skills`.
    /// `string | null` (required-but-nullable): `None` serializes as an
    /// explicit `null`, and the key must be present on the wire.
    pub custom_dir: Option<String>,
    /// Off by default: no skill listing/body is injected automatically.
    pub auto_use: bool,
    /// Explicitly selected stable skill ids used while `autoUse` is false.
    pub selected: Vec<String>,
}

/// React `defaultAiSkillsSettings` — the exact literal object.
pub const DEFAULT_AI_SKILLS_SETTINGS: AiSkillsSettings = AiSkillsSettings {
    enabled: false,
    custom_dir: None,
    auto_use: false,
    selected: Vec::new(),
};

/// React `AiSkillSummary.source = 'user' | 'custom'` — where the skill was
/// found: the user directory or the custom directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiSkillSource {
    User,
    Custom,
}

impl AiSkillSource {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Custom => "custom",
        }
    }
}

/// React `AiSkillSummary` — one discoverable skill, as the listing shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSkillSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: AiSkillSource,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// React `AiSkillContent extends AiSkillSummary` — the summary plus the
/// loaded body. React expresses this with interface extension; the wire
/// JSON carries the base fields at the top level, so the Rust twin
/// flattens them (same shape `AiSessionDetail` takes over
/// `AiSessionSummary` in [`super::chat`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSkillContent {
    // ── AiSkillSummary (flattened base) ──
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: AiSkillSource,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    // ── AiSkillContent's own fields ──
    pub content: String,
    /// Resource paths declared by the skill (`resources/` entries).
    pub resources: Vec<String>,
    /// Front-matter metadata. `Record<string, string>` → `BTreeMap`: JS
    /// object key order is insertion order, but this map is looked up by
    /// key, so the sorted iteration is inert on the wire semantic.
    pub metadata: BTreeMap<String, String>,
}


/// React `normalizeAiSkillsSettings` — fill fields omitted by configs
/// written before T17: the defaults carry, the booleans only trust `true`,
/// `customDir` only a string, and `selected` keeps its first occurrence of
/// each string (a `new Set` over the string filter — duplicate ids and
/// non-string junk die).
pub fn normalize_ai_skills_settings(skills: Option<&serde_json::Value>) -> AiSkillsSettings {
    let mut out = DEFAULT_AI_SKILLS_SETTINGS;
    let Some(obj) = skills.and_then(serde_json::Value::as_object) else {
        return out;
    };
    out.enabled = obj.get("enabled") == Some(&serde_json::Value::Bool(true));
    out.auto_use = obj.get("autoUse") == Some(&serde_json::Value::Bool(true));
    out.custom_dir = match obj.get("customDir") {
        Some(serde_json::Value::String(dir)) => Some(dir.clone()),
        _ => None,
    };
    out.selected = match obj.get("selected") {
        Some(serde_json::Value::Array(items)) => {
            let mut seen = Vec::new();
            for item in items {
                if let serde_json::Value::String(id) = item {
                    if !seen.contains(id) {
                        seen.push(id.clone());
                    }
                }
            }
            seen
        }
        _ => Vec::new(),
    };
    out
}

/// React `normalizeLoadedSkills` — read session metadata written by this
/// version or an older one: an array of entries keeps the ones with a
/// non-blank `name`, filling `id`/`source`/`loadedAt` legs that are absent
/// or not strings (`loadedAt`'s `new Date().toISOString()` default takes the
/// caller-supplied now — this crate is wasm-pure with no clock of its own,
/// the `MetadataClock` ruling).
pub fn normalize_loaded_skills(value: &serde_json::Value, now_iso: &str) -> Vec<AiLoadedSkill> {
    let Some(entries) = value.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries {
        let Some(record) = entry.as_object() else {
            continue;
        };
        let name = match record.get("name") {
            Some(serde_json::Value::String(name)) if !name.trim().is_empty() => name.clone(),
            _ => continue,
        };
        let string_leg = |key: &str| -> Option<String> {
            match record.get(key) {
                Some(serde_json::Value::String(value)) => Some(value.clone()),
                _ => None,
            }
        };
        let loaded_at = record
            .get("loadedAt")
            .and_then(|value| value.as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| now_iso.to_owned());
        out.push(AiLoadedSkill {
            id: string_leg("id"),
            name,
            source: string_leg("source"),
            loaded_at,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The React suite (`__tests__/aiSkills.test.ts`) — T228's dependency
    /// leg carries this file's two functions, so its suite ports here.
    #[test]
    fn defaults_and_validates_selected_stable_ids() {
        assert_eq!(normalize_ai_skills_settings(None).selected, Vec::<String>::new());
        let stored = serde_json::json!({
            "enabled": true,
            "customDir": null,
            "autoUse": false,
            "selected": ["user:one", "user:one", 3],
        });
        let normalized = normalize_ai_skills_settings(Some(&stored));
        assert_eq!(normalized.selected, vec!["user:one".to_owned()]);
        assert!(normalized.enabled);
        assert!(!normalized.auto_use);
        assert_eq!(normalized.custom_dir, None);
    }

    #[test]
    fn keeps_skill_ids_when_restoring_a_persisted_session() {
        let stored = serde_json::json!([
            { "id": "user:one", "name": "one", "source": "user", "loadedAt": "2026-01-01T00:00:00Z" },
            { "name": "legacy", "loadedAt": "2026-01-01T00:00:00Z" },
        ]);
        let loaded = normalize_loaded_skills(&stored, "2026-06-01T00:00:00Z");
        assert_eq!(
            loaded,
            vec![
                AiLoadedSkill {
                    id: Some("user:one".into()),
                    name: "one".into(),
                    source: Some("user".into()),
                    loaded_at: "2026-01-01T00:00:00Z".into(),
                },
                AiLoadedSkill {
                    id: None,
                    name: "legacy".into(),
                    source: None,
                    loaded_at: "2026-01-01T00:00:00Z".into(),
                },
            ]
        );
    }

    /// Beyond the React suite — the blank-name rejection and the
    /// now-iso default leg.
    #[test]
    fn a_blank_name_dies_and_the_now_leg_backfills_loadedat() {
        let stored = serde_json::json!([
            { "name": "  " },
            { "id": 5, "name": "keep" },
            "junk",
        ]);
        let loaded = normalize_loaded_skills(&stored, "2026-06-01T00:00:00Z");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "keep");
        assert_eq!(loaded[0].id, None, "a non-string id dies to None");
        assert_eq!(loaded[0].loaded_at, "2026-06-01T00:00:00Z");
    }

    use super::*;

    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire)
            .unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    #[test]
    fn skill_sources_keep_the_react_literals() {
        assert_eq!(AiSkillSource::User.as_str(), "user");
        assert_eq!(AiSkillSource::Custom.as_str(), "custom");
        for (literal, value) in [("user", AiSkillSource::User), ("custom", AiSkillSource::Custom)] {
            let parsed: AiSkillSource = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(serde_json::to_value(parsed).ok(), Some(serde_json::json!(literal)));
        }
    }

    /// The default constant is React's literal object, including the
    /// explicit-null `customDir`.
    #[test]
    fn default_skills_settings_match_the_react_constant() {
        assert_eq!(DEFAULT_AI_SKILLS_SETTINGS.enabled, false);
        assert_eq!(DEFAULT_AI_SKILLS_SETTINGS.custom_dir, None);
        assert_eq!(DEFAULT_AI_SKILLS_SETTINGS.auto_use, false);
        assert!(DEFAULT_AI_SKILLS_SETTINGS.selected.is_empty());
        assert_eq!(
            serde_json::to_value(DEFAULT_AI_SKILLS_SETTINGS.clone()).ok(),
            Some(serde_json::json!({
                "enabled": false,
                "customDir": null,
                "autoUse": false,
                "selected": []
            }))
        );
        assert_eq!(AiSkillsSettings::default(), DEFAULT_AI_SKILLS_SETTINGS);
    }

    #[test]
    fn a_skill_summary_round_trips_with_an_optional_version() {
        let wire = serde_json::json!({
            "id": "sql-helper",
            "name": "SQL helper",
            "description": "Writes careful SQL.",
            "source": "user",
            "path": "/home/u/.agents/skills/sql-helper/SKILL.md"
        });
        let summary: AiSkillSummary = decode(wire.clone());
        assert_eq!(summary.source, AiSkillSource::User);
        assert_eq!(summary.version, None);
        assert_eq!(serde_json::to_value(&summary).ok(), Some(wire));

        let with_version: AiSkillSummary = decode(serde_json::json!({
            "id": "sql-helper",
            "name": "SQL helper",
            "description": "Writes careful SQL.",
            "source": "custom",
            "path": "/opt/skills/sql-helper/SKILL.md",
            "version": "1.2.0"
        }));
        assert_eq!(with_version.source, AiSkillSource::Custom);
        assert_eq!(with_version.version.as_deref(), Some("1.2.0"));
    }

    #[test]
    fn skill_content_flattens_the_summary_fields_on_the_wire() {
        let wire = serde_json::json!({
            "id": "sql-helper",
            "name": "SQL helper",
            "description": "Writes careful SQL.",
            "source": "user",
            "path": "/home/u/.agents/skills/sql-helper/SKILL.md",
            "version": "1.2.0",
            "content": "---\nname: SQL helper\n---\nBody.",
            "resources": ["refs/schema.md"],
            "metadata": { "license": "MIT", "author": "u" }
        });
        let content: AiSkillContent = decode(wire.clone());
        assert_eq!(content.source, AiSkillSource::User);
        assert_eq!(content.version.as_deref(), Some("1.2.0"));
        assert_eq!(content.resources, vec!["refs/schema.md".to_string()]);
        assert_eq!(content.metadata.get("license").map(String::as_str), Some("MIT"));
        // The flattened base round-trips at the top level, not nested.
        assert_eq!(serde_json::to_value(&content).ok(), Some(wire));
    }
}
