//! Driver Command IPC — discovery, validation, execute, and streaming.
//!
//! B2 driver_command 域刀（commands-layer-review §3.3/§4，2026-10-31）：
//! `*_impl` 转发层拆除。命令发现（discovery）的两条接线直接住在
//! `#[tauri::command]` 处理函数里（原 discovery.rs 删除——目录合成本体
//! 早已是 `datazen::execution::application::command_policy::
//! discovery_definitions`，钉子测试随之归位该模块内联测试）；命令执行与
//! 流式执行因**跨域调用方**保留两个共享入口（见 execute.rs / streaming.rs
//! 的调用方清单注释），域内不再暴露 `pub(crate)` 转发 API（原
//! `execute_driver_command_with_mode` 收编为 execute.rs 的域内私有核心
//! `run_driver_command`）。
//!
//! IPC 契约逐字节不变：命令名、参数（serde camelCase）、返回 JSON 与错误
//! 文案（见 `tests/driver_command_ipc.rs`、`platform/ipc.rs` 冻结策略与
//! 本目录 tests.rs 的 C1/C2 钉子）。

mod execute;
mod helpers;
mod resolve;
mod streaming;
mod types;

#[cfg(test)]
#[path = "driver_command_test_host.rs"]
mod driver_command_test_host;

#[cfg(test)]
mod tests;

// ── 跨域共享入口（B2 域刀保留，待调用方各自域刀内联后回收）────────────
//
// `execute_driver_command_impl`（生产：query.rs / schema.rs；测试：
// export/live_acceptance.rs、schema/tests.rs、platform/session_backend/
// tests.rs、services/schema_metadata/tests.rs）与
// `execute_driver_command_stream_impl`（测试：query_tests.rs 五个流测试；
// query.rs 的日志卫生守卫还逐字扫它的函数体）的调用方清单见各自文件头。
// `resolve_command_driver` / `ResolvedCommandDriver` 是域外命令共用的驱动
// 解析入口（query / value_search / fk_navigation / schema / result_origin /
// data/table_columns），完整清单见 resolve.rs。
#[allow(unused_imports)]
pub(crate) use execute::execute_driver_command_impl;
pub(crate) use resolve::{resolve_command_driver, ResolvedCommandDriver};
#[allow(unused_imports)]
pub(crate) use streaming::execute_driver_command_stream_impl;
pub use types::{
    ExecuteDriverCommandRequest, ExecuteDriverCommandStreamOpts, ExecuteDriverCommandStreamRequest,
};

use std::sync::Arc;

use super::error::CommandError;
use super::AppState;
use datazen_driver_api::{
    CommandResult, DriverCommandDefinition, QueryStreamCallback, QueryStreamEvent,
};
use tauri::ipc::Channel;
use tauri::State;

/// Discover commands from a Driver type. No live Connection is required.
///
/// 接线（B2 域刀自 discovery.rs 内联）：按 driverType 取驱动 → 模块的
/// 发现目录合成（原生命令 + schema 元数据命令合并）。
#[tauri::command]
pub async fn get_driver_commands(
    state: State<'_, AppState>,
    driver_type: String,
) -> Result<Vec<DriverCommandDefinition>, CommandError> {
    let driver = state
        .driver_registry
        .get(&driver_type)
        .await
        .ok_or_else(|| CommandError::NotFound(format!("Driver not found: {driver_type}")))?;
    Ok(
        datazen::mcp::application::command_policy::discovery_definitions(
            driver.as_ref(),
        ),
    )
}

/// Discover commands from a concrete live DB session.
///
/// 接线（B2 域刀自 discovery.rs 内联）：经共享解析入口绑定活会话 → 同一份
/// 发现目录合成。
#[tauri::command]
pub async fn get_connection_commands(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<Vec<DriverCommandDefinition>, CommandError> {
    let resolved = resolve::resolve_command_driver(&state, Some(&db_session_id), None).await?;
    Ok(
        datazen::mcp::application::command_policy::discovery_definitions(
            resolved.driver.as_ref(),
        ),
    )
}

/// Execute a driver command from the GUI.
///
/// 接线（B2 域刀）：GUI IPC 传入 AppHandle，元数据声明保存对话框的命令
/// 可以弹原生对话框落盘；权限档位恒 `None`（带档位的 MCP / workflow 路径
/// 在 modules 侧自带管线）。执行编排在 execute.rs 的域内核心
/// `run_driver_command`，与域外共享入口共用同一份。
#[tauri::command]
pub async fn execute_driver_command(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: ExecuteDriverCommandRequest,
) -> Result<CommandResult, CommandError> {
    // GUI IPC passes the AppHandle so metadata-declared save dialogs can run.
    execute::run_driver_command(&state, request, None, Some(&app)).await
}

/// Stream a `query_stream` command's events to the caller's channel.
///
/// 接线（宿主形态部分）：tauri IPC `Channel` → 驱动流回调端口
/// （`QueryStreamCallback = Arc<dyn Fn(QueryStreamEvent) + Send + Sync>`），
/// 外加两个布尔开关的默认值。流式编排本体在 streaming.rs 的共享入口
/// `execute_driver_command_stream_impl`（query 域的 5 个流测试仍驱动它）；
/// 事件**发射**（往 Channel 送）留在本宿主，形态中立的遥测聚合在
/// `datazen::execution::application::stream_telemetry`。
#[tauri::command]
pub async fn execute_driver_command_stream(
    state: State<'_, AppState>,
    request: ExecuteDriverCommandStreamRequest,
    on_event: Channel<QueryStreamEvent>,
    apply_result_limit: Option<bool>,
    record_history: Option<bool>,
) -> Result<(), CommandError> {
    let callback: QueryStreamCallback = Arc::new(move |event| {
        let _ = on_event.send(event);
    });
    streaming::execute_driver_command_stream_impl(
        &state,
        request,
        callback,
        ExecuteDriverCommandStreamOpts {
            apply_result_limit: apply_result_limit.unwrap_or(true),
            record_history: record_history.unwrap_or(true),
        },
    )
    .await
}
