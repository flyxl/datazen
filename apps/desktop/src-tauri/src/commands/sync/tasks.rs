//! Persisted Sync task endpoint resolution (B2 sync 域刀残余).
//!
//! `get/save/delete_sync_task*` 与 `check_sync_conflicts` 的接线已内联进
//! `sync/mod.rs` 的命令体；本文件保留的是任务端点解析这一域内核心：
//! 持久化任务的 runtime 会话 id 是进程局部的、可能指向重启后的另一个库，
//! 所以冲突检查按 connectionId 重开**专用**会话，并在成功与每条错误路径
//! 上释放。

use super::super::error::CommandError;
use super::super::AppState;

pub(super) struct ResolvedTaskEndpoint {
    pub(super) db_session_id: String,
    pub(super) dedicated: bool,
    pub(super) driver: std::sync::Arc<dyn datazen_driver_api::DatabaseDriver>,
    pub(super) handle: datazen_driver_api::ConnectionHandle,
    pub(super) config: datazen_driver_api::ConnectionConfig,
}

pub(super) async fn release_task_endpoint(state: &AppState, endpoint: &ResolvedTaskEndpoint) {
    if !endpoint.dedicated {
        return;
    }
    if let Err(error) =
        crate::services::migration_endpoint::close_dedicated_session(state, &endpoint.db_session_id)
            .await
    {
        tracing::warn!(
            db_session_id = %endpoint.db_session_id,
            %error,
            "Failed to release dedicated sync task session"
        );
    }
}

async fn release_task_session_if_dedicated(state: &AppState, db_session_id: &str, dedicated: bool) {
    if !dedicated {
        return;
    }
    if let Err(error) =
        crate::services::migration_endpoint::close_dedicated_session(state, db_session_id).await
    {
        tracing::warn!(
            db_session_id,
            %error,
            "Failed to release dedicated sync task session"
        );
    }
}

/// Resolve a persisted task endpoint from its stable connection id. A task's
/// runtime session ids are intentionally ignored: they are process-local and
/// may refer to a different database after restart. Runtime opens a dedicated
/// session from the stored config for this check.
pub(super) async fn resolve_task_endpoint(
    state: &AppState,
    connection_id: &str,
    database: Option<&str>,
    _schema: Option<&str>,
    label: &str,
) -> Result<ResolvedTaskEndpoint, CommandError> {
    let connection_id = connection_id.trim();
    if connection_id.is_empty() {
        return Err(CommandError::Validation(format!(
            "Sync task is missing its {label} connectionId; reopen the task and choose a connection"
        )));
    }

    let config = state
        .store
        .get_connection(connection_id)
        .await
        .ok_or_else(|| CommandError::NotFound(format!("connection '{connection_id}' not found")))?;
    let requested_database = database
        .filter(|value| !value.trim().is_empty())
        .or(config.database.as_deref())
        .ok_or_else(|| {
            CommandError::Validation(format!(
                "Sync task is missing its {label} database; reopen the task and choose a database"
            ))
        })?;
    let db_session_id = crate::services::migration_endpoint::open_dedicated_session(
        state,
        connection_id,
        Some(requested_database),
        "syncTask",
    )
    .await
    .map_err(|_| {
        CommandError::Validation(format!(
            "could not open the persisted Sync task {label} session"
        ))
    })?;
    let context = match crate::services::migration_endpoint::runtime_session_context(
        state,
        &db_session_id,
        datazen::execution::application::LivePurpose::Inspection,
    )
    .await
    {
        Ok(context) => context,
        Err(error) => {
            release_task_session_if_dedicated(state, &db_session_id, true).await;
            return Err(error);
        }
    };
    let mut config = context.config;
    config.database = context
        .view
        .observed_context
        .namespace
        .database
        .clone()
        .or(config.database);
    let driver = context.driver;
    let handle = context.handle;
    let dedicated = true;

    // Keep database/schema explicit in the count and in the validation error.
    // A persisted task may outlive an edited connection config; silently
    // counting a different catalog would produce a false conflict result.
    if let Some(expected) = database.filter(|value| !value.trim().is_empty()) {
        if config.database.as_deref() != Some(expected) {
            let endpoint = ResolvedTaskEndpoint {
                db_session_id,
                dedicated,
                driver,
                handle,
                config,
            };
            release_task_endpoint(state, &endpoint).await;
            return Err(CommandError::Validation(format!(
                "Sync task {label} database '{}' is not active on connection '{}'; reopen the task and compare again",
                expected, connection_id
            )));
        }
    }
    // Schema is applied explicitly when qualifying the inspected relation,
    // so it may legitimately differ from the connection's mutable default
    // search schema. Keep the field in the task identity without requiring a
    // process-local search_path to match it.

    Ok(ResolvedTaskEndpoint {
        db_session_id,
        dedicated,
        driver,
        handle,
        config,
    })
}
