//! Shared single-turn AI use cases whose inputs come from host adapters.

use crate::ai::prompt_resolver::PromptResolver;
use crate::ai::response::{inject_language_hint, parse_ai_json};
use crate::error::FeatureError;
use datazen_ai_api::{
    AiProvider, ChatMessage, CompletionRequest, DiagnosisResult, ExplainAnalysis, MessageRole,
};
use datazen_driver_api::{DatabaseDriver, PromptScenario};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionDiagnosis {
    pub diagnosis: String,
    pub possible_causes: Vec<String>,
    pub solutions: Vec<ConnectionSolution>,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSolution {
    pub description: String,
    pub command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryAnalysis {
    pub summary: String,
    pub categories: Vec<QueryCategory>,
    pub insights: Vec<String>,
    pub frequent_tables: Vec<String>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryCategory {
    pub name: String,
    pub count: usize,
    pub examples: Vec<String>,
}

/// Diagnose SQL and database errors using a host-provided schema snapshot.
pub async fn diagnose_sql_error(
    provider: &dyn AiProvider,
    model: &str,
    prompt_resolver: &PromptResolver,
    driver: &dyn DatabaseDriver,
    lang: &str,
    schema_ddl: &str,
    sql: &str,
    error_message: &str,
) -> Result<DiagnosisResult, FeatureError> {
    let db_type = driver.driver_type();
    let mut vars = HashMap::new();
    vars.insert("db_type", db_type.as_str());
    vars.insert("schema", schema_ddl);
    let template = prompt_resolver
        .resolve(PromptScenario::Diagnose, Some(driver), lang)
        .await;
    let system_content = super::prompt_resolver::render_template(&template, &vars);
    let user_content = format!("SQL:\n```\n{sql}\n```\n\nError:\n{error_message}");
    let request = build_json_request(model, system_content, user_content, 0.0, lang);
    complete_json(provider, &request, "ai_diagnose_error").await
}

/// Analyze an EXPLAIN result. SQL text and plan output must be redacted by the
/// host before they cross this application boundary.
pub async fn analyze_explain(
    provider: &dyn AiProvider,
    model: &str,
    prompt_resolver: &PromptResolver,
    driver: &dyn DatabaseDriver,
    lang: &str,
    explain_output: &str,
    original_sql: &str,
) -> Result<ExplainAnalysis, FeatureError> {
    let db_type = driver.driver_type();
    let mut vars = HashMap::new();
    vars.insert("db_type", db_type.as_str());
    let template = prompt_resolver
        .resolve(PromptScenario::ExplainAnalysis, Some(driver), lang)
        .await;
    let system_content = super::prompt_resolver::render_template(&template, &vars);
    let user_content =
        format!("SQL:\n```\n{original_sql}\n```\n\nEXPLAIN output:\n```\n{explain_output}\n```");
    let request = build_json_request(model, system_content, user_content, 0.0, lang);
    complete_json(provider, &request, "ai_analyze_explain").await
}

/// Diagnose a connection using the already-sanitized profile summary prepared
/// by the host's profile repository adapter.
pub async fn diagnose_connection(
    provider: &dyn AiProvider,
    model: &str,
    prompt_resolver: &PromptResolver,
    lang: &str,
    connection_summary: &str,
    error_message: &str,
) -> Result<ConnectionDiagnosis, FeatureError> {
    let system_content = prompt_resolver
        .resolve(PromptScenario::ConnectionDiagnose, None, lang)
        .await;
    let user_content =
        format!("Connection details:\n{connection_summary}\n\nError:\n{error_message}");
    let request = build_json_request(model, system_content, user_content, 0.0, lang);
    complete_json(provider, &request, "ai_diagnose_connection").await
}

/// Summarize query history that the host has already filtered by its owner and
/// redacted according to the active egress gate.
pub async fn analyze_query_history(
    provider: &dyn AiProvider,
    model: &str,
    prompt_resolver: &PromptResolver,
    lang: &str,
    queries: &[String],
) -> Result<QueryAnalysis, FeatureError> {
    let (query_count, queries_text) = format_query_history(queries)?;
    let system_content = prompt_resolver
        .resolve(PromptScenario::QuerySummary, None, lang)
        .await;
    let user_content = format!("Analyze these {query_count} queries:\n\n{queries_text}");
    let request = build_json_request(model, system_content, user_content, 0.2, lang);
    complete_json(provider, &request, "ai_analyze_queries").await
}

fn format_query_history(queries: &[String]) -> Result<(usize, String), FeatureError> {
    if queries.is_empty() {
        return Err(FeatureError::Validation(
            "No query history available".into(),
        ));
    }
    let query_count = queries.len().min(100);
    let queries_text = queries
        .iter()
        .take(query_count)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("\n---\n");
    Ok((query_count, queries_text))
}

fn build_json_request(
    model: &str,
    system_content: String,
    user_content: String,
    temperature: f32,
    lang: &str,
) -> CompletionRequest {
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
                content: user_content,
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ],
        temperature: Some(temperature),
        stop: None,
        tools: None,
        previous_response_id: None,
        cancel_token: None,
    };
    inject_language_hint(&mut request.messages, lang);
    request
}

async fn complete_json<T: serde::de::DeserializeOwned>(
    provider: &dyn AiProvider,
    request: &CompletionRequest,
    operation: &str,
) -> Result<T, FeatureError> {
    let response = provider.complete(request).await?;
    parse_ai_json(
        &response.content,
        response.finish_reason.as_deref(),
        operation,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_history_prompt_caps_examples_at_one_hundred() {
        let queries = (0..150)
            .map(|index| format!("SELECT {index}"))
            .collect::<Vec<_>>();
        let (query_count, selected) = format_query_history(&queries).expect("history exists");
        let user_content = format!("Analyze these {query_count} queries:\n\n{selected}");

        assert!(user_content.starts_with("Analyze these 100 queries:"));
        assert!(user_content.contains("SELECT 99"));
        assert!(!user_content.contains("SELECT 100"));
    }

    #[test]
    fn query_history_policy_rejects_empty_history() {
        assert!(matches!(
            format_query_history(&[]),
            Err(FeatureError::Validation(message)) if message == "No query history available"
        ));
    }

    #[test]
    fn single_turn_request_injects_active_language() {
        let request = build_json_request("model", "system".into(), "user".into(), 0.0, "zh-CN");

        assert!(request.messages[0]
            .content
            .contains("all your natural language responses"));
        assert_eq!(request.messages[1].content, "user");
    }
}
