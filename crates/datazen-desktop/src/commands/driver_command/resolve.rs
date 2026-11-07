use std::sync::Arc;

use super::super::error::CommandError;
use super::super::AppState;
use super::helpers::{nonempty, unbound_handle};
use datazen::execution::application::LiveConnectionLease;
use datazen::execution::SessionView;
use datazen_driver_api::{ConnectionHandle, DatabaseDriver};

pub(crate) struct ResolvedCommandDriver {
    pub(crate) driver: Arc<dyn DatabaseDriver>,
    pub(crate) handle: ConnectionHandle,
    pub(crate) _lease: Option<LiveConnectionLease>,
    pub(crate) session: Option<SessionView>,
    pub(crate) bound: bool,
}

/// 会话/驱动解析的**跨域共享入口**（B2 域刀确认为保留项）。
///
/// 调用方（待各自域刀把编排收编后评估回收）：
/// * 本域：两个 discovery 命令、`execute_driver_command_impl`、
///   `execute_driver_command_stream_impl`；
/// * 生产（域外）：`commands/query.rs`（cancel_query）、
///   `commands/value_search.rs`、`commands/fk_navigation.rs`、
///   `commands/schema.rs`（estimate_table_rows）、
///   `commands/result_origin.rs`（也消费 `ResolvedCommandDriver`）、
///   `commands/data/table_columns.rs`。
///
/// 宿主绑定是它的本质（platform runtime 租借 + 驱动注册表），不下沉；
/// 「会话缺失 → `DB session {id} is unavailable`」与「无会话无类型 →
/// `dbSessionId or driverType is required`」是冻结的 IPC 文案。
pub(crate) async fn resolve_command_driver(
    state: &AppState,
    db_session_id: Option<&String>,
    driver_type: Option<&String>,
) -> Result<ResolvedCommandDriver, CommandError> {
    if let Some(id) = nonempty(db_session_id) {
        let adapter = state
            .platform_require()
            .map_err(crate::platform::into_command_error)?;
        let call = adapter.begin_call();
        let runtime = adapter
            .runtime()
            .map_err(crate::platform::into_command_error)?;
        let (session, lease) = runtime
            .borrow_live_connection_by_id(
                call.context(),
                id,
                datazen::execution::application::LivePurpose::CommandExecution,
            )
            .await
            .map_err(|error| {
                if error.code == datazen::shared::error::ApiErrorCode::SessionNotFound {
                    CommandError::NotFound(format!("DB session {id} is unavailable"))
                } else {
                    crate::platform::into_command_error(error)
                }
            })?;
        let connection = lease
            .connection()
            .map_err(|error| CommandError::Internal(error.to_string()))?;
        return Ok(ResolvedCommandDriver {
            driver: Arc::clone(connection.driver()),
            handle: connection.handle().clone(),
            _lease: Some(lease),
            session: Some(session),
            bound: true,
        });
    }
    let driver_type = nonempty(driver_type)
        .ok_or_else(|| CommandError::Validation("dbSessionId or driverType is required".into()))?;
    let driver = state
        .driver_registry
        .get(&driver_type.to_string())
        .await
        .ok_or_else(|| CommandError::NotFound(format!("Driver not found: {driver_type}")))?;
    Ok(ResolvedCommandDriver {
        driver,
        handle: unbound_handle(),
        _lease: None,
        session: None,
        bound: false,
    })
}
