//! AI-related Tauri IPC commands.

use crate::ai::budget;
use crate::ai::prompt_resolver::{PromptInfo, PromptOverrideEntry};
use crate::ai::*;
use crate::commands::error::{CmdExt, CommandError};
use crate::commands::AppState;
use datazen_driver_api::PromptScenario;
use serde::Serialize;
use std::collections::HashMap;
use tauri::{AppHandle, Emitter, State, WebviewWindow};
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

use datazen_ai_api::AiProviderConfig;
use std::sync::Arc;

/// Delivers streaming chunks to the UI (or test collector).
pub(crate) type StreamCallback = Arc<dyn Fn(&str, Result<StreamChunk, AiError>) + Send + Sync>;

pub(crate) fn window_stream_callback(window: &WebviewWindow) -> StreamCallback {
    let window = window.clone();
    Arc::new(move |request_id, result| {
        emit_stream_chunk_or_error(&window, request_id, result);
    })
}

async fn resolve_ai(
    state: &AppState,
) -> Result<(Arc<dyn AiProvider>, AiProviderConfig), CommandError> {
    state.ensure_ai_ready().await;

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
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub default_endpoint: String,
    pub default_protocol: String,
}

fn provider_defaults(pt: AiProviderType) -> (&'static str, &'static str) {
    match pt {
        AiProviderType::OpenAi => ("https://api.openai.com/v1", "open_ai_compatible"),
        AiProviderType::Anthropic => ("https://api.anthropic.com", "anthropic_compatible"),
        AiProviderType::DeepSeek => ("https://api.deepseek.com", "open_ai_responses"),
        AiProviderType::Ollama => ("http://127.0.0.1:11434/v1", "open_ai_compatible"),
        AiProviderType::Custom => ("", "open_ai_compatible"),
    }
}

pub(crate) async fn ai_get_providers_impl(
    state: &AppState,
) -> Result<Vec<ProviderListItem>, CommandError> {
    state.ensure_ai_ready().await;
    let providers = state.ai_registry.all_providers().await;
    let result = providers
        .into_iter()
        .map(|p| {
            let (ep, proto) = provider_defaults(p.provider_type());
            ProviderListItem {
                provider_type: p.provider_type(),
                display_name: p.display_name().to_string(),
                supports_streaming: p.supports_streaming(),
                supports_tools: p.supports_tools(),
                default_endpoint: ep.into(),
                default_protocol: proto.into(),
            }
        })
        .collect();
    Ok(result)
}

#[tauri::command]
pub async fn ai_get_providers(
    state: State<'_, AppState>,
) -> Result<Vec<ProviderListItem>, CommandError> {
    ai_get_providers_impl(&state).await
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
        other => {
            return Err(CommandError::Validation(format!(
                "Unknown protocol: {other}"
            )))
        }
    };

    crate::ai::custom::fetch_remote_models(proto, &endpoint, &api_key)
        .await
        .cmd_err("ai_fetch_remote_models")
}

pub(crate) async fn ai_validate_config_impl(
    state: &AppState,
    config: AiProviderConfig,
) -> Result<(), CommandError> {
    state.ensure_ai_ready().await;
    let provider = state
        .ai_registry
        .get(&config.provider_type)
        .await
        .ok_or_else(|| {
            CommandError::NotFound(format!("Provider {:?} not found", config.provider_type))
        })?;
    provider
        .validate_config(&config)
        .await
        .cmd_err("ai_validate_config")
}

#[tauri::command]
pub async fn ai_validate_config(
    state: State<'_, AppState>,
    config: AiProviderConfig,
) -> Result<(), CommandError> {
    ai_validate_config_impl(&state, config).await
}

pub(crate) async fn ai_save_config_impl(
    state: &AppState,
    config: AiProviderConfig,
) -> Result<(), CommandError> {
    state.ensure_ai_ready().await;
    let provider = state
        .ai_registry
        .get(&config.provider_type)
        .await
        .ok_or_else(|| {
            CommandError::NotFound(format!("Provider {:?} not found", config.provider_type))
        })?;

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
pub async fn ai_save_config(
    handle: AppHandle,
    state: State<'_, AppState>,
    config: AiProviderConfig,
) -> Result<(), CommandError> {
    ai_save_config_impl(&state, config).await?;
    let _ = handle.emit("ai:config-changed", true);
    Ok(())
}

pub(crate) async fn ai_get_config_impl(
    state: &AppState,
) -> Result<Option<AiProviderConfig>, CommandError> {
    Ok(state.store.get_ai_config().await)
}

#[tauri::command]
pub async fn ai_get_config(
    state: State<'_, AppState>,
) -> Result<Option<AiProviderConfig>, CommandError> {
    ai_get_config_impl(&state).await
}

pub(crate) async fn ai_delete_config_impl(state: &AppState) -> Result<(), CommandError> {
    for provider in state.ai_registry.all_providers().await {
        provider.reset().await;
    }
    state
        .store
        .delete_ai_config()
        .await
        .cmd_err("ai_delete_config")
}

#[tauri::command]
pub async fn ai_delete_config(
    handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    ai_delete_config_impl(&state).await?;
    let _ = handle.emit("ai:config-changed", false);
    Ok(())
}

// ─── NL2SQL ───

pub(crate) async fn ai_generate_sql_impl(
    state: &AppState,
    on_chunk: StreamCallback,
    db_session_id: String,
    database: String,
    natural_language: String,
    request_id: String,
    current_table: Option<String>,
    recent_queries: Option<Vec<String>>,
    context_files: Option<Vec<String>>,
    context_tables: Option<Vec<String>>,
) -> Result<String, CommandError> {
    let recent_queries = recent_queries.unwrap_or_default();
    let mut natural_language = natural_language;
    tracing::info!(
        %request_id,
        %db_session_id,
        %database,
        input_len = natural_language.len(),
        current_table = ?current_table,
        recent_queries_count = recent_queries.len(),
        "ai_generate_sql: start"
    );
    tracing::debug!(%request_id, input = %natural_language, "ai_generate_sql: input");

    let mut ctx_yaml_tables: Vec<String> = Vec::new();

    if let Some(ref ctx_files) = context_files {
        if !ctx_files.is_empty() {
            let ctx_dir = super::context::resolve_context_dir_from_state(&state).await?;
            let entries = super::context::read_context_paths(&ctx_dir, ctx_files).await?;
            if !entries.is_empty() {
                let (yaml_tables, remaining) =
                    crate::ai::ctx_yaml::extract_ctx_yaml_tables(&entries);
                ctx_yaml_tables = yaml_tables;

                if !remaining.is_empty() {
                    let context_block = super::context::format_context_block(&remaining);
                    natural_language = format!("{context_block}\n\n{natural_language}");
                }
            }
        }
    }

    let (provider, ai_config) = resolve_ai(&state).await?;

    let lang = state.store.get_settings().await.language;

    let (driver_ref, _) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("ai_generate_sql")?;

    let mut pinned = context_tables.unwrap_or_default();
    for table in ctx_yaml_tables {
        if !pinned.iter().any(|p| p == &table) {
            pinned.push(table);
        }
    }
    if let Some(ref t) = current_table {
        if !pinned.iter().any(|p| p == t) {
            pinned.insert(0, t.clone());
        }
    }

    let supports_tools = provider.supports_tools();
    let pipeline = SchemaContextPipeline::new(state.schema_context_builder.clone());
    let seed = pipeline
        .resolve(
            &db_session_id,
            &database,
            &pinned,
            supports_tools,
            budget::PINNED_DDL,
            budget::FALLBACK_DDL,
        )
        .await
        .cmd_err("ai_generate_sql")?;

    tracing::debug!(
        schema_suffix_len = compose_schema_system_suffix(&seed).len(),
        attach_db_tools = seed.attach_db_tools,
        pinned_count = pinned.len(),
        "ai_generate_sql: schema context built"
    );

    let schema_suffix = compose_schema_system_suffix(&seed);
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
                .map(|q| format!("- {q}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    let mut vars = HashMap::new();
    vars.insert("db_type", seed.database_type.as_str());
    vars.insert("version", "");
    vars.insert("schema", schema_suffix.as_str());
    vars.insert("recent", recent.as_str());
    let tpl = state
        .prompt_resolver
        .resolve(PromptScenario::Nl2Sql, Some(driver_ref.as_ref()), &lang)
        .await;
    let system_content = prompt_resolver::render_template(&tpl, &vars);

    let system_msg = ChatMessage {
        role: MessageRole::System,
        content: system_content,
        reasoning: None,
        tool_calls: None,
        tool_call_id: None,
    };
    let user_msg = ChatMessage {
        role: MessageRole::User,
        content: natural_language,
        reasoning: None,
        tool_calls: None,
        tool_call_id: None,
    };

    let mut request = CompletionRequest {
        request_id: request_id.clone(),
        model: ai_config.model.clone(),
        messages: vec![system_msg, user_msg],
        temperature: Some(0.0),
        stop: None,
        tools: None,
        previous_response_id: None,
    };
    inject_language_hint(&mut request.messages, &lang);

    tracing::debug!(
        %request_id,
        model = %request.model,
        messages_count = request.messages.len(),
        system_prompt_len = request.messages.first().map(|m| m.content.len()).unwrap_or(0),
        attach_db_tools = seed.attach_db_tools,
        "ai_generate_sql: sending to provider (stream)"
    );

    if seed.attach_db_tools {
        request.tools = Some(db_tool_definitions());
        return run_streaming_tool_loop(
            provider,
            state,
            on_chunk,
            &request_id,
            request,
            10,
            "ai_generate_sql",
        )
        .await;
    }

    let (tx, mut rx) = mpsc::channel::<Result<StreamChunk, AiError>>(32);
    let req_id_clone = request_id.clone();
    let on_chunk_bg = on_chunk.clone();

    tokio::spawn(async move {
        while let Some(chunk_result) = rx.recv().await {
            on_chunk_bg(&req_id_clone, chunk_result);
        }
    });

    provider
        .stream_complete(&request, tx)
        .await
        .cmd_err("ai_generate_sql")?;

    Ok(request_id)
}

#[tauri::command]
pub async fn ai_generate_sql(
    state: State<'_, AppState>,
    window: WebviewWindow,
    db_session_id: String,
    database: String,
    natural_language: String,
    request_id: String,
    current_table: Option<String>,
    recent_queries: Option<Vec<String>>,
    context_files: Option<Vec<String>>,
    context_tables: Option<Vec<String>>,
) -> Result<String, CommandError> {
    ai_generate_sql_impl(
        &state,
        window_stream_callback(&window),
        db_session_id,
        database,
        natural_language,
        request_id,
        current_table,
        recent_queries,
        context_files,
        context_tables,
    )
    .await
}

// ─── SQL Error Diagnosis ───

pub(crate) async fn ai_diagnose_error_impl(
    state: &AppState,
    db_session_id: String,
    database: String,
    sql: String,
    error_message: String,
) -> Result<DiagnosisResult, CommandError> {
    tracing::info!(
        %db_session_id,
        %database,
        sql_len = sql.len(),
        error_len = error_message.len(),
        "ai_diagnose_error: start"
    );
    tracing::debug!(%db_session_id, error = %error_message, "ai_diagnose_error: error");

    let (provider, ai_config) = resolve_ai(&state).await?;

    let context = state
        .schema_context_builder
        .build_sql_context(&db_session_id, &database, None, &[], budget::DIAGNOSE)
        .await
        .cmd_err("ai_diagnose_error")?;

    let lang = state.store.get_settings().await.language;

    let (driver_ref, _) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("ai_diagnose_error")?;

    let mut vars = HashMap::new();
    vars.insert("db_type", context.database_type.as_str());
    vars.insert("schema", context.schema_ddl.as_str());
    let tpl = state
        .prompt_resolver
        .resolve(PromptScenario::Diagnose, Some(driver_ref.as_ref()), &lang)
        .await;
    let system_content = prompt_resolver::render_template(&tpl, &vars);

    let mut request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
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
                content: format!("SQL:\n```\n{sql}\n```\n\nError:\n{error_message}"),
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ],
        temperature: Some(0.0),

        stop: None,
        tools: None,
        previous_response_id: None,
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

    parse_ai_json::<DiagnosisResult>(
        &response.content,
        response.finish_reason.as_deref(),
        "ai_diagnose_error",
    )
}

#[tauri::command]
pub async fn ai_diagnose_error(
    state: State<'_, AppState>,
    db_session_id: String,
    database: String,
    sql: String,
    error_message: String,
) -> Result<DiagnosisResult, CommandError> {
    ai_diagnose_error_impl(&state, db_session_id, database, sql, error_message).await
}

// ─── EXPLAIN Analysis ───

pub(crate) async fn ai_analyze_explain_impl(
    state: &AppState,
    db_session_id: String,
    explain_output: String,
    original_sql: String,
) -> Result<ExplainAnalysis, CommandError> {
    tracing::info!(
        %db_session_id,
        sql_len = original_sql.len(),
        explain_len = explain_output.len(),
        "ai_analyze_explain: start"
    );
    let (provider, ai_config) = resolve_ai(&state).await?;

    let (driver, _) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("ai_analyze_explain")?;

    let db_type = prompt_db_type(driver.as_ref());

    let lang = state.store.get_settings().await.language;
    let mut vars = HashMap::new();
    vars.insert("db_type", db_type.as_str());
    let tpl = state
        .prompt_resolver
        .resolve(
            PromptScenario::ExplainAnalysis,
            Some(driver.as_ref()),
            &lang,
        )
        .await;
    let system_content = prompt_resolver::render_template(&tpl, &vars);

    let mut request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
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
                content: format!(
                    "SQL:\n```\n{original_sql}\n```\n\nEXPLAIN output:\n```\n{explain_output}\n```"
                ),
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ],
        temperature: Some(0.0),

        stop: None,
        tools: None,
        previous_response_id: None,
    };
    inject_language_hint(&mut request.messages, &lang);

    for (i, msg) in request.messages.iter().enumerate() {
        tracing::debug!(
            idx = i,
            role = ?msg.role,
            content_len = msg.content.len(),
            content_preview = %truncate_str(&msg.content, 300),
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
        tracing::debug!(
            response_content = %truncate_str(&response.content, 500),
            "ai_analyze_explain: response content"
        );
    }

    parse_ai_json::<ExplainAnalysis>(
        &response.content,
        response.finish_reason.as_deref(),
        "ai_analyze_explain",
    )
}

#[tauri::command]
pub async fn ai_analyze_explain(
    state: State<'_, AppState>,
    db_session_id: String,
    explain_output: String,
    original_sql: String,
) -> Result<ExplainAnalysis, CommandError> {
    ai_analyze_explain_impl(&state, db_session_id, explain_output, original_sql).await
}

// ─── Smart Filter ───

pub(crate) async fn ai_parse_filter_impl(
    state: &AppState,
    db_session_id: String,
    database: String,
    table: String,
    natural_language: String,
) -> Result<Vec<crate::services::query_executor::FilterCondition>, CommandError> {
    tracing::info!(
        %db_session_id,
        %database,
        %table,
        input_len = natural_language.len(),
        "ai_parse_filter: start"
    );
    tracing::debug!(%db_session_id, input = %natural_language, "ai_parse_filter: input");
    let (provider, ai_config) = resolve_ai(&state).await?;

    let (driver, handle) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("ai_parse_filter")?;

    let db_type = prompt_db_type(driver.as_ref());

    let cached = state
        .schema_cache
        .get_columns(&db_session_id, &database, &table, &driver, &handle)
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
    let tpl = state
        .prompt_resolver
        .resolve(PromptScenario::NlFilter, Some(driver.as_ref()), &lang)
        .await;
    let system_content = prompt_resolver::render_template(&tpl, &vars);

    let mut request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
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
    };
    inject_language_hint(&mut request.messages, &lang);

    let response = provider
        .complete(&request)
        .await
        .cmd_err("ai_parse_filter")?;

    let mut filters: Vec<crate::services::query_executor::FilterCondition> = parse_ai_json(
        &response.content,
        response.finish_reason.as_deref(),
        "ai_parse_filter",
    )?;

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

#[tauri::command]
pub async fn ai_parse_filter(
    state: State<'_, AppState>,
    db_session_id: String,
    database: String,
    table: String,
    natural_language: String,
) -> Result<Vec<crate::services::query_executor::FilterCondition>, CommandError> {
    ai_parse_filter_impl(&state, db_session_id, database, table, natural_language).await
}

// ─── Database Tool Definitions & Execution ───

fn db_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "list_connections".into(),
            description: "List all configured database connections with their IDs, names, database types, and hosts. Call this first to discover available data sources.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "list_databases".into(),
            description: "List all databases on a connected database server.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string", "description": "The connection ID from list_connections" }
                },
                "required": ["connection_id"]
            }),
        },
        ToolDefinition {
            name: "list_tables".into(),
            description: "List all tables in a database with their types and row counts.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string", "description": "The connection ID from list_connections" },
                    "database": { "type": "string", "description": "Database name (optional for some database types)" }
                },
                "required": ["connection_id"]
            }),
        },
        ToolDefinition {
            name: "search_tables".into(),
            description: "Search for tables by name pattern (case-insensitive substring match). Use this instead of list_tables when the database has many tables and you need to find specific ones.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string", "description": "The connection ID from list_connections" },
                    "database": { "type": "string", "description": "Database name" },
                    "pattern": { "type": "string", "description": "Search keyword to match against table names" },
                    "limit": { "type": "integer", "description": "Max results to return (default 20)", "default": 20 }
                },
                "required": ["connection_id", "pattern"]
            }),
        },
        ToolDefinition {
            name: "get_table_schema".into(),
            description: "Get detailed schema of one or more tables, including column names, data types, primary keys, foreign keys, and indexes. Supports batch queries for multiple tables.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string", "description": "The connection ID from list_connections" },
                    "tables": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "One or more table names to get schema for"
                    }
                },
                "required": ["connection_id", "tables"]
            }),
        },
    ]
}

/// Convert connected MCP client tools into AI `ToolDefinition`s (Phase 3 wires into chat).
pub(crate) async fn mcp_tool_definitions(state: &AppState) -> Vec<ToolDefinition> {
    let settings = state.store.get_settings().await;
    state
        .mcp_client_manager
        .all_tools()
        .await
        .into_iter()
        .filter(|tool| {
            settings
                .mcp_client_servers
                .iter()
                .find(|c| c.id == tool.server_id)
                .map(|c| c.enabled_for_ai)
                .unwrap_or(true)
        })
        .map(|tool| ToolDefinition {
            name: tool.qualified_name,
            description: tool.description.unwrap_or_else(|| tool.tool_name.clone()),
            parameters: tool.input_schema,
        })
        .collect()
}

fn is_db_tool(name: &str) -> bool {
    matches!(
        name,
        "list_connections"
            | "list_databases"
            | "list_tables"
            | "search_tables"
            | "get_table_schema"
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ToolKind {
    AskQuestions,
    Db(String),
    Mcp {
        server_id: String,
        tool_name: String,
    },
    Unknown,
}

pub(crate) fn classify_tool(name: &str) -> ToolKind {
    if name == "ask_questions" {
        return ToolKind::AskQuestions;
    }
    if is_db_tool(name) {
        return ToolKind::Db(name.to_string());
    }
    if let Some(rest) = name.strip_prefix("mcp/") {
        if let Some((server_id, tool_name)) = rest.split_once('/') {
            if !server_id.is_empty() && !tool_name.is_empty() {
                return ToolKind::Mcp {
                    server_id: server_id.to_string(),
                    tool_name: tool_name.to_string(),
                };
            }
        }
    }
    ToolKind::Unknown
}

pub(crate) async fn execute_mcp_tool(
    state: &AppState,
    server_id: &str,
    tool_name: &str,
    arguments: &str,
) -> String {
    let qualified = crate::mcp::client::mcp_qualified_name(server_id, tool_name);
    let args: serde_json::Value = match serde_json::from_str(arguments) {
        Ok(v) => v,
        Err(e) => {
            return format!("MCP tool error ({qualified}): invalid JSON arguments: {e}");
        }
    };

    match state
        .mcp_client_manager
        .call_tool(server_id, tool_name, args)
        .await
    {
        Ok(result) => crate::mcp::format_call_tool_result(&result, &qualified),
        Err(msg) => format!("MCP tool error ({qualified}): {msg}"),
    }
}

async fn execute_db_tool(state: &AppState, tool_call: &ToolCall) -> String {
    let args: serde_json::Value = serde_json::from_str(&tool_call.arguments).unwrap_or_default();
    let args_str = args.to_string();
    tracing::info!(tool = %tool_call.name, args_len = args_str.len(), "execute_db_tool");
    tracing::debug!(tool = %tool_call.name, args = %args, "execute_db_tool args");

    let cm = &state.connection_manager;
    let result = match tool_call.name.as_str() {
        "list_connections" => crate::services::db_tools::list_connections(&state.store).await,
        "list_databases" => {
            let connection_id = args["connection_id"].as_str().unwrap_or("");
            crate::services::db_tools::list_databases(cm, connection_id).await
        }
        "list_tables" => {
            let connection_id = args["connection_id"].as_str().unwrap_or("");
            let database = args["database"].as_str().unwrap_or("");
            crate::services::db_tools::list_tables(cm, connection_id, database).await
        }
        "search_tables" => {
            let connection_id = args["connection_id"].as_str().unwrap_or("");
            let database = args["database"].as_str().unwrap_or("");
            let pattern = args["pattern"].as_str().unwrap_or("");
            let limit = args["limit"].as_u64().unwrap_or(20) as usize;
            crate::services::db_tools::search_tables(cm, connection_id, database, pattern, limit)
                .await
        }
        "get_table_schema" => {
            let connection_id = args["connection_id"].as_str().unwrap_or("");
            let tables: Vec<String> = args["tables"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            crate::services::db_tools::get_table_schema(cm, connection_id, &tables).await
        }
        other => Err(format!("Unknown tool: {other}")),
    };
    result.unwrap_or_else(|e| e)
}

struct StreamRoundResult {
    content: String,
    reasoning: String,
    tool_calls: Option<Vec<ToolCall>>,
    usage: Option<TokenUsage>,
    had_error: bool,
    response_id: Option<String>,
}

async fn run_streaming_tool_loop(
    provider: Arc<dyn AiProvider>,
    state: &AppState,
    on_chunk: StreamCallback,
    request_id: &str,
    mut request: CompletionRequest,
    max_rounds: usize,
    cmd_label: &str,
) -> Result<String, CommandError> {
    for round in 0..max_rounds {
        let (tx, mut rx) = mpsc::channel::<Result<StreamChunk, AiError>>(32);

        let (result_tx, result_rx) = tokio::sync::oneshot::channel::<StreamRoundResult>();
        let on_chunk_c = on_chunk.clone();
        let rid_c = request_id.to_string();

        tokio::spawn(async move {
            let mut full_content = String::new();
            let mut full_reasoning = String::new();
            let mut final_tool_calls: Option<Vec<ToolCall>> = None;
            let mut final_usage = None;
            let mut final_response_id = None;
            let mut had_error = false;

            while let Some(chunk_result) = rx.recv().await {
                match chunk_result {
                    Ok(chunk) => {
                        if chunk.done {
                            let content = chunk.content;
                            let reasoning = chunk.reasoning;
                            full_content.push_str(&content);
                            if let Some(r) = &reasoning {
                                full_reasoning.push_str(r);
                            }
                            final_tool_calls = chunk.tool_calls;
                            final_usage = chunk.usage;
                            final_response_id = chunk.response_id;
                            if !content.is_empty() || reasoning.is_some() {
                                on_chunk_c(
                                    &rid_c,
                                    Ok(StreamChunk {
                                        content,
                                        reasoning,
                                        done: false,
                                        usage: None,
                                        tool_calls: None,
                                        response_id: None,
                                    }),
                                );
                            }
                        } else {
                            full_content.push_str(&chunk.content);
                            if let Some(r) = &chunk.reasoning {
                                full_reasoning.push_str(r);
                            }
                            on_chunk_c(&rid_c, Ok(chunk));
                        }
                    }
                    Err(e) => {
                        on_chunk_c(&rid_c, Err(e));
                        had_error = true;
                        break;
                    }
                }
            }

            let _ = result_tx.send(StreamRoundResult {
                content: full_content,
                reasoning: full_reasoning,
                tool_calls: final_tool_calls,
                usage: final_usage,
                had_error,
                response_id: final_response_id,
            });
        });

        provider
            .stream_complete(&request, tx)
            .await
            .cmd_err(cmd_label)?;

        let result = result_rx
            .await
            .map_err(|_| CommandError::Internal("Stream result channel closed".into()))?;

        if result.had_error {
            return Ok(request_id.to_string());
        }

        let all_tcs = match result.tool_calls {
            Some(tcs) if !tcs.is_empty() => tcs,
            _ => {
                on_chunk(
                    request_id,
                    Ok(StreamChunk {
                        content: String::new(),
                        reasoning: None,
                        done: true,
                        usage: result.usage,
                        tool_calls: result.tool_calls,
                        response_id: result.response_id,
                    }),
                );
                return Ok(request_id.to_string());
            }
        };

        let classified: Vec<(ToolCall, ToolKind)> = all_tcs
            .into_iter()
            .map(|tc| {
                let kind = classify_tool(&tc.name);
                (tc, kind)
            })
            .collect();

        let executable_count = classified
            .iter()
            .filter(|(_, kind)| matches!(kind, ToolKind::Db(_) | ToolKind::Mcp { .. }))
            .count();

        let has_ask_questions = classified
            .iter()
            .any(|(_, kind)| matches!(kind, ToolKind::AskQuestions));
        let all_ask_questions = classified
            .iter()
            .all(|(_, kind)| matches!(kind, ToolKind::AskQuestions));

        if executable_count == 0 && all_ask_questions {
            on_chunk(
                request_id,
                Ok(StreamChunk {
                    content: String::new(),
                    reasoning: None,
                    done: true,
                    usage: result.usage,
                    tool_calls: Some(classified.iter().map(|(tc, _)| tc.clone()).collect()),
                    response_id: result.response_id,
                }),
            );
            return Ok(request_id.to_string());
        }

        tracing::info!(
            %request_id,
            round,
            executable_count,
            has_ask_questions,
            tool_names = ?classified.iter().map(|(t, _)| t.name.as_str()).collect::<Vec<_>>(),
            response_id = ?result.response_id,
            "{cmd_label}: executing tools (round {round})"
        );
        tracing::debug!(
            %request_id,
            round,
            tools = ?classified.iter().map(|(t, _)| format!("{}({})", t.name, t.arguments)).collect::<Vec<_>>(),
            "{cmd_label}: tool arguments"
        );

        request.messages.push(ChatMessage {
            role: MessageRole::Assistant,
            content: result.content,
            reasoning: if result.reasoning.is_empty() {
                None
            } else {
                Some(result.reasoning)
            },
            tool_calls: Some(classified.iter().map(|(tc, _)| tc.clone()).collect()),
            tool_call_id: None,
        });

        let mcp_tool_calls: Vec<ToolCall> = classified
            .iter()
            .filter(|(_, kind)| matches!(kind, ToolKind::Mcp { .. }))
            .map(|(tc, _)| tc.clone())
            .collect();
        if !mcp_tool_calls.is_empty() {
            on_chunk(
                request_id,
                Ok(StreamChunk {
                    content: String::new(),
                    reasoning: None,
                    done: false,
                    usage: None,
                    tool_calls: Some(mcp_tool_calls),
                    response_id: None,
                }),
            );
        }

        for (tc, kind) in &classified {
            let tool_result = match kind {
                ToolKind::AskQuestions => continue,
                ToolKind::Db(_) => execute_db_tool(state, tc).await,
                ToolKind::Mcp {
                    server_id,
                    tool_name,
                } => execute_mcp_tool(state, server_id, tool_name, &tc.arguments).await,
                ToolKind::Unknown => format!("Unknown tool: {}", tc.name),
            };
            request.messages.push(ChatMessage {
                role: MessageRole::Tool,
                content: tool_result,
                reasoning: None,
                tool_calls: None,
                tool_call_id: Some(tc.id.clone()),
            });
        }

        if has_ask_questions {
            let ask_tool_calls: Vec<ToolCall> = classified
                .iter()
                .filter(|(_, kind)| matches!(kind, ToolKind::AskQuestions))
                .map(|(tc, _)| tc.clone())
                .collect();
            on_chunk(
                request_id,
                Ok(StreamChunk {
                    content: String::new(),
                    reasoning: None,
                    done: true,
                    usage: result.usage,
                    tool_calls: Some(ask_tool_calls),
                    response_id: result.response_id,
                }),
            );
            return Ok(request_id.to_string());
        }
    }

    tracing::warn!(%request_id, "{cmd_label}: reached max tool rounds");
    on_chunk(
        request_id,
        Ok(StreamChunk {
            content: String::new(),
            reasoning: None,
            done: true,
            usage: None,
            tool_calls: None,
            response_id: None,
        }),
    );
    Ok(request_id.to_string())
}

// ─── AI Chat ───

pub(crate) async fn ai_chat_impl(
    state: &AppState,
    on_chunk: StreamCallback,
    db_session_id: Option<String>,
    database: Option<String>,
    messages: Vec<ChatMessage>,
    request_id: String,
    include_schema: bool,
    scenario: Option<String>,
    context_files: Option<Vec<String>>,
    context_tables: Option<Vec<String>>,
) -> Result<String, CommandError> {
    let is_workflow = scenario.as_deref() == Some("workflow_generate");
    let prompt_scenario = if is_workflow {
        PromptScenario::WorkflowGenerate
    } else {
        PromptScenario::Chat
    };

    tracing::info!(
        %request_id,
        db_session_id = ?db_session_id,
        database = ?database,
        messages_count = messages.len(),
        %include_schema,
        scenario = ?scenario,
        context_tables_count = context_tables.as_ref().map(|t| t.len()).unwrap_or(0),
        last_user_msg_len = messages.last().map(|m| m.content.len()).unwrap_or(0),
        "ai_chat: start"
    );
    if let Some(last) = messages.last() {
        tracing::debug!(
            %request_id,
            last_user_msg = %truncate_str(&last.content, 100),
            "ai_chat: last user message"
        );
    }
    let (provider, ai_config) = resolve_ai(&state).await?;

    let lang = state.store.get_settings().await.language;
    let mut full_messages: Vec<ChatMessage> = Vec::new();
    let mut attach_db_tools = true;

    if include_schema {
        if let Some(ref conn_id) = db_session_id {
            let db = database.as_deref().unwrap_or("");
            if let Ok((driver, _handle)) = state.connection_manager.get_session(conn_id).await {
                let pinned = context_tables.clone().unwrap_or_default();
                let supports_tools = provider.supports_tools();
                let pipeline = SchemaContextPipeline::new(state.schema_context_builder.clone());
                match pipeline
                    .resolve(
                        conn_id,
                        db,
                        &pinned,
                        supports_tools,
                        budget::PINNED_DDL,
                        budget::FALLBACK_DDL,
                    )
                    .await
                {
                    Ok(seed) => {
                        attach_db_tools = seed.attach_db_tools;
                        let db_type = seed.database_type.clone();
                        let suffix = compose_schema_system_suffix(&seed);

                        let mut vars = HashMap::new();
                        vars.insert("db_type", db_type.as_str());
                        vars.insert("schema", "");

                        let connections_ctx = if is_workflow {
                            build_connections_context(&state, &lang).await
                        } else {
                            String::new()
                        };
                        vars.insert("connections", connections_ctx.as_str());

                        let base_tpl = state
                            .prompt_resolver
                            .resolve(prompt_scenario, Some(driver.as_ref()), &lang)
                            .await;
                        let base = crate::ai::prompt_resolver::render_template(&base_tpl, &vars);

                        let desc = if is_workflow {
                            base
                        } else if lang.starts_with("zh") {
                            format!("{base}\n\n用户已连接到 {db_type} 数据库。")
                        } else {
                            format!("{base}\n\nThe user is connected to a {db_type} database.")
                        };

                        full_messages.push(ChatMessage {
                            role: MessageRole::System,
                            content: format!("{desc}\n\n{suffix}"),
                            reasoning: None,
                            tool_calls: None,
                            tool_call_id: None,
                        });
                    }
                    Err(e) => {
                        tracing::warn!(
                            %request_id,
                            db_session_id = %conn_id,
                            database = %db,
                            error = %e,
                            "ai_chat: schema context pipeline resolve failed; disabling DB tools"
                        );
                        attach_db_tools = false;
                    }
                }
            }
        }
    }

    if full_messages.is_empty() {
        if is_workflow {
            let connections_ctx = build_connections_context(&state, &lang).await;
            let mut vars = HashMap::new();
            vars.insert("connections", connections_ctx.as_str());
            vars.insert("schema", "");
            vars.insert("db_type", "");
            let tpl = state
                .prompt_resolver
                .resolve(prompt_scenario, None, &lang)
                .await;
            let prompt = crate::ai::prompt_resolver::render_template(&tpl, &vars);
            full_messages.push(ChatMessage {
                role: MessageRole::System,
                content: prompt,
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            });
        } else {
            let chat_prompt = state
                .prompt_resolver
                .resolve(prompt_scenario, None, &lang)
                .await;
            full_messages.push(ChatMessage {
                role: MessageRole::System,
                content: chat_prompt,
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            });
        }
    }

    full_messages.extend(messages);

    // Inject context files into last user message
    if let Some(ref ctx_files) = context_files {
        if !ctx_files.is_empty() {
            let ctx_dir = super::context::resolve_context_dir_from_state(&state).await?;
            let entries = super::context::read_context_paths(&ctx_dir, ctx_files).await?;
            if !entries.is_empty() {
                if let Some(last_user) = full_messages
                    .iter_mut()
                    .rev()
                    .find(|m| m.role == MessageRole::User)
                {
                    let context_block = super::context::format_context_block(&entries);
                    last_user.content = format!("{context_block}\n\n{}", last_user.content);
                }
            }
        }
    }

    let ask_questions_tool = ToolDefinition {
        name: "ask_questions".into(),
        description: "Ask the user structured questions to gather information. Use when you need the user to choose between options or provide specific input.".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string", "description": "Unique question identifier" },
                            "prompt": { "type": "string", "description": "The question text" },
                            "options": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "id": { "type": "string" },
                                        "label": { "type": "string" }
                                    },
                                    "required": ["id", "label"]
                                },
                                "description": "Predefined options. Can be empty for free-text input."
                            },
                            "allowMultiple": { "type": "boolean", "description": "Allow selecting multiple options", "default": false }
                        },
                        "required": ["id", "prompt"]
                    }
                }
            },
            "required": ["questions"]
        }),
    };

    let mut all_tools = vec![ask_questions_tool];
    if attach_db_tools {
        all_tools.extend(db_tool_definitions());
    }
    if provider.supports_tools() {
        all_tools.extend(mcp_tool_definitions(state).await);
    }

    let mut request = CompletionRequest {
        request_id: request_id.clone(),
        model: ai_config.model.clone(),
        messages: full_messages,
        temperature: Some(0.7),
        stop: None,
        tools: Some(all_tools),
        previous_response_id: None,
    };
    inject_language_hint(&mut request.messages, &lang);

    run_streaming_tool_loop(
        provider,
        state,
        on_chunk,
        &request_id,
        request,
        10,
        "ai_chat",
    )
    .await
}

#[tauri::command]
pub async fn ai_chat(
    state: State<'_, AppState>,
    window: WebviewWindow,
    db_session_id: Option<String>,
    database: Option<String>,
    messages: Vec<ChatMessage>,
    request_id: String,
    include_schema: bool,
    scenario: Option<String>,
    context_files: Option<Vec<String>>,
    context_tables: Option<Vec<String>>,
) -> Result<String, CommandError> {
    ai_chat_impl(
        &state,
        window_stream_callback(&window),
        db_session_id,
        database,
        messages,
        request_id,
        include_schema,
        scenario,
        context_files,
        context_tables,
    )
    .await
}

async fn build_connections_context(state: &AppState, lang: &str) -> String {
    let conns = state.store.get_connections().await;
    if conns.is_empty() {
        return String::new();
    }
    let mut lines = Vec::new();
    let header = if lang.starts_with("zh") {
        "用户有以下可用的数据库连接："
    } else {
        "The user has the following database connections available:"
    };
    lines.push(header.to_string());
    for c in &conns {
        lines.push(format!(
            "- \"{}\" ({:?}) — id: {}",
            c.name, c.database_type, c.id
        ));
    }
    lines.join("\n")
}

fn truncate_str(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn parse_ai_json<T: serde::de::DeserializeOwned>(
    raw: &str,
    finish_reason: Option<&str>,
    cmd: &str,
) -> Result<T, CommandError> {
    let content = strip_markdown_fences(raw);
    if content.trim().is_empty() {
        tracing::error!(cmd, "LLM returned empty response");
        return Err(CommandError::Internal("LLM returned empty response".into()));
    }

    if let Ok(val) = serde_json::from_str::<T>(&content) {
        return Ok(val);
    }

    if let Some(extracted) = extract_json_boundary(&content) {
        if let Ok(val) = serde_json::from_str::<T>(extracted) {
            tracing::debug!(cmd, "Parsed JSON after extracting from mixed content");
            return Ok(val);
        }
    }

    let Err(err) = serde_json::from_str::<serde_json::Value>(&content) else {
        return Err(CommandError::Internal(
            "AI response JSON structure does not match the expected schema.".into(),
        ));
    };
    tracing::error!(
        cmd,
        raw_content = %truncate_str(&content, 500),
        ?finish_reason,
        "JSON parse failed: {err}"
    );
    let is_truncated = matches!(finish_reason, Some("length") | Some("max_tokens"));
    if is_truncated {
        Err(CommandError::Internal(
            "AI response was truncated due to max_tokens limit. \
             Please increase the \"Max Tokens\" setting in AI configuration."
                .into(),
        ))
    } else {
        Err(CommandError::Internal(format!(
            "Failed to parse AI response. The model may have returned an invalid format. \
             Try again or increase Max Tokens in settings. (detail: {err})"
        )))
    }
}

/// Extract the first complete JSON object `{...}` or array `[...]` from text
/// that may contain trailing non-JSON content (e.g. model reasoning).
fn extract_json_boundary(s: &str) -> Option<&str> {
    let trimmed = s.trim();
    let (open, close) = if trimmed.starts_with('{') {
        ('{', '}')
    } else if trimmed.starts_with('[') {
        ('[', ']')
    } else {
        return None;
    };

    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, ch) in trimmed.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape_next = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return Some(&trimmed[..=i]);
            }
        }
    }
    None
}

pub(crate) fn emit_stream_chunk_or_error<R: tauri::Runtime>(
    emitter: &impl Emitter<R>,
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
            if let Some(tool_calls) = &chunk.tool_calls {
                payload["toolCalls"] = serde_json::to_value(tool_calls).unwrap_or_default();
            }
            let _ = emitter.emit("ai:stream-chunk", payload);
        }
        Err(e) => {
            let _ = emitter.emit(
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

// ─── Workflow IPC commands ───

pub(crate) async fn workflow_list_impl(
    state: &AppState,
) -> Result<Vec<crate::workflow::WorkflowListItem>, CommandError> {
    Ok(state.workflow_registry.list().await)
}

#[tauri::command]
pub async fn workflow_list(
    state: State<'_, AppState>,
) -> Result<Vec<crate::workflow::WorkflowListItem>, CommandError> {
    workflow_list_impl(&state).await
}

pub(crate) async fn workflow_execute_impl(
    state: &AppState,
    workflow_id: String,
    variables: serde_json::Value,
    connection_id: Option<String>,
) -> Result<crate::workflow::WorkflowExecutionResult, CommandError> {
    let workflow = state
        .workflow_registry
        .get(&workflow_id)
        .await
        .ok_or_else(|| CommandError::NotFound(format!("Workflow '{workflow_id}' not found")))?;

    let result = crate::workflow::WorkflowExecutor::execute(
        &workflow,
        state,
        connection_id.as_deref(),
        &variables,
    )
    .await
    .cmd_err("workflow_execute")?;

    // Dashboard-owned hidden workflows must not pollute the user-facing history list.
    if workflow.visibility != crate::workflow::WorkflowVisibility::DashboardHidden {
        if let Err(e) = state
            .workflow_history
            .record(&workflow.id, &workflow.name, &variables, &result)
            .await
        {
            tracing::warn!("Failed to record workflow history: {e}");
        }
    }

    Ok(result)
}

#[tauri::command]
pub async fn workflow_execute(
    state: State<'_, AppState>,
    workflow_id: String,
    variables: serde_json::Value,
    connection_id: Option<String>,
) -> Result<crate::workflow::WorkflowExecutionResult, CommandError> {
    workflow_execute_impl(&state, workflow_id, variables, connection_id).await
}

#[tauri::command]
pub async fn workflow_save(
    state: State<'_, AppState>,
    workflow: crate::workflow::WorkflowDefinition,
) -> Result<(), CommandError> {
    state
        .workflow_registry
        .save_workflow(&workflow)
        .await
        .map_err(CommandError::Internal)
}

#[tauri::command]
pub async fn workflow_save_yaml(
    state: State<'_, AppState>,
    yaml: String,
) -> Result<crate::workflow::WorkflowDefinition, CommandError> {
    state
        .workflow_registry
        .save_workflow_yaml(&yaml)
        .await
        .map_err(CommandError::Validation)
}

#[tauri::command]
pub async fn workflow_get_yaml(
    state: State<'_, AppState>,
    workflow_id: String,
) -> Result<String, CommandError> {
    let record = state
        .workflow_registry
        .app_db()
        .get_workflow(&workflow_id)
        .map_err(|e| match e {
            crate::store::AppDbError::NotFound(id) => {
                CommandError::NotFound(format!("Workflow '{id}' not found"))
            }
            other => CommandError::Internal(other.to_string()),
        })?;
    Ok(record.definition_yaml)
}

#[tauri::command]
pub async fn workflow_delete(
    state: State<'_, AppState>,
    workflow_id: String,
) -> Result<(), CommandError> {
    state
        .workflow_registry
        .delete_workflow(&workflow_id)
        .await
        .map_err(|msg| CommandError::Validation(msg))
}

#[tauri::command]
pub async fn workflow_reload(state: State<'_, AppState>) -> Result<(), CommandError> {
    state
        .workflow_registry
        .load_all()
        .await
        .map_err(CommandError::Internal)
}

#[tauri::command]
pub async fn workflow_get_dir(state: State<'_, AppState>) -> Result<String, CommandError> {
    Ok(state
        .workflow_registry
        .workflows_dir()
        .display()
        .to_string())
}

#[tauri::command]
pub async fn workflow_get(
    state: State<'_, AppState>,
    workflow_id: String,
) -> Result<crate::workflow::WorkflowDefinition, CommandError> {
    state
        .workflow_registry
        .get(&workflow_id)
        .await
        .ok_or_else(|| CommandError::NotFound(format!("Workflow '{workflow_id}' not found")))
}

// ─── Workflow History ───

#[tauri::command]
pub async fn workflow_history_list(
    state: State<'_, AppState>,
    workflow_id: Option<String>,
) -> Result<Vec<crate::workflow::HistoryListItem>, CommandError> {
    Ok(state.workflow_history.list(workflow_id.as_deref()).await)
}

#[tauri::command]
pub async fn workflow_history_get(
    state: State<'_, AppState>,
    history_id: String,
) -> Result<crate::workflow::HistoryEntry, CommandError> {
    state
        .workflow_history
        .get(&history_id)
        .await
        .ok_or_else(|| CommandError::NotFound(format!("History '{history_id}' not found")))
}

#[tauri::command]
pub async fn workflow_history_clear(
    state: State<'_, AppState>,
    workflow_id: Option<String>,
) -> Result<usize, CommandError> {
    state
        .workflow_history
        .clear(workflow_id.as_deref())
        .await
        .cmd_err("workflow_history_clear")
}

// ─── Phase 8: Schema documentation ───

pub(crate) async fn ai_generate_schema_doc_impl(
    state: &AppState,
    db_session_id: String,
    database: String,
) -> Result<String, CommandError> {
    tracing::info!(%db_session_id, %database, "ai_generate_schema_doc: start");
    let (provider, ai_config) = resolve_ai(&state).await?;

    // Step 1: Get table names only (no column details)
    let (db_type, all_table_names) = state
        .schema_context_builder
        .get_table_names(&db_session_id, &database)
        .await
        .cmd_err("ai_generate_schema_doc")?;

    let lang = state.store.get_settings().await.language;

    let (driver_ref, _) = state
        .connection_manager
        .get_session(&db_session_id)
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
        let select_tpl = state
            .prompt_resolver
            .resolve(
                PromptScenario::SchemaDocSelectTables,
                Some(driver_ref.as_ref()),
                &lang,
            )
            .await;
        let select_content = prompt_resolver::render_template(&select_tpl, &select_vars);

        let select_request = CompletionRequest {
            request_id: Uuid::new_v4().to_string(),
            model: ai_config.model.clone(),
            messages: vec![
                ChatMessage {
                    role: MessageRole::System,
                    content: select_content,
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
        .build_selective_context(
            &db_session_id,
            &database,
            &selected_tables,
            budget::SCHEMA_DOC,
        )
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
    let doc_tpl = state
        .prompt_resolver
        .resolve(PromptScenario::SchemaDoc, Some(driver_ref.as_ref()), &lang)
        .await;
    let doc_system_content = prompt_resolver::render_template(&doc_tpl, &doc_vars);

    let mut request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
        messages: vec![
            ChatMessage {
                role: MessageRole::System,
                content: doc_system_content,
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

#[tauri::command]
pub async fn ai_generate_schema_doc(
    state: State<'_, AppState>,
    db_session_id: String,
    database: String,
) -> Result<String, CommandError> {
    ai_generate_schema_doc_impl(&state, db_session_id, database).await
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
    ai_diagnose_connection_impl(&state, connection_id, error_message).await
}

pub(crate) async fn ai_diagnose_connection_impl(
    state: &AppState,
    connection_id: String,
    error_message: String,
) -> Result<ConnectionDiagnosis, CommandError> {
    tracing::info!(%connection_id, error_len = error_message.len(), "ai_diagnose_connection: start");
    tracing::debug!(%connection_id, error = %error_message, "ai_diagnose_connection: error");
    let (provider, ai_config) = resolve_ai(&state).await?;

    let conn_info = state
        .store
        .get_connection(&connection_id)
        .await
        .ok_or_else(|| CommandError::NotFound("Connection config not found".into()))?;

    let ssl_str = format!("{:?}", conn_info.ssl_mode);
    let ssh_str = if conn_info.ssh_tunnel.is_some() {
        "enabled"
    } else {
        "disabled"
    };
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
    let conn_diag_prompt = state
        .prompt_resolver
        .resolve(PromptScenario::ConnectionDiagnose, None, &lang)
        .await;

    let mut request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
        messages: vec![
            ChatMessage {
                role: MessageRole::System,
                content: conn_diag_prompt,
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: MessageRole::User,
                content: format!("Connection details:\n{conn_summary}\n\nError:\n{error_message}"),
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ],
        temperature: Some(0.0),

        stop: None,
        tools: None,
        previous_response_id: None,
    };
    inject_language_hint(&mut request.messages, &lang);

    let response = provider
        .complete(&request)
        .await
        .cmd_err("ai_diagnose_connection")?;

    parse_ai_json::<ConnectionDiagnosis>(
        &response.content,
        response.finish_reason.as_deref(),
        "ai_diagnose_connection",
    )
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

pub(crate) async fn ai_analyze_queries_impl(
    state: &AppState,
    db_session_id: Option<String>,
) -> Result<QueryAnalysis, CommandError> {
    tracing::info!(db_session_id = ?db_session_id, "ai_analyze_queries: start");
    let (provider, ai_config) = resolve_ai(&state).await?;

    // The IPC parameter is the runtime dbSessionId; resolve the persisted
    // connection id that owns it so history is filtered by connection.
    let owner_connection_id = if let Some(ref cid) = db_session_id {
        state.connection_manager.owner_connection_id(cid).await
    } else {
        None
    };
    let history = state
        .store
        .get_query_history(200, owner_connection_id.as_deref(), None, None)
        .await;
    let filtered: Vec<_> = history.iter().collect();

    if filtered.is_empty() {
        return Err(CommandError::Validation(
            "No query history available".into(),
        ));
    }

    let queries_text = filtered
        .iter()
        .take(100)
        .map(|h| h.sql.as_str())
        .collect::<Vec<_>>()
        .join("\n---\n");

    let lang = state.store.get_settings().await.language;
    let query_summary_prompt = state
        .prompt_resolver
        .resolve(PromptScenario::QuerySummary, None, &lang)
        .await;

    let mut request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
        messages: vec![
            ChatMessage {
                role: MessageRole::System,
                content: query_summary_prompt,
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: MessageRole::User,
                content: format!(
                    "Analyze these {} queries:\n\n{queries_text}",
                    filtered.len().min(100),
                ),
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ],
        temperature: Some(0.2),

        stop: None,
        tools: None,
        previous_response_id: None,
    };
    inject_language_hint(&mut request.messages, &lang);

    let response = provider
        .complete(&request)
        .await
        .cmd_err("ai_analyze_queries")?;

    parse_ai_json::<QueryAnalysis>(
        &response.content,
        response.finish_reason.as_deref(),
        "ai_analyze_queries",
    )
}

#[tauri::command]
pub async fn ai_analyze_queries(
    state: State<'_, AppState>,
    db_session_id: Option<String>,
) -> Result<QueryAnalysis, CommandError> {
    ai_analyze_queries_impl(&state, db_session_id).await
}

// ─── Prompt management IPC commands ───

pub(crate) async fn prompt_list_impl(
    state: &AppState,
    driver_type: Option<String>,
) -> Result<Vec<PromptInfo>, CommandError> {
    state.ensure_ai_ready().await;
    let driver: Option<std::sync::Arc<dyn datazen_driver_api::DatabaseDriver>> =
        if let Some(ref dt) = driver_type {
            state.driver_registry.get_sql_driver_by_name(dt).await
        } else {
            None
        };
    let prompts = state.prompt_resolver.list_prompts(driver.as_deref()).await;
    Ok(prompts)
}

#[tauri::command]
pub async fn prompt_list(
    state: State<'_, AppState>,
    driver_type: Option<String>,
) -> Result<Vec<PromptInfo>, CommandError> {
    prompt_list_impl(&state, driver_type).await
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
#[path = "ai_integration_tests.rs"]
mod ai_integration_tests;

#[cfg(test)]
#[path = "ai_mock_provider_tests.rs"]
mod ai_mock_provider_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_list_item_serialization() {
        let item = ProviderListItem {
            provider_type: AiProviderType::OpenAi,
            display_name: "OpenAI".into(),
            supports_streaming: true,
            supports_tools: true,
            default_endpoint: "https://api.openai.com/v1".into(),
            default_protocol: "open_ai_compatible".into(),
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"providerType\":\"open_ai\""));
        assert!(json.contains("\"displayName\":\"OpenAI\""));
        assert!(json.contains("\"supportsStreaming\":true"));
        assert!(json.contains("\"defaultEndpoint\":"));
        assert!(json.contains("\"defaultProtocol\":"));
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
    fn test_extract_json_boundary_object_with_trailing() {
        let input = r#"{"key":"val"}The user wants me to..."#;
        assert_eq!(extract_json_boundary(input), Some(r#"{"key":"val"}"#));
    }

    #[test]
    fn test_extract_json_boundary_array_with_trailing() {
        let input = r#"[{"a":1}]Some reasoning text"#;
        assert_eq!(extract_json_boundary(input), Some(r#"[{"a":1}]"#));
    }

    #[test]
    fn test_extract_json_boundary_nested() {
        let input = r#"{"a":{"b":"c"},"d":[1,2]}trailing"#;
        assert_eq!(
            extract_json_boundary(input),
            Some(r#"{"a":{"b":"c"},"d":[1,2]}"#)
        );
    }

    #[test]
    fn test_extract_json_boundary_with_escaped_quotes() {
        let input = r#"{"msg":"say \"hello\""}extra"#;
        assert_eq!(
            extract_json_boundary(input),
            Some(r#"{"msg":"say \"hello\""}"#)
        );
    }

    #[test]
    fn test_extract_json_boundary_no_json() {
        assert_eq!(extract_json_boundary("not json at all"), None);
    }

    #[test]
    fn test_extract_json_boundary_clean() {
        let input = r#"{"key":"val"}"#;
        assert_eq!(extract_json_boundary(input), Some(input));
    }

    #[test]
    fn test_parse_ai_json_with_trailing_reasoning() {
        #[derive(serde::Deserialize)]
        struct Simple {
            key: String,
        }
        let raw = r#"{"key":"val"}The user wants me to analyze..."#;
        let result: Result<Simple, _> = parse_ai_json(raw, None, "test");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().key, "val");
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
        assert_eq!(
            result.solutions[1].command.as_deref(),
            Some("CREATE USER test")
        );
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

    #[test]
    fn test_is_db_tool() {
        assert!(is_db_tool("list_connections"));
        assert!(is_db_tool("list_databases"));
        assert!(is_db_tool("list_tables"));
        assert!(is_db_tool("get_table_schema"));
        assert!(!is_db_tool("ask_questions"));
        assert!(!is_db_tool("unknown_tool"));
        assert!(!is_db_tool(""));
    }

    #[test]
    fn test_classify_tool() {
        assert_eq!(classify_tool("ask_questions"), ToolKind::AskQuestions);
        assert_eq!(
            classify_tool("list_connections"),
            ToolKind::Db("list_connections".into())
        );
        assert_eq!(
            classify_tool("mcp/files/read_file"),
            ToolKind::Mcp {
                server_id: "files".into(),
                tool_name: "read_file".into(),
            }
        );
        assert_eq!(
            classify_tool("mcp/my-server/my_tool_name"),
            ToolKind::Mcp {
                server_id: "my-server".into(),
                tool_name: "my_tool_name".into(),
            }
        );
        assert_eq!(classify_tool("mcp/bad"), ToolKind::Unknown);
        assert_eq!(classify_tool("mcp//tool"), ToolKind::Unknown);
        assert_eq!(classify_tool("unknown_tool"), ToolKind::Unknown);
    }

    #[tokio::test]
    async fn execute_mcp_tool_errors_when_server_not_connected() {
        use crate::testing::app_state::TestAppState;

        let test = TestAppState::new().await;
        let out = execute_mcp_tool(&test.state, "missing", "search", r#"{"q":"x"}"#).await;
        assert!(out.contains("MCP tool error (mcp/missing/search)"));
        assert!(out.contains("not connected"));
    }

    #[tokio::test]
    async fn execute_mcp_tool_rejects_invalid_json() {
        use crate::testing::app_state::TestAppState;

        let test = TestAppState::new().await;
        let out = execute_mcp_tool(&test.state, "srv", "tool", "not-json").await;
        assert!(out.contains("MCP tool error (mcp/srv/tool)"));
        assert!(out.contains("invalid JSON arguments"));
    }

    #[test]
    fn test_db_tool_definitions_count() {
        let tools = db_tool_definitions();
        assert_eq!(tools.len(), 5);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"list_connections"));
        assert!(names.contains(&"list_databases"));
        assert!(names.contains(&"list_tables"));
        assert!(names.contains(&"get_table_schema"));
    }

    #[test]
    fn test_db_tool_definitions_have_valid_schemas() {
        for tool in db_tool_definitions() {
            assert!(!tool.name.is_empty());
            assert!(!tool.description.is_empty());
            assert!(tool.parameters.is_object());
            let obj = tool.parameters.as_object().unwrap();
            assert_eq!(obj.get("type").and_then(|v| v.as_str()), Some("object"));
        }
    }

    #[test]
    fn test_language_hint_known_locales() {
        assert!(language_hint("zh-CN").contains("Chinese (Simplified)"));
        assert!(language_hint("zh-TW").contains("Chinese (Traditional)"));
        assert!(language_hint("en").contains("English"));
        assert!(language_hint("ja").contains("Japanese"));
        assert!(language_hint("ko").contains("Korean"));
        assert!(language_hint("fr").contains("fr"));
    }

    #[test]
    fn test_inject_language_hint_appends_to_system() {
        let mut messages = vec![
            ChatMessage {
                role: MessageRole::System,
                content: "Base prompt".into(),
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: MessageRole::User,
                content: "Hi".into(),
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ];
        inject_language_hint(&mut messages, "zh-CN");
        assert!(messages[0].content.contains("Base prompt"));
        assert!(messages[0].content.contains("Chinese (Simplified)"));
        assert!(!messages[1].content.contains("Chinese"));
    }

    #[test]
    fn test_provider_defaults_all_types() {
        assert_eq!(
            provider_defaults(AiProviderType::OpenAi),
            ("https://api.openai.com/v1", "open_ai_compatible")
        );
        assert_eq!(
            provider_defaults(AiProviderType::Anthropic),
            ("https://api.anthropic.com", "anthropic_compatible")
        );
        assert_eq!(
            provider_defaults(AiProviderType::DeepSeek),
            ("https://api.deepseek.com", "open_ai_responses")
        );
        assert_eq!(
            provider_defaults(AiProviderType::Custom),
            ("", "open_ai_compatible")
        );
    }

    #[test]
    fn test_truncate_str_multibyte() {
        assert_eq!(truncate_str("héllo", 3), "hé");
        assert_eq!(truncate_str("你好世界", 7), "你好");
    }

    #[test]
    fn test_parse_ai_json_empty_response() {
        #[derive(Debug, serde::Deserialize)]
        #[allow(dead_code)]
        struct Simple {
            key: String,
        }
        let err = parse_ai_json::<Simple>("   ", None, "test").unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn test_parse_ai_json_truncated_finish_reason() {
        #[derive(Debug, serde::Deserialize)]
        #[allow(dead_code)]
        struct Simple {
            items: Vec<String>,
        }
        let raw = r#"{"items": ["unclosed"#;
        let err = parse_ai_json::<Simple>(raw, Some("length"), "test").unwrap_err();
        assert!(err.to_string().contains("truncated"));
    }

    #[test]
    fn test_parse_ai_json_invalid_with_reason() {
        #[derive(Debug, serde::Deserialize)]
        #[allow(dead_code)]
        struct NeedsField {
            required_field: String,
        }
        let raw = r#"{"wrong": "field"}"#;
        let err = parse_ai_json::<NeedsField>(raw, Some("stop"), "test").unwrap_err();
        assert!(err.to_string().contains("schema"));
    }

    #[test]
    fn test_language_hint_unknown_locale_uses_code() {
        assert!(language_hint("de").contains("de"));
    }

    #[test]
    fn test_emit_stream_chunk_builds_payload_via_callback() {
        use std::sync::{Arc, Mutex};

        #[derive(Default)]
        struct Captured {
            events: Vec<(String, serde_json::Value)>,
        }

        let captured = Arc::new(Mutex::new(Captured::default()));
        let cap = captured.clone();
        let cb: StreamCallback = Arc::new(move |request_id, result| match result {
            Ok(chunk) => {
                let mut payload = serde_json::json!({
                    "requestId": request_id,
                    "content": chunk.content,
                    "done": chunk.done,
                });
                if let Some(reasoning) = &chunk.reasoning {
                    payload["reasoning"] = serde_json::Value::String(reasoning.clone());
                }
                cap.lock()
                    .unwrap()
                    .events
                    .push(("ai:stream-chunk".into(), payload));
            }
            Err(e) => {
                cap.lock().unwrap().events.push((
                    "ai:stream-error".into(),
                    serde_json::json!({ "requestId": request_id, "error": e.to_string() }),
                ));
            }
        });

        cb(
            "req-1",
            Ok(StreamChunk {
                content: "hello".into(),
                reasoning: Some("think".into()),
                done: false,
                usage: None,
                tool_calls: None,
                response_id: None,
            }),
        );
        cb("req-2", Err(AiError::RequestFailed("bad".into())));

        let events = captured.lock().unwrap();
        assert_eq!(events.events.len(), 2);
        assert_eq!(events.events[0].0, "ai:stream-chunk");
        assert_eq!(events.events[0].1["content"], "hello");
        assert_eq!(events.events[0].1["reasoning"], "think");
        assert_eq!(events.events[1].0, "ai:stream-error");
    }

    #[tokio::test]
    async fn ai_providers_config_prompt_workflow_impl() {
        use crate::testing::app_state::TestAppState;
        use datazen_ai_api::AiProviderConfig;
        use datazen_ai_api::AiProviderType;

        let test = TestAppState::new().await;
        assert!(ai_get_config_impl(&test.state).await.unwrap().is_none());

        let providers = ai_get_providers_impl(&test.state).await.unwrap();
        assert!(!providers.is_empty());

        let prompts = prompt_list_impl(&test.state, None).await.unwrap();
        assert!(!prompts.is_empty());

        let workflows = workflow_list_impl(&test.state).await.unwrap();
        assert!(
            workflows.iter().any(|w| w.id.starts_with("builtin-")),
            "empty workflow dir should be seeded with builtin workflows, got: {workflows:?}"
        );

        let cfg = AiProviderConfig {
            provider_type: AiProviderType::OpenAi,
            api_key: Some("test-key".into()),
            model: "gpt-4".into(),
            endpoint: None,
            max_tokens: 4096,
            extra: serde_json::json!({}),
        };
        test.state.store.save_ai_config(&cfg).await.unwrap();
        assert!(ai_get_config_impl(&test.state).await.unwrap().is_some());
        ai_delete_config_impl(&test.state).await.unwrap();
        assert!(ai_get_config_impl(&test.state).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn ai_fetch_remote_models_rejects_unknown_protocol() {
        let err = ai_fetch_remote_models(
            "unknown_proto".into(),
            "https://example.com".into(),
            "key".into(),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("Unknown protocol"));
    }

    #[tokio::test]
    async fn ai_validate_config_rejects_missing_api_key() {
        use crate::testing::app_state::TestAppState;
        use datazen_ai_api::AiProviderConfig;
        use datazen_ai_api::AiProviderType;

        let test = TestAppState::new().await;
        let cfg = AiProviderConfig {
            provider_type: AiProviderType::OpenAi,
            api_key: None,
            model: "gpt-4".into(),
            endpoint: None,
            max_tokens: 4096,
            extra: serde_json::json!({}),
        };
        assert!(ai_validate_config_impl(&test.state, cfg).await.is_err());
    }

    #[tokio::test]
    async fn mcp_tool_definitions_empty_when_no_connections() {
        use crate::testing::app_state::TestAppState;

        let test = TestAppState::new().await;
        assert!(mcp_tool_definitions(&test.state).await.is_empty());
    }

    #[tokio::test]
    async fn mcp_tool_definitions_respects_enabled_for_ai() {
        use crate::mcp::{McpServerConfig, McpToolInfo};
        use crate::testing::app_state::TestAppState;
        use rmcp::model::{CallToolResult, ContentBlock, Tool};
        use std::collections::HashMap;
        use std::sync::Arc;

        let test = TestAppState::new().await;
        let schema = serde_json::json!({"type": "object"});
        let tool = Tool::new("ping", "Ping", schema.as_object().unwrap().clone());
        test.state
            .mcp_client_manager
            .register_test_server(
                "hidden_srv",
                "Hidden",
                vec![tool],
                Arc::new(|_, _| Ok(CallToolResult::success(vec![ContentBlock::text("pong")]))),
            )
            .await;

        let mut settings = test.state.store.get_settings().await;
        settings.mcp_client_servers = vec![McpServerConfig {
            id: "hidden_srv".into(),
            name: "Hidden".into(),
            transport: "stdio".into(),
            command: None,
            args: vec![],
            env: HashMap::new(),
            enabled: true,
            enabled_for_ai: false,
        }];
        test.state.store.save_settings(settings).await.unwrap();

        assert!(mcp_tool_definitions(&test.state).await.is_empty());

        let tools = test.state.mcp_client_manager.all_tools().await;
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].tool_name, "ping");
        let _info: McpToolInfo = tools[0].clone();
    }

    #[tokio::test]
    async fn mcp_connect_rejects_invalid_server_id() {
        use crate::mcp::McpServerConfig;
        use crate::testing::app_state::TestAppState;
        use std::collections::HashMap;

        let test = TestAppState::new().await;
        let config = McpServerConfig {
            id: "bad id".into(),
            name: "Bad".into(),
            transport: "stdio".into(),
            command: Some("/bin/echo".into()),
            args: vec![],
            env: HashMap::new(),
            enabled: true,
            enabled_for_ai: true,
        };
        let err = test
            .state
            .mcp_client_manager
            .connect(&config)
            .await
            .unwrap_err();
        assert!(err.contains("Invalid MCP server id"));
    }
}

#[cfg(test)]
mod ipc_contract_guards {
    //! D1 regression anchors: these commands receive a **runtime** db session
    //! id over IPC, so their wire parameter must be `db_session_id` (frontend
    //! camelCase `dbSessionId`). An earlier revision shipped them as
    //! `connection_id` (persisted-configuration semantics) while the bodies
    //! called strict runtime-session lookups — "renamed but reversed". The
    //! assertions below pin both directions so it cannot silently return.

    const SOURCE: &str = include_str!("ai.rs");

    /// Extracts the parameter list of a `pub async fn <command>(...)`.
    fn command_params(command: &str) -> String {
        let needle = format!("pub async fn {command}(");
        let start = SOURCE
            .find(&needle)
            .unwrap_or_else(|| panic!("command `{command}` not found in ai.rs"));
        let rest = &SOURCE[start + needle.len()..];
        let end = rest.find(')').expect("unterminated parameter list");
        rest[..end].to_string()
    }

    #[test]
    fn session_semantics_commands_take_db_session_id() {
        for cmd in [
            "ai_generate_sql",
            "ai_diagnose_error",
            "ai_analyze_explain",
            "ai_parse_filter",
            "ai_chat",
            "ai_generate_schema_doc",
            "ai_analyze_queries",
        ] {
            let params = command_params(cmd);
            assert!(
                params.contains("db_session_id"),
                "`{cmd}` must take `db_session_id` (runtime session semantics); got: {params}"
            );
            let without_new = params.replace("db_session_id", "");
            assert!(
                !without_new.contains("connection_id"),
                "`{cmd}` must not take (or also take) `connection_id`; got: {params}"
            );
        }
    }

    #[test]
    fn config_semantics_commands_keep_connection_id() {
        // workflow_execute feeds a persisted id into the executor's dual-mode
        // resolve; ai_diagnose_connection looks up the stored configuration.
        for cmd in ["workflow_execute", "ai_diagnose_connection"] {
            let params = command_params(cmd);
            assert!(
                params.contains("connection_id"),
                "`{cmd}` keeps persisted-configuration semantics and must take `connection_id`; got: {params}"
            );
            assert!(
                !params.contains("db_session_id"),
                "`{cmd}` must not take `db_session_id`; got: {params}"
            );
        }
    }

    #[test]
    fn strict_session_lookups_are_never_fed_a_connection_id_binding() {
        // Body-level guard mirroring the signature guards above: if any of
        // these strings reappear, a strict runtime-session lookup is being fed
        // a variable named after the *persisted* configuration id. The needles
        // are assembled at runtime so this test module never contains them.
        let conn = "connection_";
        let id = "id";
        let conn_id = format!("{conn}{id}");
        assert!(!SOURCE.contains(&format!(".get_session(&{conn_id})")));
        assert!(!SOURCE.contains(&format!(".get_session({conn_id})")));
        assert!(!SOURCE.contains(&format!("owner_connection_id(&{conn_id})")));
    }
}
