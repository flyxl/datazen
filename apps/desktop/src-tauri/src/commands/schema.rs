//! Schema 域 IPC：连接权限目录与行数估算。
//!
//! B2 schema 域刀（commands-layer-review §3.6）：`get_privileges_impl` /
//! `estimate_table_rows_impl` 转发层已拆除，接线直接住在 `#[tauri::command]`
//! 体里。形态中立的用例体已下沉 `datazen::schema::application`：
//! 权限目录组装在 [`privileges`](datazen::schema::application::privileges)，
//! 估算管线的数据库/模式解析梯子在
//! [`scope`](datazen::schema::application::scope)（`table_read_database` /
//! `metadata_schema`），行数估算本体（`QueryExecutor::estimate_row_count`）
//! 早就在 `datazen::services`。留在宿主的是会话解析与驱动网关接线：
//! 权限目录经 `driver_command` 网关读取（该网关是跨域共享入口），估算走
//! `resolve_command_driver` + 连接档案回读。
//!
//! IPC 契约逐字不变：命令名、参数、返回形状与错误文案。

use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::services::{embedded_schema_of, metadata_schema, QueryExecutor};
use datazen::schema::application::privileges::parse_grants_from_command;
use datazen_driver_api::schema_objects::PrivilegeGrant;
use std::time::Instant;
use tauri::State;

async fn run_schema_object_command(
    state: &AppState,
    db_session_id: &str,
    command: &str,
    input: serde_json::Value,
) -> Result<serde_json::Value, CommandError> {
    let result = super::driver_command::execute_driver_command_impl(
        state,
        super::driver_command::ExecuteDriverCommandRequest {
            db_session_id: Some(db_session_id.to_string()),
            driver_type: None,
            command: command.to_string(),
            input,
            database: None,
            schema: None,
        },
    )
    .await?;
    Ok(result.data)
}

#[tauri::command]
pub async fn get_privileges(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<Vec<PrivilegeGrant>, CommandError> {
    let data = run_schema_object_command(
        &state,
        &db_session_id,
        "list_privileges",
        serde_json::json!({}),
    )
    .await?;
    Ok(parse_grants_from_command(&data))
}

/// The estimated number of rows in a table.
///
/// `Ok(None)` means "this database cannot estimate this table" — an answer,
/// not a failure. The command never falls back to an exact count behind the
/// caller's back: deciding to spend that work is the caller's business.
#[tauri::command]
pub async fn estimate_table_rows(
    state: State<'_, AppState>,
    db_session_id: String,
    table: String,
    database: Option<String>,
    schema: Option<String>,
) -> Result<Option<i64>, CommandError> {
    let start = Instant::now();
    tracing::info!(%db_session_id, %table, "estimate_table_rows");
    let resolved =
        super::driver_command::resolve_command_driver(&state, Some(&db_session_id), None).await?;
    let driver = &resolved.driver;
    let handle = &resolved.handle;

    let config = resolved
        .session
        .as_ref()
        .ok_or_else(|| CommandError::NotFound("DB session is unavailable".into()))?;
    let config = state
        .store
        .get_connection(config.connection_id.as_str())
        .await
        .ok_or_else(|| CommandError::NotFound("connection profile is unavailable".into()))?;
    // 数据库梯子（请求 → 档案 → "default"）与模式梯子（显式 → 限定名内嵌 →
    // 档案 → 驱动默认）都已下沉 `schema::application::scope`，与网格读取
    // （value_search 等）爬同一份梯子。
    let database = datazen::schema::application::scope::table_read_database(
        database.as_deref(),
        config.database.as_deref(),
    );
    let schema = metadata_schema(
        driver.as_ref(),
        schema.as_deref(),
        embedded_schema_of(&table),
        config.schema.as_deref(),
    );

    let executor = QueryExecutor::new(state.schema_cache.clone());
    let estimate = executor
        .estimate_row_count(&driver, &handle, database, schema.as_deref(), &table)
        .await
        .cmd_err("estimate_table_rows")?;
    tracing::info!(
        %db_session_id,
        %table,
        estimated = ?estimate,
        ms = start.elapsed().as_millis() as u64,
        "estimate_table_rows OK"
    );
    Ok(estimate)
}

#[cfg(test)]
mod command_test_host;

#[cfg(test)]
mod count_tests;
#[cfg(test)]
mod tests;
