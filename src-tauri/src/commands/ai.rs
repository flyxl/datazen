//! AI-related Tauri IPC commands.

use crate::ai::*;
use crate::ai::prompt_resolver::{PromptInfo, PromptOverrideEntry};
use crate::commands::error::{CmdExt, CommandError};
use crate::commands::AppState;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;
use uuid::Uuid;
use std::collections::HashMap;
use datazen_driver_api::PromptScenario;

fn language_hint(lang: &str) -> String {
    let lang_name = match lang {
        "zh-CN" => "Chinese (Simplified)",
        "zh-TW" => "Chinese (Traditional)",
        "en" => "English",
        "ja" => "Japanese",
        "ko" => "Korean",
        _ => lang,
    };
    format!("\n\nIMPORTANT: All free-text content in your response MUST be in {lang_name}.")
}

fn inject_language_hint(messages: &mut [ChatMessage], lang: &str) {
    if let Some(sys) = messages.iter_mut().find(|m| m.role == MessageRole::System) {
        sys.content.push_str(&language_hint(lang));
    }
}

use std::sync::Arc;
use datazen_ai_api::AiProviderConfig;

async fn resolve_ai(state: &AppState) -> Result<(Arc<dyn AiProvider>, AiProviderConfig), CommandError> {
    let config = state
        .store
        .get_ai_config()
        .await
        .ok_or_else(|| CommandError::NotConfigured("AI_NOT_CONFIGURED".into()))?;

    tracing::debug!(
        provider = %config.provider_type,
        model = %config.model,
        endpoint = ?config.endpoint,
        "resolve_ai: provider config"
    );

    let provider = state
        .ai_registry
        .get(&config.provider_type)
        .await
        .ok_or_else(|| CommandError::NotConfigured("AI_PROVIDER_NOT_AVAILABLE".into()))?;

    Ok((provider, config))
}

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
) -> Result<Vec<ProviderListItem>, CommandError> {
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
) -> Result<Vec<ModelInfo>, CommandError> {
    let provider = state
        .ai_registry
        .get(&provider_type)
        .await
        .ok_or_else(|| CommandError::NotFound(format!("Provider {:?} not found", provider_type)))?;
    Ok(provider.available_models())
}

#[tauri::command]
pub async fn ai_fetch_remote_models(
    protocol: String,
    endpoint: String,
    api_key: String,
) -> Result<Vec<ModelInfo>, CommandError> {
    let proto = match protocol.as_str() {
        "open_ai_compatible" => crate::ai::custom::CustomProtocol::OpenAiCompatible,
        "open_ai_responses" => crate::ai::custom::CustomProtocol::OpenAiResponses,
        "anthropic_compatible" => crate::ai::custom::CustomProtocol::AnthropicCompatible,
        other => return Err(CommandError::Validation(format!("Unknown protocol: {other}"))),
    };

    crate::ai::custom::fetch_remote_models(proto, &endpoint, &api_key)
        .await
        .cmd_err("ai_fetch_remote_models")
}

#[tauri::command]
pub async fn ai_validate_config(
    state: State<'_, AppState>,
    config: AiProviderConfig,
) -> Result<(), CommandError> {
    let provider = state
        .ai_registry
        .get(&config.provider_type)
        .await
        .ok_or_else(|| CommandError::NotFound(format!("Provider {:?} not found", config.provider_type)))?;
    provider
        .validate_config(&config)
        .await
        .cmd_err("ai_validate_config")
}

#[tauri::command]
pub async fn ai_save_config(
    state: State<'_, AppState>,
    config: AiProviderConfig,
) -> Result<(), CommandError> {
    let provider = state
        .ai_registry
        .get(&config.provider_type)
        .await
        .ok_or_else(|| CommandError::NotFound(format!("Provider {:?} not found", config.provider_type)))?;

    provider
        .initialize(&config)
        .await
        .cmd_err("ai_save_config")?;

    state
        .store
        .save_ai_config(&config)
        .await
        .cmd_err("ai_save_config")
}

#[tauri::command]
pub async fn ai_get_config(
    state: State<'_, AppState>,
) -> Result<Option<AiProviderConfig>, CommandError> {
    Ok(state.store.get_ai_config().await)
}

#[tauri::command]
pub async fn ai_delete_config(
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    for provider in state.ai_registry.all_providers().await {
        provider.reset().await;
    }
    state
        .store
        .delete_ai_config()
        .await
        .cmd_err("ai_delete_config")
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
    recent_queries: Option<Vec<String>>,
) -> Result<String, CommandError> {
    let recent_queries = recent_queries.unwrap_or_default();
    tracing::info!(
        %request_id,
        %connection_id,
        %database,
        input = %natural_language,
        current_table = ?current_table,
        recent_queries_count = recent_queries.len(),
        "ai_generate_sql: start"
    );

    let (provider, ai_config) = resolve_ai(&state).await?;

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
        .cmd_err("ai_generate_sql")?;

    tracing::debug!(
        schema_ddl_len = context.schema_ddl.len(),
        "ai_generate_sql: schema context built"
    );

    let lang = state.store.get_settings().await.language;

    let (driver_ref, _) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("ai_generate_sql")?;

    let version_str = context
        .database_version
        .as_deref()
        .map(|v| format!(" {v}"))
        .unwrap_or_default();
    let recent = if context.recent_queries.is_empty() {
        String::new()
    } else {
        let label = if lang.starts_with("zh") { "近期查询（供风格参考）" } else { "Recent queries (for style reference)" };
        format!(
            "\n\n{label}:\n{}",
            context.recent_queries.iter().map(|q| format!("- {q}")).collect::<Vec<_>>().join("\n")
        )
    };
    let mut vars = HashMap::new();
    vars.insert("db_type", context.database_type.as_str());
    vars.insert("version", version_str.as_str());
    vars.insert("schema", context.schema_ddl.as_str());
    vars.insert("recent", recent.as_str());
    let tpl = state.prompt_resolver.resolve(PromptScenario::Nl2Sql, Some(driver_ref.as_ref()), &lang).await;
    let system_content = prompt_resolver::render_template(&tpl, &vars);

    let system_msg = ChatMessage { role: MessageRole::System, content: system_content };
    let user_msg = ChatMessage {
        role: MessageRole::User,
        content: natural_language,
    };

    let (tx, mut rx) = mpsc::channel::<Result<StreamChunk, AiError>>(32);
    let mut request = CompletionRequest {
        request_id: request_id.clone(),
        model: ai_config.model.clone(),
        messages: vec![system_msg, user_msg],
        temperature: Some(0.0),
        max_tokens: Some(ai_config.max_tokens),
        stop: None,
    };
    inject_language_hint(&mut request.messages, &lang);

    tracing::debug!(
        %request_id,
        model = %request.model,
        messages_count = request.messages.len(),
        system_prompt_len = request.messages.first().map(|m| m.content.len()).unwrap_or(0),
        "ai_generate_sql: sending to provider (stream)"
    );

    let req_id_clone = request_id.clone();
    let handle_clone = app_handle.clone();

    tokio::spawn(async move {
        while let Some(chunk_result) = rx.recv().await {
            emit_stream_chunk_or_error(&handle_clone, &req_id_clone, chunk_result);
        }
    });

    provider
        .stream_complete(&request, tx)
        .await
        .cmd_err("ai_generate_sql")?;

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
) -> Result<DiagnosisResult, CommandError> {
    tracing::info!(
        %connection_id,
        %database,
        sql_len = sql.len(),
        error = %error_message,
        "ai_diagnose_error: start"
    );

    let (provider, ai_config) = resolve_ai(&state).await?;

    let context = state
        .schema_context_builder
        .build_sql_context(&connection_id, &database, None, &[], 3000)
        .await
        .cmd_err("ai_diagnose_error")?;

    let lang = state.store.get_settings().await.language;

    let (driver_ref, _) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("ai_diagnose_error")?;

    let mut vars = HashMap::new();
    vars.insert("db_type", context.database_type.as_str());
    vars.insert("schema", context.schema_ddl.as_str());
    let tpl = state.prompt_resolver.resolve(PromptScenario::Diagnose, Some(driver_ref.as_ref()), &lang).await;
    let system_content = prompt_resolver::render_template(&tpl, &vars);

    let mut request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
        messages: vec![
            ChatMessage { role: MessageRole::System, content: system_content },
            ChatMessage {
                role: MessageRole::User,
                content: format!("SQL:\n```\n{sql}\n```\n\nError:\n{error_message}"),
            },
        ],
        temperature: Some(0.0),
        max_tokens: Some(ai_config.max_tokens),
        stop: None,
    };
    inject_language_hint(&mut request.messages, &lang);

    tracing::debug!(
        model = %request.model,
        messages_count = request.messages.len(),
        "ai_diagnose_error: sending to provider"
    );

    let response = provider
        .complete(&request)
        .await
        .cmd_err("ai_diagnose_error")?;

    tracing::debug!(
        response_len = response.content.len(),
        usage = ?response.usage,
        "ai_diagnose_error: response received"
    );

    let content = strip_markdown_fences(&response.content);
    if content.trim().is_empty() {
        tracing::error!(cmd = "ai_diagnose_error", "LLM returned empty response");
        return Err(CommandError::Internal("LLM returned empty response".into()));
    }
    serde_json::from_str::<DiagnosisResult>(&content)
        .map_err(|e| {
            tracing::error!(
                cmd = "ai_diagnose_error",
                raw_content = %&content[..content.len().min(500)],
                "JSON parse failed: {e}"
            );
            CommandError::Json(e)
        })
}

// ─── EXPLAIN Analysis ───

#[tauri::command]
pub async fn ai_analyze_explain(
    state: State<'_, AppState>,
    connection_id: String,
    explain_output: String,
    original_sql: String,
) -> Result<ExplainAnalysis, CommandError> {
    tracing::info!(
        %connection_id,
        sql_len = original_sql.len(),
        explain_len = explain_output.len(),
        "ai_analyze_explain: start"
    );
    let (provider, ai_config) = resolve_ai(&state).await?;

    let (driver, _) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("ai_analyze_explain")?;

    let db_type = format!("{:?}", driver.driver_type());

    let lang = state.store.get_settings().await.language;
    let mut vars = HashMap::new();
    vars.insert("db_type", db_type.as_str());
    let tpl = state.prompt_resolver.resolve(PromptScenario::ExplainAnalysis, Some(driver.as_ref()), &lang).await;
    let system_content = prompt_resolver::render_template(&tpl, &vars);

    let mut request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
        messages: vec![
            ChatMessage { role: MessageRole::System, content: system_content },
            ChatMessage {
                role: MessageRole::User,
                content: format!(
                    "SQL:\n```\n{original_sql}\n```\n\nEXPLAIN output:\n```\n{explain_output}\n```"
                ),
            },
        ],
        temperature: Some(0.0),
        max_tokens: Some(ai_config.max_tokens),
        stop: None,
    };
    inject_language_hint(&mut request.messages, &lang);

    for (i, msg) in request.messages.iter().enumerate() {
        tracing::info!(
            idx = i,
            role = ?msg.role,
            content_len = msg.content.len(),
            content_preview = %&msg.content[..msg.content.len().min(300)],
            "ai_analyze_explain: message[{}]", i
        );
    }

    let response = provider
        .complete(&request)
        .await
        .cmd_err("ai_analyze_explain")?;

    tracing::info!(
        response_len = response.content.len(),
        finish_reason = ?response.finish_reason,
        prompt_tokens = response.usage.prompt_tokens,
        completion_tokens = response.usage.completion_tokens,
        "ai_analyze_explain: response received"
    );
    if !response.content.is_empty() {
        tracing::info!(
            response_content = %&response.content[..response.content.len().min(500)],
            "ai_analyze_explain: response content"
        );
    }

    let content = strip_markdown_fences(&response.content);
    if content.trim().is_empty() {
        tracing::error!(cmd = "ai_analyze_explain", "LLM returned empty response");
        return Err(CommandError::Internal("LLM returned empty response".into()));
    }
    serde_json::from_str::<ExplainAnalysis>(&content)
        .map_err(|e| {
            tracing::error!(
                cmd = "ai_analyze_explain",
                raw_content = %&content[..content.len().min(500)],
                "JSON parse failed: {e}"
            );
            CommandError::Json(e)
        })
}

// ─── Smart Filter ───

#[tauri::command]
pub async fn ai_parse_filter(
    state: State<'_, AppState>,
    connection_id: String,
    database: String,
    table: String,
    natural_language: String,
) -> Result<Vec<crate::services::query_executor::FilterCondition>, CommandError> {
    tracing::info!(
        %connection_id,
        %database,
        %table,
        input = %natural_language,
        "ai_parse_filter: start"
    );
    let (provider, ai_config) = resolve_ai(&state).await?;

    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("ai_parse_filter")?;

    let db_type = format!("{:?}", driver.driver_type());

    let cached = state
        .schema_cache
        .get_columns(&connection_id, &database, &table, &driver, &handle)
        .await
        .cmd_err("ai_parse_filter")?;

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

    let lang = state.store.get_settings().await.language;
    let mut vars = HashMap::new();
    vars.insert("db_type", db_type.as_str());
    vars.insert("columns", columns_ddl.as_str());
    let tpl = state.prompt_resolver.resolve(PromptScenario::NlFilter, Some(driver.as_ref()), &lang).await;
    let system_content = prompt_resolver::render_template(&tpl, &vars);

    let mut request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
        messages: vec![
            ChatMessage { role: MessageRole::System, content: system_content },
            ChatMessage {
                role: MessageRole::User,
                content: natural_language,
            },
        ],
        temperature: Some(0.0),
        max_tokens: Some(ai_config.max_tokens),
        stop: None,
    };
    inject_language_hint(&mut request.messages, &lang);

    let response = provider
        .complete(&request)
        .await
        .cmd_err("ai_parse_filter")?;

    let content = strip_markdown_fences(&response.content);
    let mut filters: Vec<crate::services::query_executor::FilterCondition> =
        serde_json::from_str(&content).cmd_err("ai_parse_filter")?;

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
) -> Result<String, CommandError> {
    tracing::info!(
        %request_id,
        connection_id = ?connection_id,
        database = ?database,
        messages_count = messages.len(),
        %include_schema,
        last_user_msg = messages.last().map(|m| &m.content[..m.content.len().min(100)]).unwrap_or(""),
        "ai_chat: start"
    );
    let (provider, ai_config) = resolve_ai(&state).await?;

    let lang = state.store.get_settings().await.language;
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
                    let base = state.prompt_resolver.resolve(PromptScenario::Chat, Some(driver.as_ref()), &lang).await;
                    let desc = if lang.starts_with("zh") {
                        format!("{base}\n\n用户已连接到 {db_type} 数据库。")
                    } else {
                        format!("{base}\n\nThe user is connected to a {db_type} database.")
                    };
                    full_messages.push(ChatMessage {
                        role: MessageRole::System,
                        content: format!("{desc}\n\nSchema:\n{}", context.schema_ddl),
                    });
                }
            }
        }
    }

    if full_messages.is_empty() {
        let chat_prompt = state.prompt_resolver.resolve(PromptScenario::Chat, None, &lang).await;
        full_messages.push(ChatMessage {
            role: MessageRole::System,
            content: chat_prompt,
        });
    }

    full_messages.extend(messages);

    let (tx, mut rx) = mpsc::channel::<Result<StreamChunk, AiError>>(32);
    let mut request = CompletionRequest {
        request_id: request_id.clone(),
        model: ai_config.model.clone(),
        messages: full_messages,
        temperature: Some(0.7),
        max_tokens: Some(ai_config.max_tokens),
        stop: None,
    };
    inject_language_hint(&mut request.messages, &lang);

    let req_id_clone = request_id.clone();
    let handle_clone = app_handle.clone();

    tokio::spawn(async move {
        while let Some(chunk_result) = rx.recv().await {
            emit_stream_chunk_or_error(&handle_clone, &req_id_clone, chunk_result);
        }
    });

    provider
        .stream_complete(&request, tx)
        .await
        .cmd_err("ai_chat")?;

    Ok(request_id)
}

fn emit_stream_chunk_or_error(
    handle: &AppHandle,
    request_id: &str,
    result: Result<StreamChunk, AiError>,
) {
    match result {
        Ok(chunk) => {
            let mut payload = serde_json::json!({
                "requestId": request_id,
                "content": chunk.content,
                "done": chunk.done,
                "usage": chunk.usage,
            });
            if let Some(reasoning) = &chunk.reasoning {
                payload["reasoning"] = serde_json::Value::String(reasoning.clone());
            }
            let _ = handle.emit("ai:stream-chunk", payload);
        }
        Err(e) => {
            let _ = handle.emit(
                "ai:stream-error",
                serde_json::json!({
                    "requestId": request_id,
                    "error": e.to_string(),
                }),
            );
        }
    }
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
) -> Result<Vec<crate::mcp::SkillListItem>, CommandError> {
    Ok(state.skill_registry.list().await)
}

#[tauri::command]
pub async fn skill_execute(
    state: State<'_, AppState>,
    skill_id: String,
    variables: serde_json::Value,
    connection_id: Option<String>,
) -> Result<crate::mcp::SkillExecutionResult, CommandError> {
    let skill = state
        .skill_registry
        .get(&skill_id)
        .await
        .ok_or_else(|| CommandError::NotFound(format!("Skill '{skill_id}' not found")))?;

    let result = crate::mcp::SkillExecutor::execute(
        &skill,
        &state,
        connection_id.as_deref(),
        &variables,
    )
    .await
    .cmd_err("skill_execute")?;

    if let Err(e) = state
        .skill_history
        .record(&skill.id, &skill.name, &variables, &result)
        .await
    {
        tracing::warn!("Failed to record skill history: {e}");
    }

    Ok(result)
}

#[tauri::command]
pub async fn skill_save(
    state: State<'_, AppState>,
    skill: crate::mcp::SkillDefinition,
) -> Result<(), CommandError> {
    state.skill_registry.save_skill(&skill).await.map_err(CommandError::Internal)
}

#[tauri::command]
pub async fn skill_delete(
    state: State<'_, AppState>,
    skill_id: String,
) -> Result<(), CommandError> {
    state.skill_registry.delete_skill(&skill_id).await.map_err(CommandError::Internal)
}

#[tauri::command]
pub async fn skill_reload(state: State<'_, AppState>) -> Result<(), CommandError> {
    state.skill_registry.load_all().await.map_err(CommandError::Internal)
}

#[tauri::command]
pub async fn skill_get_dir(
    state: State<'_, AppState>,
) -> Result<String, CommandError> {
    Ok(state.skill_registry.skills_dir().display().to_string())
}

#[tauri::command]
pub async fn skill_get(
    state: State<'_, AppState>,
    skill_id: String,
) -> Result<crate::mcp::SkillDefinition, CommandError> {
    state
        .skill_registry
        .get(&skill_id)
        .await
        .ok_or_else(|| CommandError::NotFound(format!("Skill '{skill_id}' not found")))
}

// ─── Skill History ───

#[tauri::command]
pub async fn skill_history_list(
    state: State<'_, AppState>,
    skill_id: Option<String>,
) -> Result<Vec<crate::mcp::skill_history::HistoryListItem>, CommandError> {
    Ok(state.skill_history.list(skill_id.as_deref()).await)
}

#[tauri::command]
pub async fn skill_history_get(
    state: State<'_, AppState>,
    history_id: String,
) -> Result<crate::mcp::skill_history::HistoryEntry, CommandError> {
    state
        .skill_history
        .get(&history_id)
        .await
        .ok_or_else(|| CommandError::NotFound(format!("History '{history_id}' not found")))
}

#[tauri::command]
pub async fn skill_history_clear(
    state: State<'_, AppState>,
    skill_id: Option<String>,
) -> Result<usize, CommandError> {
    state
        .skill_history
        .clear(skill_id.as_deref())
        .await
        .cmd_err("skill_history_clear")
}

// ─── Phase 8: Schema documentation ───

#[tauri::command]
pub async fn ai_generate_schema_doc(
    state: State<'_, AppState>,
    connection_id: String,
    database: String,
) -> Result<String, CommandError> {
    tracing::info!(%connection_id, %database, "ai_generate_schema_doc: start");
    let (provider, ai_config) = resolve_ai(&state).await?;

    // Step 1: Get table names only (no column details)
    let (db_type, all_table_names) = state
        .schema_context_builder
        .get_table_names(&connection_id, &database)
        .await
        .cmd_err("ai_generate_schema_doc")?;

    let lang = state.store.get_settings().await.language;

    let (driver_ref, _) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("ai_generate_schema_doc")?;

    // If few tables, skip the selection step and document all
    let selected_tables = if all_table_names.len() <= 30 {
        all_table_names.clone()
    } else {
        let table_names_str = all_table_names.join(", ");
        let mut select_vars = HashMap::new();
        select_vars.insert("db_type", db_type.as_str());
        select_vars.insert("table_names", table_names_str.as_str());
        let select_tpl = state.prompt_resolver.resolve(PromptScenario::SchemaDocSelectTables, Some(driver_ref.as_ref()), &lang).await;
        let select_content = prompt_resolver::render_template(&select_tpl, &select_vars);

        let select_request = CompletionRequest {
            request_id: Uuid::new_v4().to_string(),
            model: ai_config.model.clone(),
            messages: vec![
                ChatMessage { role: MessageRole::System, content: select_content },
                ChatMessage {
                    role: MessageRole::User,
                    content: "Select the important user tables.".into(),
                },
            ],
            temperature: Some(0.0),
            max_tokens: Some(ai_config.max_tokens),
            stop: None,
        };

        let select_response = provider
            .complete(&select_request)
            .await
            .cmd_err("ai_generate_schema_doc[select]")?;

        let raw = strip_markdown_fences(select_response.content.trim());
        serde_json::from_str::<Vec<String>>(&raw).unwrap_or_else(|_| {
            // Fallback: filter out obvious system tables
            all_table_names
                .iter()
                .filter(|n| {
                    !n.starts_with("pg_")
                        && !n.starts_with("sql_")
                        && !n.starts_with("sqlite_")
                        && !n.starts_with("information_schema")
                })
                .take(30)
                .cloned()
                .collect()
        })
    };

    // Step 2: Get detailed schema for selected tables only
    let context = state
        .schema_context_builder
        .build_selective_context(&connection_id, &database, &selected_tables, 8000)
        .await
        .cmd_err("ai_generate_schema_doc")?;

    let user_content = if lang.starts_with("zh") {
        "请为上面的数据库 schema 生成文档。"
    } else {
        "Generate documentation for the database schema above."
    };
    let mut doc_vars = HashMap::new();
    doc_vars.insert("db_type", context.database_type.as_str());
    doc_vars.insert("schema", context.schema_ddl.as_str());
    let doc_tpl = state.prompt_resolver.resolve(PromptScenario::SchemaDoc, Some(driver_ref.as_ref()), &lang).await;
    let doc_system_content = prompt_resolver::render_template(&doc_tpl, &doc_vars);

    let mut request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
        messages: vec![
            ChatMessage { role: MessageRole::System, content: doc_system_content },
            ChatMessage {
                role: MessageRole::User,
                content: user_content.into(),
            },
        ],
        temperature: Some(0.3),
        max_tokens: Some(ai_config.max_tokens),
        stop: None,
    };
    inject_language_hint(&mut request.messages, &lang);

    let response = provider
        .complete(&request)
        .await
        .cmd_err("ai_generate_schema_doc")?;

    let content = response.content.trim();
    let stripped = if content.starts_with("```markdown") || content.starts_with("```md") {
        strip_markdown_fences(content)
    } else {
        content.to_string()
    };
    Ok(stripped)
}

// ─── Phase 8: Connection diagnostics ───

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionDiagnosis {
    pub diagnosis: String,
    pub possible_causes: Vec<String>,
    pub solutions: Vec<ConnectionSolution>,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSolution {
    pub description: String,
    pub command: Option<String>,
}

#[tauri::command]
pub async fn ai_diagnose_connection(
    state: State<'_, AppState>,
    connection_id: String,
    error_message: String,
) -> Result<ConnectionDiagnosis, CommandError> {
    tracing::info!(%connection_id, error = %error_message, "ai_diagnose_connection: start");
    let (provider, ai_config) = resolve_ai(&state).await?;

    let conn_info = state
        .store
        .get_connection(&connection_id)
        .await
        .ok_or_else(|| CommandError::NotFound("Connection config not found".into()))?;

    let ssl_str = format!("{:?}", conn_info.ssl_mode);
    let ssh_str = if conn_info.ssh_tunnel.is_some() { "enabled" } else { "disabled" };
    let conn_summary = format!(
        "Connection type: {:?}\nHost: {}\nPort: {}\nDatabase: {}\nUsername: {}\nSSL: {}\nSSH Tunnel: {}\nTimeout: {}s",
        conn_info.database_type,
        conn_info.host.as_deref().unwrap_or("N/A"),
        conn_info.port.map(|p| p.to_string()).unwrap_or_else(|| "N/A".into()),
        conn_info.database.as_deref().unwrap_or("N/A"),
        conn_info.username.as_deref().unwrap_or("N/A"),
        ssl_str,
        ssh_str,
        conn_info.connection_timeout,
    );

    let lang = state.store.get_settings().await.language;
    let conn_diag_prompt = state.prompt_resolver.resolve(PromptScenario::ConnectionDiagnose, None, &lang).await;

    let mut request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
        messages: vec![
            ChatMessage { role: MessageRole::System, content: conn_diag_prompt },
            ChatMessage {
                role: MessageRole::User,
                content: format!(
                    "Connection details:\n{conn_summary}\n\nError:\n{error_message}"
                ),
            },
        ],
        temperature: Some(0.0),
        max_tokens: Some(ai_config.max_tokens),
        stop: None,
    };
    inject_language_hint(&mut request.messages, &lang);

    let response = provider
        .complete(&request)
        .await
        .cmd_err("ai_diagnose_connection")?;

    let content = strip_markdown_fences(&response.content);
    serde_json::from_str::<ConnectionDiagnosis>(&content)
        .cmd_err("ai_diagnose_connection")
}

// ─── Phase 8: Query history analysis ───

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryAnalysis {
    pub summary: String,
    pub categories: Vec<QueryCategory>,
    pub insights: Vec<String>,
    pub frequent_tables: Vec<String>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryCategory {
    pub name: String,
    pub count: usize,
    pub examples: Vec<String>,
}

#[tauri::command]
pub async fn ai_analyze_queries(
    state: State<'_, AppState>,
    connection_id: Option<String>,
) -> Result<QueryAnalysis, CommandError> {
    tracing::info!(connection_id = ?connection_id, "ai_analyze_queries: start");
    let (provider, ai_config) = resolve_ai(&state).await?;

    let history = state.store.get_query_history(200).await;
    let filtered: Vec<_> = if let Some(ref cid) = connection_id {
        history
            .iter()
            .filter(|h| &h.connection_id == cid)
            .collect()
    } else {
        history.iter().collect()
    };

    if filtered.is_empty() {
        return Err(CommandError::Validation("No query history available".into()));
    }

    let queries_text = filtered
        .iter()
        .take(100)
        .map(|h| h.sql.as_str())
        .collect::<Vec<_>>()
        .join("\n---\n");

    let lang = state.store.get_settings().await.language;
    let query_summary_prompt = state.prompt_resolver.resolve(PromptScenario::QuerySummary, None, &lang).await;

    let mut request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
        messages: vec![
            ChatMessage { role: MessageRole::System, content: query_summary_prompt },
            ChatMessage {
                role: MessageRole::User,
                content: format!(
                    "Analyze these {} queries:\n\n{queries_text}",
                    filtered.len().min(100),
                ),
            },
        ],
        temperature: Some(0.2),
        max_tokens: Some(ai_config.max_tokens),
        stop: None,
    };
    inject_language_hint(&mut request.messages, &lang);

    let response = provider
        .complete(&request)
        .await
        .cmd_err("ai_analyze_queries")?;

    let content = strip_markdown_fences(&response.content);
    serde_json::from_str::<QueryAnalysis>(&content)
        .cmd_err("ai_analyze_queries")
}

// ─── Prompt management IPC commands ───

#[tauri::command]
pub async fn prompt_list(
    state: State<'_, AppState>,
    driver_type: Option<String>,
) -> Result<Vec<PromptInfo>, CommandError> {
    let driver: Option<std::sync::Arc<dyn datazen_driver_api::DatabaseDriver>> =
        if let Some(ref dt) = driver_type {
            state.driver_registry.get_sql_driver_by_name(dt)
        } else {
            None
        };
    let prompts = state
        .prompt_resolver
        .list_prompts(driver.as_deref())
        .await;
    Ok(prompts)
}

#[tauri::command]
pub async fn prompt_set_override(
    state: State<'_, AppState>,
    entry: PromptOverrideEntry,
) -> Result<(), CommandError> {
    state
        .prompt_resolver
        .set_override(entry)
        .await
        .map_err(CommandError::Internal)
}

#[tauri::command]
pub async fn prompt_remove_override(
    state: State<'_, AppState>,
    driver_type: String,
    scenario: PromptScenario,
) -> Result<(), CommandError> {
    state
        .prompt_resolver
        .remove_override(&driver_type, scenario)
        .await
        .map_err(CommandError::Internal)
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

    #[test]
    fn test_connection_diagnosis_deserialization() {
        let json = r#"{
            "diagnosis": "Authentication failed",
            "possibleCauses": ["Wrong password", "User does not exist"],
            "solutions": [
                {"description": "Check password", "command": null},
                {"description": "Create user", "command": "CREATE USER test"}
            ],
            "category": "auth"
        }"#;
        let result: ConnectionDiagnosis = serde_json::from_str(json).unwrap();
        assert_eq!(result.diagnosis, "Authentication failed");
        assert_eq!(result.possible_causes.len(), 2);
        assert_eq!(result.solutions.len(), 2);
        assert_eq!(result.category, "auth");
        assert!(result.solutions[0].command.is_none());
        assert_eq!(result.solutions[1].command.as_deref(), Some("CREATE USER test"));
    }

    #[test]
    fn test_query_analysis_deserialization() {
        let json = r#"{
            "summary": "Mostly read queries",
            "categories": [
                {"name": "SELECT", "count": 10, "examples": ["SELECT * FROM users"]}
            ],
            "insights": ["Heavy read workload"],
            "frequentTables": ["users", "orders"],
            "recommendations": ["Add index on orders.user_id"]
        }"#;
        let result: QueryAnalysis = serde_json::from_str(json).unwrap();
        assert_eq!(result.summary, "Mostly read queries");
        assert_eq!(result.categories.len(), 1);
        assert_eq!(result.categories[0].count, 10);
        assert_eq!(result.frequent_tables, vec!["users", "orders"]);
        assert_eq!(result.recommendations.len(), 1);
    }
}
