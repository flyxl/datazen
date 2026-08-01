//! AI-related Tauri IPC commands.

use crate::ai::*;
use crate::commands::{log_err, AppState};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;
use uuid::Uuid;

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

async fn resolve_ai(state: &AppState) -> Result<(Arc<dyn AiProvider>, AiProviderConfig), String> {
    let config = state
        .store
        .get_ai_config()
        .await
        .ok_or_else(|| "AI_NOT_CONFIGURED".to_string())?;

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
        .ok_or_else(|| "AI_PROVIDER_NOT_AVAILABLE".to_string())?;

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
pub async fn ai_fetch_remote_models(
    protocol: String,
    endpoint: String,
    api_key: String,
) -> Result<Vec<ModelInfo>, String> {
    let proto = match protocol.as_str() {
        "open_ai_compatible" => crate::ai::custom::CustomProtocol::OpenAiCompatible,
        "open_ai_responses" => crate::ai::custom::CustomProtocol::OpenAiResponses,
        "anthropic_compatible" => crate::ai::custom::CustomProtocol::AnthropicCompatible,
        other => return Err(format!("Unknown protocol: {other}")),
    };

    crate::ai::custom::fetch_remote_models(proto, &endpoint, &api_key)
        .await
        .map_err(|e| log_err("ai_fetch_remote_models", &e))
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
    recent_queries: Option<Vec<String>>,
) -> Result<String, String> {
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
        .map_err(|e| log_err("ai_generate_sql", &e))?;

    tracing::debug!(
        schema_ddl_len = context.schema_ddl.len(),
        "ai_generate_sql: schema context built"
    );

    let lang = state.store.get_settings().await.language;
    let system_msg = PromptBuilder::nl2sql_system(&context, &lang);
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
        .map_err(|e| log_err("ai_diagnose_error", &e))?;

    let lang = state.store.get_settings().await.language;
    let mut request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
        messages: vec![
            PromptBuilder::diagnose_system(&context.database_type, &context.schema_ddl, &lang),
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
        .map_err(|e| log_err("ai_diagnose_error", &e))?;

    tracing::debug!(
        response_len = response.content.len(),
        usage = ?response.usage,
        "ai_diagnose_error: response received"
    );

    let content = strip_markdown_fences(&response.content);
    if content.trim().is_empty() {
        return Err(log_err("ai_diagnose_error", &"LLM returned empty response"));
    }
    serde_json::from_str::<DiagnosisResult>(&content)
        .map_err(|e| {
            tracing::error!(
                raw_content = %&content[..content.len().min(500)],
                "ai_diagnose_error: JSON parse failed"
            );
            log_err("ai_diagnose_error", &e)
        })
}

// ─── EXPLAIN Analysis ───

#[tauri::command]
pub async fn ai_analyze_explain(
    state: State<'_, AppState>,
    connection_id: String,
    explain_output: String,
    original_sql: String,
) -> Result<ExplainAnalysis, String> {
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
        .map_err(|e| log_err("ai_analyze_explain", &e))?;

    let db_type = format!("{:?}", driver.driver_type());

    let lang = state.store.get_settings().await.language;
    let mut request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
        messages: vec![
            PromptBuilder::explain_analysis_system(&db_type, &lang),
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
        .map_err(|e| log_err("ai_analyze_explain", &e))?;

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
        return Err(log_err("ai_analyze_explain", &"LLM returned empty response"));
    }
    serde_json::from_str::<ExplainAnalysis>(&content)
        .map_err(|e| {
            tracing::error!(
                raw_content = %&content[..content.len().min(500)],
                "ai_analyze_explain: JSON parse failed"
            );
            log_err("ai_analyze_explain", &e)
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
) -> Result<Vec<crate::services::query_executor::FilterCondition>, String> {
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

    let lang = state.store.get_settings().await.language;
    let mut request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
        messages: vec![
            PromptBuilder::nl_filter_system(&db_type, &columns_ddl, &lang),
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
    let is_zh = lang.starts_with("zh");
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
                    let desc = if is_zh {
                        format!("你是一个有用的数据库助手。用户已连接到 {db_type} 数据库。")
                    } else {
                        format!("You are a helpful database assistant. The user is connected to a {db_type} database.")
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
        let desc = if is_zh {
            "你是一个有用的数据库助手。帮助用户处理 SQL 查询、数据库概念和数据分析。编写 SQL 时请使用正确的格式并解释你的思路。"
        } else {
            "You are a helpful database assistant. Help the user with SQL queries, database concepts, and data analysis. When writing SQL, use proper formatting and explain your reasoning."
        };
        full_messages.push(ChatMessage {
            role: MessageRole::System,
            content: desc.to_string(),
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

// ─── Phase 8: Schema documentation ───

#[tauri::command]
pub async fn ai_generate_schema_doc(
    state: State<'_, AppState>,
    connection_id: String,
    database: String,
) -> Result<String, String> {
    tracing::info!(%connection_id, %database, "ai_generate_schema_doc: start");
    let (provider, ai_config) = resolve_ai(&state).await?;

    // Step 1: Get table names only (no column details)
    let (db_type, all_table_names) = state
        .schema_context_builder
        .get_table_names(&connection_id, &database)
        .await
        .map_err(|e| log_err("ai_generate_schema_doc", &e))?;

    let lang = state.store.get_settings().await.language;

    // If few tables, skip the selection step and document all
    let selected_tables = if all_table_names.len() <= 30 {
        all_table_names.clone()
    } else {
        let select_request = CompletionRequest {
            request_id: Uuid::new_v4().to_string(),
            model: ai_config.model.clone(),
            messages: vec![
                PromptBuilder::schema_doc_select_tables(&db_type, &all_table_names),
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
            .map_err(|e| log_err("ai_generate_schema_doc[select]", &e))?;

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
        .map_err(|e| log_err("ai_generate_schema_doc", &e))?;

    let user_content = if lang.starts_with("zh") {
        "请为上面的数据库 schema 生成文档。"
    } else {
        "Generate documentation for the database schema above."
    };
    let mut request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
        messages: vec![
            PromptBuilder::schema_doc_system(&context.database_type, &context.schema_ddl, &lang),
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
        .map_err(|e| log_err("ai_generate_schema_doc", &e))?;

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
) -> Result<ConnectionDiagnosis, String> {
    tracing::info!(%connection_id, error = %error_message, "ai_diagnose_connection: start");
    let (provider, ai_config) = resolve_ai(&state).await?;

    let conn_info = state
        .store
        .get_connection(&connection_id)
        .await
        .ok_or("Connection config not found")?;

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
    let mut request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
        messages: vec![
            PromptBuilder::connection_diagnose_system(&lang),
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
        .map_err(|e| log_err("ai_diagnose_connection", &e))?;

    let content = strip_markdown_fences(&response.content);
    serde_json::from_str::<ConnectionDiagnosis>(&content)
        .map_err(|e| log_err("ai_diagnose_connection", &e))
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
) -> Result<QueryAnalysis, String> {
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
        return Err("No query history available".to_string());
    }

    let queries_text = filtered
        .iter()
        .take(100)
        .map(|h| h.sql.as_str())
        .collect::<Vec<_>>()
        .join("\n---\n");

    let lang = state.store.get_settings().await.language;
    let mut request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
        messages: vec![
            PromptBuilder::query_summary_system(&lang),
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
        .map_err(|e| log_err("ai_analyze_queries", &e))?;

    let content = strip_markdown_fences(&response.content);
    serde_json::from_str::<QueryAnalysis>(&content)
        .map_err(|e| log_err("ai_analyze_queries", &e))
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
