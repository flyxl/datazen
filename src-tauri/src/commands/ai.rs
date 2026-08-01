//! AI-related Tauri IPC commands.

use crate::ai::*;
use crate::commands::{log_err, AppState};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderListItem {
    pub provider_type: AiProviderType,
    pub display_name: String,
    pub models: Vec<ModelInfo>,
    pub default_model: String,
    pub supports_streaming: bool,
    pub supports_tools: bool,
}

#[tauri::command]
pub async fn ai_get_providers(
    state: State<'_, AppState>,
) -> Result<Vec<ProviderListItem>, String> {
    let providers = state.ai_registry.all_providers().await;
    let result = providers
        .into_iter()
        .map(|p| ProviderListItem {
            provider_type: p.provider_type(),
            display_name: p.display_name().to_string(),
            models: p.available_models(),
            default_model: p.default_model().to_string(),
            supports_streaming: p.supports_streaming(),
            supports_tools: p.supports_tools(),
        })
        .collect();
    Ok(result)
}

#[tauri::command]
pub async fn ai_get_models(
    state: State<'_, AppState>,
    provider_type: AiProviderType,
) -> Result<Vec<ModelInfo>, String> {
    let provider = state
        .ai_registry
        .get(&provider_type)
        .await
        .ok_or_else(|| format!("Provider {:?} not found", provider_type))?;
    Ok(provider.available_models())
}

#[tauri::command]
pub async fn ai_validate_config(
    state: State<'_, AppState>,
    config: AiProviderConfig,
) -> Result<(), String> {
    let provider = state
        .ai_registry
        .get(&config.provider_type)
        .await
        .ok_or_else(|| format!("Provider {:?} not found", config.provider_type))?;
    provider
        .validate_config(&config)
        .await
        .map_err(|e| log_err("ai_validate_config", &e))
}

#[tauri::command]
pub async fn ai_save_config(
    state: State<'_, AppState>,
    config: AiProviderConfig,
) -> Result<(), String> {
    let provider = state
        .ai_registry
        .get(&config.provider_type)
        .await
        .ok_or_else(|| format!("Provider {:?} not found", config.provider_type))?;

    provider
        .initialize(&config)
        .await
        .map_err(|e| log_err("ai_save_config", &e))?;

    state
        .store
        .save_ai_config(&config)
        .await
        .map_err(|e| log_err("ai_save_config", &e))
}

#[tauri::command]
pub async fn ai_get_config(
    state: State<'_, AppState>,
) -> Result<Option<AiProviderConfig>, String> {
    Ok(state.store.get_ai_config().await)
}

#[tauri::command]
pub async fn ai_delete_config(
    state: State<'_, AppState>,
) -> Result<(), String> {
    for provider in state.ai_registry.all_providers().await {
        provider.reset().await;
    }
    state
        .store
        .delete_ai_config()
        .await
        .map_err(|e| log_err("ai_delete_config", &e))
}

// ─── NL2SQL ───

#[tauri::command]
pub async fn ai_generate_sql(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    connection_id: String,
    database: String,
    natural_language: String,
    request_id: String,
    current_table: Option<String>,
    recent_queries: Vec<String>,
) -> Result<String, String> {
    let ai_config = state
        .store
        .get_ai_config()
        .await
        .ok_or_else(|| "AI 未配置".to_string())?;

    let provider = state
        .ai_registry
        .get(&ai_config.provider_type)
        .await
        .ok_or("Provider not available")?;

    let context = state
        .schema_context_builder
        .build_sql_context(
            &connection_id,
            &database,
            current_table.as_deref(),
            &recent_queries,
            4000,
        )
        .await
        .map_err(|e| log_err("ai_generate_sql", &e))?;

    let system_msg = PromptBuilder::nl2sql_system(&context);
    let user_msg = ChatMessage {
        role: MessageRole::User,
        content: natural_language,
    };

    let (tx, mut rx) = mpsc::channel::<Result<StreamChunk, AiError>>(32);
    let request = CompletionRequest {
        request_id: request_id.clone(),
        model: ai_config.model.clone(),
        messages: vec![system_msg, user_msg],
        temperature: Some(0.0),
        max_tokens: Some(2000),
        stop: None,
    };

    let req_id_clone = request_id.clone();
    let handle_clone = app_handle.clone();

    tokio::spawn(async move {
        while let Some(chunk_result) = rx.recv().await {
            match chunk_result {
                Ok(chunk) => {
                    let _ = handle_clone.emit(
                        "ai:stream-chunk",
                        serde_json::json!({
                            "requestId": req_id_clone,
                            "content": chunk.content,
                            "done": chunk.done,
                            "usage": chunk.usage,
                        }),
                    );
                }
                Err(e) => {
                    let _ = handle_clone.emit(
                        "ai:stream-error",
                        serde_json::json!({
                            "requestId": req_id_clone,
                            "error": e.to_string(),
                        }),
                    );
                }
            }
        }
    });

    provider
        .stream_complete(&request, tx)
        .await
        .map_err(|e| log_err("ai_generate_sql", &e))?;

    Ok(request_id)
}

// ─── SQL Error Diagnosis ───

#[tauri::command]
pub async fn ai_diagnose_error(
    state: State<'_, AppState>,
    connection_id: String,
    database: String,
    sql: String,
    error_message: String,
) -> Result<DiagnosisResult, String> {
    let ai_config = state
        .store
        .get_ai_config()
        .await
        .ok_or_else(|| "AI 未配置".to_string())?;

    let provider = state
        .ai_registry
        .get(&ai_config.provider_type)
        .await
        .ok_or("Provider not available")?;

    let context = state
        .schema_context_builder
        .build_sql_context(&connection_id, &database, None, &[], 3000)
        .await
        .map_err(|e| log_err("ai_diagnose_error", &e))?;

    let request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
        messages: vec![
            PromptBuilder::diagnose_system(&context.database_type, &context.schema_ddl),
            ChatMessage {
                role: MessageRole::User,
                content: format!("SQL:\n```\n{sql}\n```\n\nError:\n{error_message}"),
            },
        ],
        temperature: Some(0.0),
        max_tokens: Some(1500),
        stop: None,
    };

    let response = provider
        .complete(&request)
        .await
        .map_err(|e| log_err("ai_diagnose_error", &e))?;

    let content = strip_markdown_fences(&response.content);
    serde_json::from_str::<DiagnosisResult>(&content)
        .map_err(|e| log_err("ai_diagnose_error", &e))
}

// ─── EXPLAIN Analysis ───

#[tauri::command]
pub async fn ai_analyze_explain(
    state: State<'_, AppState>,
    connection_id: String,
    explain_output: String,
    original_sql: String,
) -> Result<ExplainAnalysis, String> {
    let ai_config = state
        .store
        .get_ai_config()
        .await
        .ok_or_else(|| "AI 未配置".to_string())?;

    let provider = state
        .ai_registry
        .get(&ai_config.provider_type)
        .await
        .ok_or("Provider not available")?;

    let (driver, _) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("ai_analyze_explain", &e))?;

    let db_type = format!("{:?}", driver.driver_type());

    let request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
        messages: vec![
            PromptBuilder::explain_analysis_system(&db_type),
            ChatMessage {
                role: MessageRole::User,
                content: format!(
                    "SQL:\n```\n{original_sql}\n```\n\nEXPLAIN output:\n```\n{explain_output}\n```"
                ),
            },
        ],
        temperature: Some(0.0),
        max_tokens: Some(2000),
        stop: None,
    };

    let response = provider
        .complete(&request)
        .await
        .map_err(|e| log_err("ai_analyze_explain", &e))?;

    let content = strip_markdown_fences(&response.content);
    serde_json::from_str::<ExplainAnalysis>(&content)
        .map_err(|e| log_err("ai_analyze_explain", &e))
}

// ─── Smart Filter ───

#[tauri::command]
pub async fn ai_parse_filter(
    state: State<'_, AppState>,
    connection_id: String,
    database: String,
    table: String,
    natural_language: String,
) -> Result<Vec<crate::services::query_executor::FilterCondition>, String> {
    let ai_config = state
        .store
        .get_ai_config()
        .await
        .ok_or_else(|| "AI 未配置".to_string())?;

    let provider = state
        .ai_registry
        .get(&ai_config.provider_type)
        .await
        .ok_or("Provider not available")?;

    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("ai_parse_filter", &e))?;

    let db_type = format!("{:?}", driver.driver_type());

    let cached = state
        .schema_cache
        .get_columns(&connection_id, &database, &table, &driver, &handle)
        .await
        .map_err(|e| log_err("ai_parse_filter", &e))?;

    let columns_ddl = cached
        .columns
        .iter()
        .map(|c| {
            let nullable = if c.nullable { " NULL" } else { " NOT NULL" };
            let pk = if c.is_primary_key { " PK" } else { "" };
            format!("  {} {}{}{}", c.name, c.data_type, nullable, pk)
        })
        .collect::<Vec<_>>()
        .join("\n");

    let request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
        messages: vec![
            PromptBuilder::nl_filter_system(&db_type, &columns_ddl),
            ChatMessage {
                role: MessageRole::User,
                content: natural_language,
            },
        ],
        temperature: Some(0.0),
        max_tokens: Some(1000),
        stop: None,
    };

    let response = provider
        .complete(&request)
        .await
        .map_err(|e| log_err("ai_parse_filter", &e))?;

    let content = strip_markdown_fences(&response.content);
    let mut filters: Vec<crate::services::query_executor::FilterCondition> =
        serde_json::from_str(&content).map_err(|e| log_err("ai_parse_filter", &e))?;

    let valid_columns: std::collections::HashSet<String> =
        cached.columns.iter().map(|c| c.name.clone()).collect();

    filters.retain(|f| valid_columns.contains(&f.column));

    use crate::services::query_executor::FilterOperator;
    use datazen_driver_api::Value;
    for f in &mut filters {
        match (&f.operator, &f.value) {
            (FilterOperator::Eq, Value::Null) => f.operator = FilterOperator::IsNull,
            (FilterOperator::Ne, Value::Null) => f.operator = FilterOperator::IsNotNull,
            _ => {}
        }
    }

    Ok(filters)
}

// ─── AI Chat ───

#[tauri::command]
pub async fn ai_chat(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    connection_id: Option<String>,
    database: Option<String>,
    messages: Vec<ChatMessage>,
    request_id: String,
    include_schema: bool,
) -> Result<String, String> {
    let ai_config = state
        .store
        .get_ai_config()
        .await
        .ok_or_else(|| "AI 未配置".to_string())?;

    let provider = state
        .ai_registry
        .get(&ai_config.provider_type)
        .await
        .ok_or("Provider not available")?;

    let mut full_messages: Vec<ChatMessage> = Vec::new();

    if include_schema {
        if let Some(ref conn_id) = connection_id {
            let db = database.as_deref().unwrap_or("");
            if let Ok((driver, _handle)) =
                state.connection_manager.get_connection(conn_id).await
            {
                let db_type = format!("{:?}", driver.driver_type());
                if let Ok(context) = state
                    .schema_context_builder
                    .build_sql_context(conn_id, db, None, &[], 4000)
                    .await
                {
                    full_messages.push(ChatMessage {
                        role: MessageRole::System,
                        content: format!(
                            "You are a helpful database assistant. The user is connected to a {db_type} database.\n\nSchema:\n{}",
                            context.schema_ddl
                        ),
                    });
                }
            }
        }
    }

    if full_messages.is_empty() {
        full_messages.push(ChatMessage {
            role: MessageRole::System,
            content: "You are a helpful database assistant. Help the user with SQL queries, database concepts, and data analysis. When writing SQL, use proper formatting and explain your reasoning.".to_string(),
        });
    }

    full_messages.extend(messages);

    let (tx, mut rx) = mpsc::channel::<Result<StreamChunk, AiError>>(32);
    let request = CompletionRequest {
        request_id: request_id.clone(),
        model: ai_config.model.clone(),
        messages: full_messages,
        temperature: Some(0.7),
        max_tokens: Some(4000),
        stop: None,
    };

    let req_id_clone = request_id.clone();
    let handle_clone = app_handle.clone();

    tokio::spawn(async move {
        while let Some(chunk_result) = rx.recv().await {
            match chunk_result {
                Ok(chunk) => {
                    let _ = handle_clone.emit(
                        "ai:stream-chunk",
                        serde_json::json!({
                            "requestId": req_id_clone,
                            "content": chunk.content,
                            "done": chunk.done,
                            "usage": chunk.usage,
                        }),
                    );
                }
                Err(e) => {
                    let _ = handle_clone.emit(
                        "ai:stream-error",
                        serde_json::json!({
                            "requestId": req_id_clone,
                            "error": e.to_string(),
                        }),
                    );
                }
            }
        }
    });

    provider
        .stream_complete(&request, tx)
        .await
        .map_err(|e| log_err("ai_chat", &e))?;

    Ok(request_id)
}

fn strip_markdown_fences(s: &str) -> String {
    let trimmed = s.trim();
    if let Some(rest) = trimmed.strip_prefix("```") {
        let body = rest
            .strip_prefix("json")
            .or_else(|| rest.strip_prefix("JSON"))
            .unwrap_or(rest);
        if let Some(end) = body.rfind("```") {
            return body[..end].trim().to_string();
        }
    }
    trimmed.to_string()
}

// ─── Skill IPC commands ───

#[tauri::command]
pub async fn skill_list(
    state: State<'_, AppState>,
) -> Result<Vec<crate::mcp::SkillListItem>, String> {
    Ok(state.skill_registry.list().await)
}

#[tauri::command]
pub async fn skill_execute(
    state: State<'_, AppState>,
    skill_id: String,
    variables: serde_json::Value,
    connection_id: Option<String>,
) -> Result<String, String> {
    let skill = state
        .skill_registry
        .get(&skill_id)
        .await
        .ok_or_else(|| format!("Skill '{skill_id}' not found"))?;

    crate::mcp::SkillExecutor::execute(
        &skill,
        &state,
        connection_id.as_deref(),
        &variables,
    )
    .await
    .map_err(|e| log_err("skill_execute", &e))
}

#[tauri::command]
pub async fn skill_save(
    state: State<'_, AppState>,
    skill: crate::mcp::SkillDefinition,
) -> Result<(), String> {
    state.skill_registry.save_skill(&skill).await
}

#[tauri::command]
pub async fn skill_delete(
    state: State<'_, AppState>,
    skill_id: String,
) -> Result<(), String> {
    state.skill_registry.delete_skill(&skill_id).await
}

#[tauri::command]
pub async fn skill_reload(state: State<'_, AppState>) -> Result<(), String> {
    state.skill_registry.load_all().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_list_item_serialization() {
        let item = ProviderListItem {
            provider_type: AiProviderType::OpenAi,
            display_name: "OpenAI".into(),
            models: vec![ModelInfo {
                id: "gpt-4o".into(),
                display_name: "GPT-4o".into(),
                context_window: 128_000,
                supports_streaming: true,
                supports_tools: true,
            }],
            default_model: "gpt-4o".into(),
            supports_streaming: true,
            supports_tools: true,
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"providerType\":\"open_ai\""));
        assert!(json.contains("\"displayName\":\"OpenAI\""));
        assert!(json.contains("\"defaultModel\":\"gpt-4o\""));
        assert!(json.contains("\"supportsStreaming\":true"));
    }

    #[test]
    fn test_strip_markdown_fences_plain_json() {
        let input = r#"{"explanation":"test","suggestedSql":null,"changes":[]}"#;
        assert_eq!(strip_markdown_fences(input), input);
    }

    #[test]
    fn test_strip_markdown_fences_with_json_fence() {
        let input = "```json\n{\"explanation\":\"test\"}\n```";
        assert_eq!(strip_markdown_fences(input), "{\"explanation\":\"test\"}");
    }

    #[test]
    fn test_strip_markdown_fences_with_bare_fence() {
        let input = "```\n{\"explanation\":\"test\"}\n```";
        assert_eq!(strip_markdown_fences(input), "{\"explanation\":\"test\"}");
    }

    #[test]
    fn test_strip_markdown_fences_whitespace() {
        let input = "  ```json\n  {\"key\":\"val\"}  \n```  ";
        assert_eq!(strip_markdown_fences(input), "{\"key\":\"val\"}");
    }
}
