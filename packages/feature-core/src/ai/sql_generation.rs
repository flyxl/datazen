//! Shared SQL-generation request construction. Streaming and cancellation are
//! left to the active platform adapter.

use crate::ai::prompt_resolver::PromptResolver;
use crate::ai::response::inject_language_hint;
use crate::ai::safety::redact_for_gate as redact_text;
use crate::ai::schema_pipeline::{compose_schema_system_suffix, PromptSeed};
use datazen_driver_api::{DatabaseDriver, PromptScenario};
use datazen_modules::ai::{AiSafetyGateConfig, ChatMessage, CompletionRequest, MessageRole};
use std::collections::HashMap;

/// Assemble the portable NL2SQL prompt from resolved schema context and
/// host-redacted user content. The caller attaches tool definitions, stream
/// cancellation and UI delivery.
pub async fn build_sql_generation_request(
    request_id: &str,
    model: &str,
    prompt_resolver: &PromptResolver,
    driver: &dyn DatabaseDriver,
    lang: &str,
    schema_seed: &PromptSeed,
    natural_language: String,
    recent_queries: &[String],
    safety_gate: &AiSafetyGateConfig,
) -> CompletionRequest {
    let schema_suffix = compose_schema_system_suffix(schema_seed);
    let recent = if recent_queries.is_empty() {
        String::new()
    } else {
        let label = if lang.starts_with("zh") {
            "近期查询（供风格参考）"
        } else {
            "Recent queries (for style reference)"
        };
        format!(
            "\n\n{label}:\n{}",
            recent_queries
                .iter()
                .map(|query| format!("- {}", redact_text(query, safety_gate)))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    let mut vars = HashMap::new();
    vars.insert("db_type", schema_seed.database_type.as_str());
    vars.insert("version", "");
    vars.insert("schema", schema_suffix.as_str());
    vars.insert("recent", recent.as_str());
    let template = prompt_resolver
        .resolve(PromptScenario::Nl2Sql, Some(driver), lang)
        .await;
    let system_content = super::prompt_resolver::render_template(&template, &vars);
    let mut request = CompletionRequest {
        request_id: request_id.to_string(),
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
                content: natural_language,
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
    inject_language_hint(&mut request.messages, lang);
    request
}
