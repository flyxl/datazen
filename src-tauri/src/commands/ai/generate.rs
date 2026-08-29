//! AI generation and analysis IPC commands.

use super::chat::{db_tool_definitions, run_streaming_tool_loop};
use super::util::{
    inject_language_hint, parse_ai_json, resolve_ai, strip_markdown_fences, truncate_str,
    StreamCallback, window_stream_callback,
};
use crate::ai::budget;
use crate::ai::prompt_resolver;
use crate::ai::*;
use crate::commands::error::{CmdExt, CommandError};
use crate::commands::AppState;
use datazen_driver_api::PromptScenario;
use serde::Serialize;
use std::collections::HashMap;
use tauri::{State, WebviewWindow};
use tokio::sync::mpsc;
use uuid::Uuid;

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
            let ctx_dir = crate::commands::context::resolve_context_dir_from_state(&state).await?;
            let entries = crate::commands::context::read_context_paths(&ctx_dir, ctx_files).await?;
            if !entries.is_empty() {
                let (yaml_tables, remaining) =
                    crate::ai::ctx_yaml::extract_ctx_yaml_tables(&entries);
                ctx_yaml_tables = yaml_tables;

                if !remaining.is_empty() {
                    let context_block = crate::commands::context::format_context_block(&remaining);
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
