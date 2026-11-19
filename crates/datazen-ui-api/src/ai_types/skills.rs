//! Skills settings and skill listings — React `types/ai.ts` lines 85–116.
//!
//! `lib/aiSkills.ts` (tasklist T230) is the behavior half (directory
//! scanning, `normalizeLoadedSkills`); this file carries only the type
//! contracts and the `defaultAiSkillsSettings` constant.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
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
