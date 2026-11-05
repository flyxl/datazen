//! AI generation and analysis IPC commands.
//!
//! WR-A2：单轮命令（diagnose/explain/filter/schema_doc/connection/queries）
//! 的处理函数直接持有接线逻辑（会话借用、SQL 上下文读取、脱敏），无
//! `_impl` 转发层；B3 后 `ai_generate_sql` 的编排本体（pinned 表合成、
//! schema 管线、取消登记、attach_db_tools 分流）下沉 modules 用例
//! `datazen::ai::application::chat_session`，命令只做形参提取与端口取值。
//! 流式命令对 `R: Runtime` 泛型，测试经 `tauri::test::mock_app` 直调。

use super::util::{
    borrow_runtime_session, read_context_entries, resolve_ai, resolve_language,
    resolve_memory_settings, resolve_safety_gate, runtime_session, window_stream_callback,
};
use crate::commands::error::{CmdExt, CommandError};
use crate::commands::AppState;
use datazen::ai::application::chat_session::{
    run_sql_generation_session, ChatSessionEnv, SqlGenerationInput,
};
use datazen::ai::application::schema_pipeline::SchemaContextPipeline;
use datazen::ai::memory::{MemoryMode, MemoryScopeKind};
use datazen::ai::ownership::AiTargetClaim;
use datazen::ai::safety::redact_for_gate;
use datazen::ai::*;
use tauri::{State, WebviewWindow};

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
    target_claim: Option<AiTargetClaim>,
    connection_id: Option<String>,
    memory_mode: Option<MemoryMode>,
    memory_scope: Option<MemoryScopeKind>,
) -> Result<String, CommandError> {
    let on_chunk = window_stream_callback(&window);
    let gate = resolve_safety_gate(&state).await;
    tracing::info!(
        %request_id,
        %db_session_id,
        %database,
        input_len = natural_language.len(),
        current_table = ?current_table,
        recent_queries_count = recent_queries.as_ref().map_or(0, Vec::len),
        "ai_generate_sql: start"
    );

    // 端口取值（thin binding）。顺序即错误优先级，与 IPC 契约一致。
    let context_entries = read_context_entries(&state, context_files.as_deref()).await?;
    let (provider, ai_config) = resolve_ai(&state).await?;
    let lang = resolve_language(&state).await;
    let (_session, lease) =
        borrow_runtime_session(&state, &db_session_id, "ai_generate_sql").await?;
    let driver = lease
        .connection()
        .map_err(|error| CommandError::Internal(error.to_string()))?
        .driver()
        .clone();

    let memory_settings = resolve_memory_settings(&state).await;
    let memory_mode = if memory_settings.enabled {
        memory_mode.unwrap_or_default()
    } else {
        MemoryMode::Off
    };
    let memory = match super::resolve_chat_memory(
        &state,
        memory_mode,
        memory_scope,
        connection_id.as_deref(),
        Some(&database),
        &natural_language,
        super::provider_is_cloud(provider.as_ref(), &ai_config),
        gate.data_egress_level,
        memory_settings.cloud_policy,
    )
    .await
    {
        Ok(memory) => memory,
        Err(error) => {
            tracing::warn!(error = %error, "ai_generate_sql: memory retrieval failed; continuing without memory");
            super::ChatMemory::default()
        }
    };
    if memory_mode == MemoryMode::Ask && !memory.candidates.is_empty() {
        super::emit_memory_candidates(&window, &request_id, &memory.candidates).await;
    }

    // 编排本体在 modules（B3 下沉）；claim 归由请求级宿主提供。
    let host = super::AiRequestHost {
        state: &state,
        claim: target_claim.unwrap_or(AiTargetClaim::Independent),
    };
    let env = ChatSessionEnv {
        provider,
        model: ai_config.model,
        gate,
        injection: datazen::ai::chat_injection::ChatInjection::default(),
        memory_injection: memory.injection,
        memory_used: memory.used,
        lang,
        connections_context: String::new(),
        mcp_tools: Vec::new(),
        context_entries,
        schema_driver: None,
        prompt_resolver: state.prompt_resolver.clone(),
        schema_pipeline: SchemaContextPipeline::new(state.schema_context_builder.clone()),
        cancel_registry: state.cancel_registry.clone(),
        cancel_owner: super::ai_owner_fingerprint(&state),
    };
    run_sql_generation_session(
        env,
        driver,
        &host,
        on_chunk,
        SqlGenerationInput {
            request_id,
            db_session_id,
            database,
            natural_language,
            current_table,
            recent_queries: recent_queries.unwrap_or_default(),
            context_tables: context_tables.unwrap_or_default(),
        },
    )
    .await
    .map_err(Into::into)
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
