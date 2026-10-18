//! AI generation and analysis IPC commands.

use super::chat::db_tool_definitions;
use super::chat::run_streaming_tool_loop_with_claim;
use super::util::{resolve_ai, resolve_safety_gate, window_stream_callback, StreamCallback};
use crate::ai::budget;
use crate::ai::safety::redact_for_gate;
use crate::ai::*;
use crate::commands::error::{CmdExt, CommandError};
use crate::commands::AppState;
use tauri::{State, WebviewWindow};
use tokio::sync::mpsc;

pub use datazen_feature_core::ai::single_turn::{
    ConnectionDiagnosis, ConnectionSolution, QueryAnalysis, QueryCategory,
};

// ─── NL2SQL ───

#[allow(dead_code)]
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
    ai_generate_sql_impl_with_claim(
        state,
        on_chunk,
        None,
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

pub(crate) async fn ai_generate_sql_impl_with_claim(
    state: &AppState,
    on_chunk: StreamCallback,
    target_claim: Option<datazen_feature_core::ai::ownership::AiTargetClaim>,
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
    let app_settings = state.store.get_settings().await;
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
                    crate::ai::ctx_yaml::extract_ctx_yaml_tables(&entries);
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

    let lang = app_settings.language;

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

    let mut request = datazen_feature_core::ai::sql_generation::build_sql_generation_request(
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
    let cancel_owner = super::ai_owner_fingerprint(state);
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

    let claim =
        target_claim.unwrap_or(datazen_feature_core::ai::ownership::AiTargetClaim::Independent);
    let result = if seed.attach_db_tools {
        request.tools = Some(db_tool_definitions());
        run_streaming_tool_loop_with_claim(
            provider,
            state,
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
    target_claim: Option<datazen_feature_core::ai::ownership::AiTargetClaim>,
) -> Result<String, CommandError> {
    ai_generate_sql_impl_with_claim(
        &state,
        window_stream_callback(&window),
        target_claim,
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
    let gate = resolve_safety_gate(state).await;
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

    let lang = state.store.get_settings().await.language;

    let (driver, _) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("ai_diagnose_error")?;

    let safe_schema_ddl = redact_for_gate(&context.schema_ddl, &gate);
    datazen_feature_core::ai::single_turn::diagnose_sql_error(
        provider.as_ref(),
        &ai_config.model,
        state.prompt_resolver.as_ref(),
        driver.as_ref(),
        &lang,
        &safe_schema_ddl,
        &safe_sql,
        &safe_error_message,
    )
    .await
    .map_err(Into::into)
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
    let gate = resolve_safety_gate(state).await;
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

    let (driver, _) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("ai_analyze_explain")?;

    let lang = state.store.get_settings().await.language;
    datazen_feature_core::ai::single_turn::analyze_explain(
        provider.as_ref(),
        &ai_config.model,
        state.prompt_resolver.as_ref(),
        driver.as_ref(),
        &lang,
        &safe_explain_output,
        &safe_original_sql,
    )
    .await
    .map_err(Into::into)
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
    let gate = resolve_safety_gate(state).await;
    let natural_language = redact_for_gate(&natural_language, &gate);
    let (provider, ai_config) = resolve_ai(&state).await?;

    let (driver, handle) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("ai_parse_filter")?;

    let config = state
        .connection_manager
        .get_session_config(&db_session_id)
        .await
        .cmd_err("ai_parse_filter")?;
    let schema =
        crate::services::metadata_schema(driver.as_ref(), None, None, config.schema.as_deref());

    let cached = state
        .schema_cache
        .get_columns(
            &db_session_id,
            &database,
            schema.as_deref(),
            &table,
            &driver,
            &handle,
        )
        .await
        .cmd_err("ai_parse_filter")?;

    let lang = state.store.get_settings().await.language;
    datazen_feature_core::ai::filter::parse_natural_language_filter(
        provider.as_ref(),
        &ai_config.model,
        state.prompt_resolver.as_ref(),
        driver.as_ref(),
        natural_language,
        &cached.columns,
        &lang,
    )
    .await
    .map_err(Into::into)
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
    let lang = state.store.get_settings().await.language;
    let (driver, _) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("ai_generate_schema_doc")?;
    datazen_feature_core::ai::schema_doc::generate_schema_doc(
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
    .map_err(Into::into)
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
    let gate = resolve_safety_gate(state).await;
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

    let ssl_str = format!("{:?}", conn_info.ssl_mode);
    let ssh_str = if conn_info.ssh_tunnel.is_some() {
        "enabled"
    } else {
        "disabled"
    };
    let conn_summary = redact_for_gate(
        &format!(
            "Connection type: {:?}\nHost: {}\nPort: {}\nDatabase: {}\nUsername: {}\nSSL: {}\nSSH Tunnel: {}\nTimeout: {}s",
            conn_info.database_type,
            conn_info.host.as_deref().unwrap_or("N/A"),
            conn_info.port.map(|p| p.to_string()).unwrap_or_else(|| "N/A".into()),
            conn_info.database.as_deref().unwrap_or("N/A"),
            conn_info.username.as_deref().unwrap_or("N/A"),
            ssl_str,
            ssh_str,
            conn_info.connection_timeout,
        ),
        &gate,
    );

    let lang = state.store.get_settings().await.language;
    datazen_feature_core::ai::single_turn::diagnose_connection(
        provider.as_ref(),
        &ai_config.model,
        state.prompt_resolver.as_ref(),
        &lang,
        &conn_summary,
        &safe_error_message,
    )
    .await
    .map_err(Into::into)
}

// ─── Phase 8: Query history analysis ───

pub(crate) async fn ai_analyze_queries_impl(
    state: &AppState,
    db_session_id: Option<String>,
) -> Result<QueryAnalysis, CommandError> {
    tracing::info!(db_session_id = ?db_session_id, "ai_analyze_queries: start");
    let app_settings = state.store.get_settings().await;
    let gate = resolve_safety_gate(state).await;
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

    let queries = filtered
        .iter()
        .take(100)
        .map(|h| redact_for_gate(&h.sql, &gate))
        .collect::<Vec<_>>();

    datazen_feature_core::ai::single_turn::analyze_query_history(
        provider.as_ref(),
        &ai_config.model,
        state.prompt_resolver.as_ref(),
        &app_settings.language,
        &queries,
    )
    .await
    .map_err(Into::into)
}

#[tauri::command]
pub async fn ai_analyze_queries(
    state: State<'_, AppState>,
    db_session_id: Option<String>,
) -> Result<QueryAnalysis, CommandError> {
    ai_analyze_queries_impl(&state, db_session_id).await
}
