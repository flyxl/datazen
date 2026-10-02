use std::sync::Arc;

use super::driver_command::{
    execute_driver_command_stream_impl, ExecuteDriverCommandStreamOpts,
    ExecuteDriverCommandStreamRequest,
};
use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::db::{ExplainResult, MultiQueryResult};
use crate::store::history_db::DEFAULT_HISTORY_PAGE_SIZE;
use crate::store::{HistoryOrder, QueryHistoryEntry, QueryHistoryFilter, QueryHistoryPage};
use datazen_driver_api::{QueryExecutionId, QueryStreamCallback, QueryStreamEvent};
use tauri::ipc::Channel;
use tauri::State;

pub(crate) async fn execute_query_impl(
    state: &AppState,
    db_session_id: String,
    sql: String,
    database: Option<String>,
) -> Result<MultiQueryResult, CommandError> {
    tracing::info!(%db_session_id, sql_len = sql.len(), "execute_query");
    tracing::debug!(
        %db_session_id,
        sql_preview = %crate::log_redact::sql_preview_for_log(&sql),
        "execute_query sql"
    );
    let result = super::driver_command::execute_driver_command_impl(
        state,
        super::driver_command::ExecuteDriverCommandRequest {
            db_session_id: Some(db_session_id),
            driver_type: None,
            command: "query".into(),
            // F7: the target rides the command envelope so rewrite-capable
            // drivers qualify unqualified relations inline (`db`.`t`). There is
            // deliberately no session switch: a `USE` on a pooled connection can
            // go stale (a recycled connection still sits on the default
            // database), which is how a query bound to a non-first database
            // ended up running against the first one after navigating away and
            // back. `schema` stays `None` so a hand-written query keeps resolving
            // through the connection's own default (PostgreSQL `search_path`).
            database,
            schema: None,
            input: serde_json::json!({ "sql": sql }),
        },
    )
    .await?;
    serde_json::from_value(result.data).map_err(CommandError::Json)
}

#[derive(Clone, Copy)]
pub(crate) struct ExecuteQueryStreamOpts {
    pub apply_result_limit: bool,
    pub record_history: bool,
}

impl Default for ExecuteQueryStreamOpts {
    fn default() -> Self {
        Self {
            apply_result_limit: true,
            record_history: true,
        }
    }
}

pub(crate) async fn execute_query_stream_impl(
    state: &AppState,
    db_session_id: String,
    sql: String,
    database: Option<String>,
    on_event: QueryStreamCallback,
    opts: ExecuteQueryStreamOpts,
) -> Result<(), CommandError> {
    execute_driver_command_stream_impl(
        state,
        ExecuteDriverCommandStreamRequest {
            db_session_id: Some(db_session_id),
            command: "query_stream".into(),
            // F7: same envelope targeting as `execute_query` — rewrite-capable
            // drivers qualify unqualified relations inline (`db`.`t`) and no
            // session switch is performed on any path.
            database,
            schema: None,
            input: serde_json::json!({ "sql": sql }),
            apply_result_limit: Some(opts.apply_result_limit),
            record_history: Some(opts.record_history),
        },
        on_event,
        ExecuteDriverCommandStreamOpts {
            apply_result_limit: opts.apply_result_limit,
            record_history: opts.record_history,
        },
    )
    .await
}

pub(crate) async fn get_explain_impl(
    state: &AppState,
    db_session_id: String,
    sql: String,
    database: Option<String>,
) -> Result<ExplainResult, CommandError> {
    tracing::debug!(%db_session_id, "get_explain");
    let (driver, handle) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("get_explain")?;

    // The plan must be produced for the caller's target, not for whatever
    // database the pooled connection defaults to. Rewrite-capable drivers
    // qualify the SQL inline; there is no session switch to fall back on.
    let config = state
        .connection_manager
        .get_session_config(&db_session_id)
        .await
        .cmd_err("get_explain")?;
    let database = database
        .as_deref()
        .map(str::trim)
        .filter(|db| !db.is_empty())
        .or(config.database.as_deref());
    let schema =
        crate::services::metadata_schema(driver.as_ref(), None, None, config.schema.as_deref());
    let sql = match driver.qualify_sql_target(&sql, database, schema.as_deref()) {
        Some(qualified) => qualified,
        None => sql,
    };

    driver.explain(&handle, &sql).await.cmd_err("get_explain")
}

pub(crate) async fn cancel_query_impl(
    state: &AppState,
    db_session_id: String,
    execution_id: String,
) -> Result<(), CommandError> {
    let execution_id = QueryExecutionId::new(execution_id);
    tracing::info!(
        %db_session_id,
        execution_id = %execution_id.as_str(),
        "cancel_query"
    );
    state
        .query_executions
        .validate_owner(&execution_id, &db_session_id)
        .await
        .map_err(CommandError::Validation)?;
    let config = state
        .connection_manager
        .get_session_config(&db_session_id)
        .await
        .cmd_err("cancel_query")?;
    match state
        .driver_registry
        .get_capabilities(&config.database_type)
        .await
    {
        Some(capabilities) if capabilities.supports_query_execution_cancel => {}
        Some(_) => {
            return Err(CommandError::Validation(
                "UNSUPPORTED_OPERATION:cancel_query:query cancellation is not supported by this driver"
                    .into(),
            ));
        }
        None => {
            return Err(CommandError::Validation(
                "UNSUPPORTED_OPERATION:cancel_query:query cancellation capability is unknown"
                    .into(),
            ));
        }
    }
    let (driver, handle) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("cancel_query")?;

    driver
        .cancel_query_with_execution(&handle, &execution_id)
        .await
        .cmd_err("cancel_query")
}

pub(crate) async fn get_query_history_impl(
    state: &AppState,
    limit: usize,
    connection_id: Option<String>,
    database: Option<String>,
    schema: Option<String>,
) -> Result<Vec<QueryHistoryEntry>, CommandError> {
    Ok(state
        .store
        .get_query_history(
            limit,
            connection_id.as_deref(),
            database.as_deref(),
            schema.as_deref(),
        )
        .await)
}

pub(crate) async fn clear_query_history_impl(state: &AppState) -> Result<(), CommandError> {
    tracing::info!("clear_query_history");
    state
        .store
        .clear_query_history()
        .await
        .cmd_err("clear_query_history")
}

/// The raw shape of a paged history read, as it arrives over IPC.
///
/// Tauri's command macro requires one parameter per IPC argument, so the
/// `#[tauri::command]` below unavoidably has a long positional list. Everything
/// past that boundary works on this struct instead, so the filter's fields
/// cannot be silently reordered at a call site.
///
/// Mirrors the frontend's `HistoryQueryState` in
/// `src/components/history/historyQuery.ts`. The two are not identical by
/// design: the frontend uses `'all'` sentinels because it binds to selects,
/// while an absent value is a real `Option` over the wire.
#[derive(Debug, Clone)]
pub struct HistoryPageRequest {
    pub limit: usize,
    pub connection_id: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub search: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub order: Option<String>,
}

impl Default for HistoryPageRequest {
    /// Hand-written for the same reason `QueryHistoryFilter`'s is: a derived
    /// `Default` would give `limit: 0`, and `LIMIT 0` returns an empty page
    /// that reads as "no history" rather than "no rows requested".
    fn default() -> Self {
        Self {
            limit: DEFAULT_HISTORY_PAGE_SIZE,
            connection_id: None,
            database: None,
            schema: None,
            search: None,
            since: None,
            until: None,
            order: None,
        }
    }
}

impl HistoryPageRequest {
    /// Validate the order and project onto the store's filter.
    ///
    /// An unknown order is rejected rather than silently defaulting: a UI that
    /// asks for "slowest first" and gets "most recent" is a worse outcome than
    /// an error, because nothing on screen reveals the substitution.
    ///
    /// Borrows rather than consumes, because `QueryHistoryFilter` borrows its
    /// strings. The caller owns the request for as long as it needs the filter.
    fn to_filter(&self) -> Result<QueryHistoryFilter<'_>, CommandError> {
        let order = match self.order.as_deref() {
            None | Some("") => HistoryOrder::Recent,
            Some(other) => HistoryOrder::parse(other)
                .ok_or_else(|| CommandError::Validation(format!("unknown order: {other}")))?,
        };
        Ok(QueryHistoryFilter {
            limit: self.limit,
            connection_id: self.connection_id.as_deref(),
            database: self.database.as_deref(),
            schema: self.schema.as_deref(),
            search: self.search.as_deref(),
            since: self.since.as_deref(),
            until: self.until.as_deref(),
            order,
        })
    }
}

/// Paged history read. `total` is the full match count, so a UI can state
/// "showing N of M" instead of implying the page is everything.
pub(crate) async fn get_query_history_page_impl(
    state: &AppState,
    request: HistoryPageRequest,
) -> Result<QueryHistoryPage, CommandError> {
    let filter = request.to_filter()?;
    state
        .store
        .get_query_history_page(&filter)
        .await
        .cmd_err("get_query_history_page")
}

/// Remove exactly one history row. Returns how many rows went away so a caller
/// can reject a no-op instead of showing a success it did not earn.
pub(crate) async fn delete_query_history_impl(
    state: &AppState,
    id: String,
) -> Result<u64, CommandError> {
    state
        .store
        .delete_query_history(&id)
        .await
        .cmd_err("delete_query_history")
}

#[tauri::command]
pub async fn execute_query(
    state: State<'_, AppState>,
    db_session_id: String,
    sql: String,
    database: Option<String>,
) -> Result<MultiQueryResult, CommandError> {
    execute_query_impl(&state, db_session_id, sql, database).await
}

#[tauri::command]
pub async fn execute_query_stream(
    state: State<'_, AppState>,
    db_session_id: String,
    sql: String,
    database: Option<String>,
    on_event: Channel<QueryStreamEvent>,
    apply_result_limit: Option<bool>,
    record_history: Option<bool>,
) -> Result<(), CommandError> {
    let callback: QueryStreamCallback = Arc::new(move |event| {
        // Unavoidable and not a masked defect: `Channel::send` fails only when
        // the receiving end is gone (the webview for this stream was closed or
        // reloaded), and there is no one left to report to — the event has
        // nowhere to go even if it were reported. `QueryStreamCallback` returns
        // `()`, so propagating would mean changing the callback contract for
        // every producer. Deliberately *not* logged: this runs once per stream
        // event, so a log line here would flood the file with the same message
        // for the remainder of a large result set.
        let _ = on_event.send(event);
    });
    execute_query_stream_impl(
        &state,
        db_session_id,
        sql,
        database,
        callback,
        ExecuteQueryStreamOpts {
            apply_result_limit: apply_result_limit.unwrap_or(true),
            record_history: record_history.unwrap_or(true),
        },
    )
    .await
}

#[tauri::command]
pub async fn get_explain(
    state: State<'_, AppState>,
    db_session_id: String,
    sql: String,
    database: Option<String>,
) -> Result<ExplainResult, CommandError> {
    get_explain_impl(&state, db_session_id, sql, database).await
}

#[tauri::command]
pub async fn cancel_query(
    state: State<'_, AppState>,
    db_session_id: String,
    execution_id: String,
) -> Result<(), CommandError> {
    cancel_query_impl(&state, db_session_id, execution_id).await
}

#[tauri::command]
pub async fn get_query_history(
    state: State<'_, AppState>,
    limit: usize,
    connection_id: Option<String>,
    database: Option<String>,
    schema: Option<String>,
) -> Result<Vec<QueryHistoryEntry>, CommandError> {
    get_query_history_impl(&state, limit, connection_id, database, schema).await
}

#[tauri::command]
pub async fn clear_query_history(state: State<'_, AppState>) -> Result<(), CommandError> {
    clear_query_history_impl(&state).await
}

/// Tauri requires one parameter per IPC argument, so this signature cannot
/// shrink. The struct above is how the rest of the code sees it.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn get_query_history_page(
    state: State<'_, AppState>,
    limit: usize,
    connection_id: Option<String>,
    database: Option<String>,
    schema: Option<String>,
    search: Option<String>,
    since: Option<String>,
    until: Option<String>,
    order: Option<String>,
) -> Result<QueryHistoryPage, CommandError> {
    get_query_history_page_impl(
        &state,
        HistoryPageRequest {
            limit,
            connection_id,
            database,
            schema,
            search,
            since,
            until,
            order,
        },
    )
    .await
}

#[tauri::command]
pub async fn delete_query_history(
    state: State<'_, AppState>,
    id: String,
) -> Result<u64, CommandError> {
    delete_query_history_impl(&state, id).await
}

/// Write `content` to a user-chosen path. Resolves `false` when the save
/// dialog is dismissed, so the caller can stay silent instead of claiming a
/// file it never wrote.
#[tauri::command]
pub async fn save_sql_file(
    app: tauri::AppHandle,
    default_file_name: String,
    content: String,
) -> Result<bool, CommandError> {
    // The dialog filters to `.sql`; the extension is appended when the user
    // types a bare stem, so a Windows user who types "queries" still gets a
    // file the editor and any `.sql` tooling will open.
    let path = super::dialog::save_file(
        &app,
        ("SQL".into(), vec!["sql".to_string()]),
        default_file_name,
    )
    .await?;

    let Some(path) = path else {
        return Ok(false);
    };
    let path = if path.extension().is_some() {
        path
    } else {
        path.with_extension("sql")
    };

    std::fs::write(&path, content).map_err(|e| {
        CommandError::Io(std::io::Error::new(
            e.kind(),
            format!("write {}: {e}", path.display()),
        ))
    })?;
    Ok(true)
}

pub(crate) async fn begin_session_transaction_impl(
    state: &AppState,
    db_session_id: String,
) -> Result<(), CommandError> {
    {
        let txs = state.session_transactions.lock().await;
        if txs.contains_key(&db_session_id) {
            return Ok(());
        }
    }
    let (driver, handle) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("begin_session_transaction")?;
    let tx = match driver.begin_transaction(&handle).await {
        Ok(tx) => tx,
        Err(e) => {
            if state
                .session_transactions
                .lock()
                .await
                .contains_key(&db_session_id)
            {
                return Ok(());
            }
            return Err(e).cmd_err("begin_session_transaction");
        }
    };
    let mut txs = state.session_transactions.lock().await;
    if txs.contains_key(&db_session_id) {
        drop(txs);
        // Lost the race: another `begin` for this `dbSessionId` registered its
        // transaction while ours was being opened, so this one must be undone
        // or the connection carries a second, untracked open transaction whose
        // writes no commit/rollback will ever reach. If the driver cannot
        // confirm the rollback, that dangling transaction is real and this
        // function returning `Ok(())` would be a lie — the caller would go on
        // to commit the *other* handle and the orphan's locks would stay held.
        // Same ruling as `services/transaction.rs::rollback` and
        // `ConnectionManager::disconnect`. The winner's transaction is
        // untouched: it is already in the map, and this path only ever removes
        // the handle this call created.
        driver.rollback(tx).await.map_err(|e| {
            let err: CommandError = e.into();
            tracing::error!(
                cmd = "begin_session_transaction",
                db_session_id = %db_session_id,
                error = %err,
                "Could not roll back the transaction opened by a lost begin race; \
                 the connection is left with an untracked open transaction",
            );
            err
        })?;
        return Ok(());
    }
    txs.insert(db_session_id, tx);
    Ok(())
}

pub(crate) async fn commit_session_transaction_impl(
    state: &AppState,
    db_session_id: String,
) -> Result<(), CommandError> {
    let (driver, _) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("commit_session_transaction")?;
    let tx = state
        .session_transactions
        .lock()
        .await
        .remove(&db_session_id)
        .ok_or_else(|| CommandError::Validation("No open transaction".into()))?;
    driver
        .commit(tx)
        .await
        .cmd_err("commit_session_transaction")
}

pub(crate) async fn rollback_session_transaction_impl(
    state: &AppState,
    db_session_id: String,
) -> Result<(), CommandError> {
    let (driver, _) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("rollback_session_transaction")?;
    let tx = state
        .session_transactions
        .lock()
        .await
        .remove(&db_session_id)
        .ok_or_else(|| CommandError::Validation("No open transaction".into()))?;
    driver
        .rollback(tx)
        .await
        .cmd_err("rollback_session_transaction")
}

pub(crate) async fn session_transaction_status_impl(
    state: &AppState,
    db_session_id: String,
) -> Result<bool, CommandError> {
    Ok(state
        .session_transactions
        .lock()
        .await
        .contains_key(&db_session_id))
}

#[tauri::command]
pub async fn begin_session_transaction(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<(), CommandError> {
    begin_session_transaction_impl(&state, db_session_id).await
}

#[tauri::command]
pub async fn commit_session_transaction(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<(), CommandError> {
    commit_session_transaction_impl(&state, db_session_id).await
}

#[tauri::command]
pub async fn rollback_session_transaction(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<(), CommandError> {
    rollback_session_transaction_impl(&state, db_session_id).await
}

#[tauri::command]
pub async fn session_transaction_status(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<bool, CommandError> {
    session_transaction_status_impl(&state, db_session_id).await
}

#[cfg(test)]
mod log_hygiene_tests {
    #[test]
    fn execute_query_source_does_not_info_log_sql_preview() {
        let src = include_str!("query.rs");
        let start = src
            .find("pub(crate) async fn execute_query_impl")
            .expect("fn");
        let chunk = &src[start..start + 800];
        assert!(
            !chunk.contains("tracing::info!(") || !chunk.contains("%sql_preview"),
            "execute_query must not info!-log sql_preview"
        );
        assert!(
            chunk.contains("sql_len") || chunk.contains("tracing::debug!"),
            "expected sql_len and/or debug preview"
        );
        assert!(
            chunk.contains("sql_preview_for_log"),
            "execute_query debug preview must use log_redact"
        );
    }

    #[test]
    fn execute_query_stream_source_does_not_info_log_sql_preview() {
        let src = include_str!("driver_command/streaming.rs");
        let start = src
            .find("pub(crate) async fn execute_driver_command_stream_impl")
            .expect("fn");
        let end = src[start..]
            .find("let read_only = state")
            .map(|i| start + i)
            .unwrap_or(start + 8000);
        let chunk = &src[start..end];
        assert!(
            !chunk.contains("tracing::info!(") || !chunk.contains("%sql_preview"),
            "execute_driver_command_stream must not info!-log sql_preview"
        );
        assert!(
            chunk.contains("sql_preview_for_log"),
            "execute_driver_command_stream debug preview must use log_redact"
        );
    }
}

#[cfg(test)]
#[path = "query_tests.rs"]
mod tests;
