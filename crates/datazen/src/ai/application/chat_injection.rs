//! User-authored prompt material for chat: global instructions, a selected
//! Prompt template, and connection notes.
//!
//! Resolution is pure and lives here so both hosts and tests see the same
//! ordering/material. Actual insertion into the system prompt happens in
//! `chat_request`; this module only decides *what* is injected and records the
//! non-secret snapshot sent back to the UI for chips.

use crate::ai::domain::types::{AiSettingsConfig, PromptTemplate};
use serde::{Deserialize, Serialize};

/// Resolved material for one chat turn. Empty strings/`None` mean "not
/// injected", so an unconfigured install keeps its previous prompt shape.
#[derive(Debug, Clone, Default)]
pub struct ChatInjection {
    pub global_instructions: String,
    pub template: Option<PromptTemplate>,
    pub connection_notes: Option<String>,
}

/// UI-facing snapshot of what was injected. It deliberately carries no
/// instruction text or note body — only identity and existence.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatInjectionSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_hash: Option<String>,
    #[serde(default)]
    pub has_connection_notes: bool,
    #[serde(default)]
    pub has_global_instructions: bool,
}

impl ChatInjection {
    pub fn snapshot(&self) -> ChatInjectionSnapshot {
        let template = self
            .template
            .as_ref()
            .filter(|template| !template.content.trim().is_empty());
        ChatInjectionSnapshot {
            template_id: template.map(|template| template.id.clone()),
            template_name: template.map(|template| template.name.clone()),
            template_hash: template.map(|template| content_hash(&template.content)),
            has_connection_notes: self
                .connection_notes
                .as_deref()
                .is_some_and(|notes| !notes.trim().is_empty()),
            has_global_instructions: !self.global_instructions.trim().is_empty(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.global_instructions.trim().is_empty()
            && self
                .template
                .as_ref()
                .is_none_or(|template| template.content.trim().is_empty())
            && self
                .connection_notes
                .as_deref()
                .is_none_or(|notes| notes.trim().is_empty())
    }
}

/// Resolve one turn's prompt material. An explicit `template_id` wins even if
/// the database type would have selected another default; an unknown id falls
/// back to the normal default selection rather than silently dropping the
/// request.
pub fn resolve_chat_injection(
    settings: &AiSettingsConfig,
    template_id: Option<&str>,
    database_type: Option<&str>,
    connection_notes: Option<String>,
) -> ChatInjection {
    let template = settings
        .select_prompt_template(template_id, database_type)
        .cloned();
    ChatInjection {
        global_instructions: settings.global_instructions.trim().to_string(),
        template,
        connection_notes: connection_notes
            .map(|notes| notes.trim().to_string())
            .filter(|notes| !notes.is_empty()),
    }
}

/// Stable FNV-1a hash for the UI snapshot. This is an identity hint, not a
/// security primitive; it lets the client tell two template revisions apart
/// without receiving the template body again.
pub fn content_hash(content: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in content.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template(id: &str, db: &[&str], default: bool) -> PromptTemplate {
        PromptTemplate {
            id: id.into(),
            name: format!("Template {id}"),
            content: format!("content-{id}"),
            database_types: db.iter().map(|value| (*value).to_string()).collect(),
            is_default: default,
        }
    }

    #[test]
    fn explicit_template_wins_over_database_default() {
        let settings = AiSettingsConfig {
            prompt_templates: vec![
                template("pg", &["postgresql"], true),
                template("explicit", &["mysql"], false),
            ],
            ..Default::default()
        };
        let injection =
            resolve_chat_injection(&settings, Some("explicit"), Some("postgresql"), None);
        assert_eq!(injection.template.map(|t| t.id), Some("explicit".into()));
    }

    #[test]
    fn database_default_then_generic_default() {
        let settings = AiSettingsConfig {
            prompt_templates: vec![
                template("generic", &[], true),
                template("mysql", &["mysql"], true),
                template("pg", &["postgresql"], true),
            ],
            ..Default::default()
        };
        assert_eq!(
            settings
                .select_prompt_template(None, Some("postgresql"))
                .map(|t| t.id.as_str()),
            Some("pg")
        );
        assert_eq!(
            settings
                .select_prompt_template(None, Some("sqlite"))
                .map(|t| t.id.as_str()),
            Some("generic")
        );
        assert_eq!(
            settings
                .select_prompt_template(None, None)
                .map(|t| t.id.as_str()),
            Some("generic")
        );
    }

    #[test]
    fn non_matching_database_default_is_not_injected_without_a_generic_default() {
        let settings = AiSettingsConfig {
            prompt_templates: vec![template("mysql-only", &["mysql"], true)],
            ..Default::default()
        };
        assert!(settings
            .select_prompt_template(None, Some("postgresql"))
            .is_none());
        assert!(settings.select_prompt_template(None, None).is_none());
        assert_eq!(
            settings
                .select_prompt_template(None, Some("mysql"))
                .map(|t| t.id.as_str()),
            Some("mysql-only")
        );
    }

    #[test]
    fn generic_default_still_covers_an_unmatched_or_absent_database_type() {
        let settings = AiSettingsConfig {
            prompt_templates: vec![
                template("mysql-only", &["mysql"], true),
                template("generic", &[], true),
            ],
            ..Default::default()
        };
        assert_eq!(
            settings
                .select_prompt_template(None, Some("postgresql"))
                .map(|t| t.id.as_str()),
            Some("generic")
        );
        assert_eq!(
            settings
                .select_prompt_template(None, None)
                .map(|t| t.id.as_str()),
            Some("generic")
        );
    }

    #[test]
    fn unknown_explicit_id_falls_back_to_default() {
        let settings = AiSettingsConfig {
            prompt_templates: vec![template("generic", &[], true)],
            ..Default::default()
        };
        assert_eq!(
            settings
                .select_prompt_template(Some("missing"), None)
                .map(|t| t.id.as_str()),
            Some("generic")
        );
    }

    #[test]
    fn snapshot_has_identity_but_not_content() {
        let settings = AiSettingsConfig {
            global_instructions: "Use UTC.".into(),
            prompt_templates: vec![template("pg", &["postgresql"], true)],
            ..Default::default()
        };
        let injection = resolve_chat_injection(
            &settings,
            None,
            Some("postgresql"),
            Some("customer name means account".into()),
        );
        let snapshot = injection.snapshot();
        assert_eq!(snapshot.template_id.as_deref(), Some("pg"));
        assert_eq!(snapshot.template_name.as_deref(), Some("Template pg"));
        assert_eq!(
            snapshot.template_hash.as_deref(),
            Some(content_hash("content-pg").as_str())
        );
        assert!(snapshot.has_connection_notes);
        assert!(snapshot.has_global_instructions);
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(!json.contains("content-pg"));
        assert!(!json.contains("customer name"));
    }

    #[test]
    fn blank_material_is_not_injected() {
        let settings = AiSettingsConfig {
            global_instructions: "   ".into(),
            prompt_templates: vec![PromptTemplate {
                id: "blank".into(),
                name: "Blank".into(),
                content: "  ".into(),
                database_types: Vec::new(),
                is_default: true,
            }],
            ..Default::default()
        };
        let injection = resolve_chat_injection(&settings, None, None, Some("  ".into()));
        assert!(injection.is_empty());
        let snapshot = injection.snapshot();
        assert!(snapshot.template_id.is_none());
        assert!(!snapshot.has_connection_notes);
        assert!(!snapshot.has_global_instructions);
    }
}
