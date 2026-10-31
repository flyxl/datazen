//! AI generation and analysis IPC commands.
//!
//! WR-A2：命令处理函数直接持有接线逻辑（会话借用、schema 上下文管线、
//! 取消登记、流式回调构造），无 `_impl` 转发层；编排本体在 modules
//! （`datazen::ai::*` 用例与 `run_streaming_tool_loop`）。流式命令对
//! `R: Runtime` 泛型，测试经 `tauri::test::mock_app` 直调。

use super::chat::{db_tool_definitions, run_streaming_tool_loop_with_claim};
use super::util::{
    borrow_runtime_session, resolve_ai, resolve_language, resolve_safety_gate, runtime_session,
    window_stream_callback,
};
use crate::commands::error::{CmdExt, CommandError};
use crate::commands::AppState;
use datazen::ai::budget;
use datazen::ai::safety::redact_for_gate;
use datazen::ai::schema_pipeline::{compose_schema_system_suffix, SchemaContextPipeline};
use datazen::ai::*;
use tauri::{State, WebviewWindow};
use tokio::sync::mpsc;

pub use datazen::ai::single_turn::{ConnectionDiagnosis, QueryAnalysis};

// ─── NL2SQL ───

#[tauri::command]
pub async fn ai_generate_sql<R: tauri::Runtime>(
    state: State<'_, AppState>,
    window: WebviewWindow<R>,
    db_session_id: String,
    database: String,
    natural_language: String,
    request_id: String,
    current_table: Option<String>,
    recent_queries: Option<Vec<String>>,
    context_files: Option<Vec<String>>,
    context_tables: Option<Vec<String>>,
    target_claim: Option<datazen::ai::ownership::AiTargetClaim>,
) -> Result<String, CommandError> {
    let on_chunk = window_stream_callback(&window);
    let recent_queries = recent_queries.unwrap_or_default();
    let mut natural_language = natural_language;
    let gate = resolve_safety_gate(&state).await;
    tracing::info!(
        %request_id,
        %db_session_id,
        %database,
        input_len = natural_language.len(),
        current_table = ?current_table,
        recent_queries_count = recent_queries.len(),
        "ai_generate_sql: start"
    );

    let mut ctx_yaml_tables: Vec<String> = Vec::new();

    if let Some(ref ctx_files) = context_files {
        if !ctx_files.is_empty() {
            let ctx_dir = crate::commands::context::resolve_context_dir_from_state(&state).await?;
            let entries = crate::commands::context::read_context_paths(&ctx_dir, ctx_files).await?;
            if !entries.is_empty() {
                let (yaml_tables, remaining) =
                    datazen::ai::ctx_yaml::extract_ctx_yaml_tables(&entries);
                ctx_yaml_tables = yaml_tables;

                if !remaining.is_empty() {
                    let context_block = crate::commands::context::format_context_block(&remaining);
                    natural_language = format!("{context_block}\n\n{natural_language}");
                }
            }
        }
    }

    // Context files and natural language are caller-provided prompt content;
    // sanitize the complete assembled value at the command boundary.
    let natural_language = redact_for_gate(&natural_language, &gate);

    let (provider, ai_config) = resolve_ai(&state).await?;

    let lang = resolve_language(&state).await;

    let (_session, lease) =
        borrow_runtime_session(&state, &db_session_id, "ai_generate_sql").await?;
    let driver_ref = lease
        .connection()
        .map_err(|error| CommandError::Internal(error.to_string()))?
        .driver()
        .clone();

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

    let mut request = datazen::ai::sql_generation::build_sql_generation_request(
        &request_id,
        &ai_config.model,
        state.prompt_resolver.as_ref(),
        driver_ref.as_ref(),
        &lang,
        &seed,
        natural_language,
        &recent_queries,
        &gate,
    )
    .await;

    // Register cancel token so `ai_cancel` IPC can interrupt this stream.
    let cancel_owner = super::ai_owner_fingerprint(&state);
    let cancel_token = state
        .cancel_registry
        .register(&request_id, &cancel_owner)
        .await;
    request.cancel_token = Some(cancel_token.clone());

    tracing::debug!(
        %request_id,
        model = %request.model,
        messages_count = request.messages.len(),
        system_prompt_len = request.messages.first().map(|m| m.content.len()).unwrap_or(0),
        attach_db_tools = seed.attach_db_tools,
        "ai_generate_sql: sending to provider (stream)"
    );

    let claim = target_claim.unwrap_or(datazen::ai::ownership::AiTargetClaim::Independent);
    let result = if seed.attach_db_tools {
        request.tools = Some(db_tool_definitions());
        run_streaming_tool_loop_with_claim(
            provider,
            &state,
            on_chunk,
            &request_id,
            request,
            10,
            "ai_generate_sql",
            &gate,
            &claim,
        )
        .await
    } else {
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

        Ok(request_id.clone())
    };

    // Always unregister — normal completion or cancellation.
    state
        .cancel_registry
        .unregister(&request_id, &cancel_owner)
        .await;
    result
}

// ─── SQL Error Diagnosis ───

#[tauri::command]
pub async fn ai_diagnose_error(
    state: State<'_, AppState>,
    db_session_id: String,
    database: String,
    sql: String,
    error_message: String,
) -> Result<DiagnosisResult, CommandError> {
    let gate = resolve_safety_gate(&state).await;
    let safe_sql = redact_for_gate(&sql, &gate);
    let safe_error_message = redact_for_gate(&error_message, &gate);
    tracing::info!(
        %db_session_id,
        %database,
        sql_len = sql.len(),
        safe_sql_len = safe_sql.len(),
        sql_redacted = safe_sql != sql,
        error_len = error_message.len(),
        safe_error_len = safe_error_message.len(),
        error_redacted = safe_error_message != error_message,
        "ai_diagnose_error: start"
    );

    let (provider, ai_config) = resolve_ai(&state).await?;

    let context = state
        .schema_context_builder
        .build_sql_context(&db_session_id, &database, None, &[], budget::DIAGNOSE)
        .await
        .cmd_err("ai_diagnose_error")?;

    let lang = resolve_language(&state).await;

    let (_session, lease) =
        borrow_runtime_session(&state, &db_session_id, "ai_diagnose_error").await?;
    let driver_ref = lease
        .connection()
        .map_err(|error| CommandError::Internal(error.to_string()))?
        .driver()
        .clone();

    let safe_schema_ddl = redact_for_gate(&context.schema_ddl, &gate);
    datazen::ai::single_turn::diagnose_sql_error(
        provider.as_ref(),
        &ai_config.model,
        state.prompt_resolver.as_ref(),
        driver_ref.as_ref(),
        &lang,
        &safe_schema_ddl,
        &safe_sql,
        &safe_error_message,
    )
    .await
    .cmd_err("ai_diagnose_error")
}

// ─── EXPLAIN Analysis ───

#[tauri::command]
pub async fn ai_analyze_explain(
    state: State<'_, AppState>,
    db_session_id: String,
    explain_output: String,
    original_sql: String,
) -> Result<ExplainAnalysis, CommandError> {
    let gate = resolve_safety_gate(&state).await;
    let safe_original_sql = redact_for_gate(&original_sql, &gate);
    let safe_explain_output = redact_for_gate(&explain_output, &gate);
    tracing::info!(
        %db_session_id,
        sql_len = original_sql.len(),
        safe_sql_len = safe_original_sql.len(),
        sql_redacted = safe_original_sql != original_sql,
        explain_len = explain_output.len(),
        safe_explain_len = safe_explain_output.len(),
        explain_redacted = safe_explain_output != explain_output,
        "ai_analyze_explain: start"
    );
    let (provider, ai_config) = resolve_ai(&state).await?;

    let (_session, lease) =
        borrow_runtime_session(&state, &db_session_id, "ai_analyze_explain").await?;
    let driver = lease
        .connection()
        .map_err(|error| CommandError::Internal(error.to_string()))?
        .driver()
        .clone();

    let lang = resolve_language(&state).await;
    datazen::ai::single_turn::analyze_explain(
        provider.as_ref(),
        &ai_config.model,
        state.prompt_resolver.as_ref(),
        driver.as_ref(),
        &lang,
        &safe_explain_output,
        &safe_original_sql,
    )
    .await
    .cmd_err("ai_analyze_explain")
}

// ─── Smart Filter ───

#[tauri::command]
pub async fn ai_parse_filter(
    state: State<'_, AppState>,
    db_session_id: String,
    database: String,
    table: String,
    natural_language: String,
) -> Result<Vec<datazen::services::query_executor::FilterCondition>, CommandError> {
    tracing::info!(
        %db_session_id,
        %database,
        %table,
        input_len = natural_language.len(),
        "ai_parse_filter: start"
    );
    let gate = resolve_safety_gate(&state).await;
    let natural_language = redact_for_gate(&natural_language, &gate);
    let (provider, ai_config) = resolve_ai(&state).await?;

    let (session, lease) =
        borrow_runtime_session(&state, &db_session_id, "ai_parse_filter").await?;
    let connection = lease
        .connection()
        .map_err(|error| CommandError::Internal(error.to_string()))?;
    let driver = connection.driver().clone();
    let handle = connection.handle().clone();

    let schema = session.observed_context.namespace.schema.clone();

    let cached = state
        .schema_cache
        .get_columns(
            &db_session_id,
            &database,
            schema.as_deref(),
            &table,
            &datazen_driver_api::LegacyCatalogReader::new(driver.as_ref()),
            &handle,
        )
        .await
        .cmd_err("ai_parse_filter")?;

    let lang = resolve_language(&state).await;
    datazen::ai::filter::parse_natural_language_filter(
        provider.as_ref(),
        &ai_config.model,
        state.prompt_resolver.as_ref(),
        driver.as_ref(),
        natural_language,
        &cached.columns,
        &lang,
    )
    .await
    .cmd_err("ai_parse_filter")
}

// ─── Phase 8: Schema documentation ───

#[tauri::command]
pub async fn ai_generate_schema_doc(
    state: State<'_, AppState>,
    db_session_id: String,
    database: String,
) -> Result<String, CommandError> {
    tracing::info!(%db_session_id, %database, "ai_generate_schema_doc: start");
    let (provider, ai_config) = resolve_ai(&state).await?;
    let lang = resolve_language(&state).await;

    let (_session, lease) =
        borrow_runtime_session(&state, &db_session_id, "ai_generate_schema_doc").await?;
    let driver = lease
        .connection()
        .map_err(|error| CommandError::Internal(error.to_string()))?
        .driver()
        .clone();
    datazen::ai::schema_doc::generate_schema_doc(
        provider.as_ref(),
        &ai_config.model,
        state.schema_context_builder.as_ref(),
        state.prompt_resolver.as_ref(),
        driver.as_ref(),
        &db_session_id,
        &database,
        &lang,
    )
    .await
    .cmd_err("ai_generate_schema_doc")
}

// ─── Phase 8: Connection diagnostics ───

#[tauri::command]
pub async fn ai_diagnose_connection(
    state: State<'_, AppState>,
    connection_id: String,
    error_message: String,
) -> Result<ConnectionDiagnosis, CommandError> {
    let gate = resolve_safety_gate(&state).await;
    let safe_error_message = redact_for_gate(&error_message, &gate);
    tracing::info!(%connection_id, error_len = error_message.len(), "ai_diagnose_connection: start");
    tracing::debug!(
        %connection_id,
        safe_error_len = safe_error_message.len(),
        error_redacted = safe_error_message != error_message,
        "ai_diagnose_connection: safe error prepared"
    );
    let (provider, ai_config) = resolve_ai(&state).await?;

    let conn_info = state
        .store
        .get_connection(&connection_id)
        .await
        .ok_or_else(|| CommandError::NotFound("Connection config not found".into()))?;

    let conn_summary = redact_for_gate(
        &datazen::ai::single_turn::format_connection_summary(
            &format!("{:?}", conn_info.database_type),
            conn_info.host.as_deref(),
            conn_info.port,
            conn_info.database.as_deref(),
            conn_info.username.as_deref(),
            &format!("{:?}", conn_info.ssl_mode),
            conn_info.ssh_tunnel.is_some(),
            conn_info.connection_timeout,
        ),
        &gate,
    );

    let lang = resolve_language(&state).await;
    datazen::ai::single_turn::diagnose_connection(
        provider.as_ref(),
        &ai_config.model,
        state.prompt_resolver.as_ref(),
        &lang,
        &conn_summary,
        &safe_error_message,
    )
    .await
    .cmd_err("ai_diagnose_connection")
}

// ─── Phase 8: Query history analysis ───

#[tauri::command]
pub async fn ai_analyze_queries(
    state: State<'_, AppState>,
    db_session_id: Option<String>,
) -> Result<QueryAnalysis, CommandError> {
    tracing::info!(db_session_id = ?db_session_id, "ai_analyze_queries: start");
    let gate = resolve_safety_gate(&state).await;
    let (provider, ai_config) = resolve_ai(&state).await?;
    let lang = resolve_language(&state).await;

    // The IPC parameter is the runtime dbSessionId; resolve the persisted
    // connection id that owns it so history is filtered by connection.
    let owner_connection_id = if let Some(ref cid) = db_session_id {
        runtime_session(&state, cid, "ai_analyze_queries")
            .await
            .ok()
            .map(|session| session.connection_id.as_str().to_owned())
    } else {
        None
    };
    // D6 第一切片：读取经模块服务（同一 `HistoryDb`）；未注入是装配错误，
    // 向上抛 CommandError 而不是仍装成空历史。
    let history = crate::platform::ipc::require_history_service(&state)?
        .recent_query_history(200, owner_connection_id.as_deref(), None, None)
        .await;
    let filtered: Vec<_> = history.iter().collect();

    if filtered.is_empty() {
        return Err(CommandError::Validation(
            "No query history available".into(),
        ));
    }

    let queries = filtered
        .iter()
        .take(100)
        .map(|h| redact_for_gate(&h.sql, &gate))
        .collect::<Vec<_>>();

    datazen::ai::single_turn::analyze_query_history(
        provider.as_ref(),
        &ai_config.model,
        state.prompt_resolver.as_ref(),
        &lang,
        &queries,
    )
    .await
    .cmd_err("ai_analyze_queries")
}
