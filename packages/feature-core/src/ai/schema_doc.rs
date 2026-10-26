//! Portable AI-assisted schema documentation use case.

use crate::ai::budget;
use crate::ai::context::SchemaContextBuilder;
use crate::ai::prompt_resolver::PromptResolver;
use crate::ai::response::{inject_language_hint, strip_markdown_fences};
use crate::error::FeatureError;
use datazen_driver_api::{DatabaseDriver, PromptScenario};
use datazen_modules::ai::{AiProvider, ChatMessage, CompletionRequest, MessageRole};
use std::collections::HashMap;

const AUTO_SELECTION_LIMIT: usize = 30;
const FALLBACK_SELECTION_LIMIT: usize = 30;

/// Build a database overview document from shared schema metadata and AI output.
/// The host chooses the model/profile and supplies the active session adapter.
pub async fn generate_schema_doc(
    provider: &dyn AiProvider,
    model: &str,
    context_builder: &SchemaContextBuilder,
    prompt_resolver: &PromptResolver,
    driver: &dyn DatabaseDriver,
    db_session_id: &str,
    database: &str,
    lang: &str,
) -> Result<String, FeatureError> {
    let (db_type, all_table_names) = context_builder
        .get_table_names(db_session_id, database)
        .await?;

    // Avoid an extra model round trip for small schemas. Larger schemas use
    // the selection prompt; malformed output keeps the existing bounded,
    // system-table-filtered fallback policy.
    let selected_tables = if all_table_names.len() <= AUTO_SELECTION_LIMIT {
        all_table_names.clone()
    } else {
        let table_names = all_table_names.join(", ");
        let mut vars = HashMap::new();
        vars.insert("db_type", db_type.as_str());
        vars.insert("table_names", table_names.as_str());
        let template = prompt_resolver
            .resolve(PromptScenario::SchemaDocSelectTables, Some(driver), lang)
            .await;
        let system_content = super::prompt_resolver::render_template(&template, &vars);
        let request = CompletionRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            model: model.to_string(),
            messages: vec![
                ChatMessage {
                    role: MessageRole::System,
                    content: system_content,
                    reasoning: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
                ChatMessage {
                    role: MessageRole::User,
                    content: "Select the important user tables.".into(),
                    reasoning: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            temperature: Some(0.0),
            stop: None,
            tools: None,
            previous_response_id: None,
            cancel_token: None,
        };
        let response = provider.complete(&request).await?;
        select_schema_doc_tables(&all_table_names, Some(&response.content))
    };

    let context = context_builder
        .build_selective_context(
            db_session_id,
            database,
            &selected_tables,
            budget::SCHEMA_DOC,
        )
        .await?;
    let user_content = if lang.starts_with("zh") {
        "请为上面的数据库 schema 生成文档。"
    } else {
        "Generate documentation for the database schema above."
    };
    let mut vars = HashMap::new();
    vars.insert("db_type", context.database_type.as_str());
    vars.insert("schema", context.schema_ddl.as_str());
    let template = prompt_resolver
        .resolve(PromptScenario::SchemaDoc, Some(driver), lang)
        .await;
    let system_content = super::prompt_resolver::render_template(&template, &vars);

    let mut request = CompletionRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        model: model.to_string(),
        messages: vec![
            ChatMessage {
                role: MessageRole::System,
                content: system_content,
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: MessageRole::User,
                content: user_content.into(),
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ],
        temperature: Some(0.3),
        stop: None,
        tools: None,
        previous_response_id: None,
        cancel_token: None,
    };
    inject_language_hint(&mut request.messages, lang);

    let response = provider.complete(&request).await?;
    let content = response.content.trim();
    if content.starts_with("```markdown") || content.starts_with("```md") {
        Ok(strip_markdown_fences(content))
    } else {
        Ok(content.to_string())
    }
}

/// Select tables returned by the model, or apply the existing fallback when
/// its response cannot be parsed as a string array.
pub fn select_schema_doc_tables(
    all_table_names: &[String],
    model_selection: Option<&str>,
) -> Vec<String> {
    if all_table_names.len() <= AUTO_SELECTION_LIMIT {
        return all_table_names.to_vec();
    }

    if let Some(selected) = model_selection.and_then(parse_table_selection) {
        return selected;
    }

    all_table_names
        .iter()
        .filter(|name| {
            !name.starts_with("pg_")
                && !name.starts_with("sql_")
                && !name.starts_with("sqlite_")
                && !name.starts_with("information_schema")
        })
        .take(FALLBACK_SELECTION_LIMIT)
        .cloned()
        .collect()
}

fn parse_table_selection(raw: &str) -> Option<Vec<String>> {
    let normalized = strip_markdown_fences(raw.trim());
    serde_json::from_str::<Vec<String>>(&normalized).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_names(count: usize) -> Vec<String> {
        (0..count).map(|index| format!("table_{index}")).collect()
    }

    #[test]
    fn small_schema_uses_all_tables_without_model_selection() {
        let tables = table_names(AUTO_SELECTION_LIMIT);
        assert_eq!(select_schema_doc_tables(&tables, None), tables);
    }

    #[test]
    fn valid_fenced_selection_is_used_for_large_schema() {
        let tables = table_names(AUTO_SELECTION_LIMIT + 1);

        assert_eq!(
            select_schema_doc_tables(&tables, Some("```json\n[\"users\", \"orders\"]\n```")),
            vec!["users".to_string(), "orders".to_string()]
        );
    }

    #[test]
    fn invalid_selection_falls_back_to_bounded_user_tables() {
        let mut tables = vec![
            "pg_catalog".into(),
            "sql_metadata".into(),
            "sqlite_master".into(),
            "information_schema.tables".into(),
        ];
        tables.extend(table_names(FALLBACK_SELECTION_LIMIT + 1));

        let selected = select_schema_doc_tables(&tables, Some("not json"));

        assert_eq!(selected.len(), FALLBACK_SELECTION_LIMIT);
        assert_eq!(selected.first().map(String::as_str), Some("table_0"));
        assert!(!selected.iter().any(|name| name.starts_with("pg_")));
    }
}
