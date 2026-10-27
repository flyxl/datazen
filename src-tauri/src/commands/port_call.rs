//! 端口调用 → 命令结果的统一收口。
//!
//! 每个走 `platform-api` 端口的命令族都需要同一件事：把 [`PortError`] 映射成
//! [`CommandError`]，并在映射之前**记一条脱敏日志**（与
//! [`CmdExt::cmd_err`](crate::commands::error::CmdExt) 同形）。
//!
//! 之所以抽成一处而不是每个命令族各写一份：映射表本身就是约定（`NotFound` 必须
//! 保留成 `NotFound`，否则前端「连接已被删除」的提示会退化成「内部错误」），
//! 复制几份之后必然有一份先漂移。日志的脱敏走
//! [`redact_secrets_for_log`](datazen_infra::redaction::redact_secrets_for_log)，
//! 与全局 IPC 错误载荷同一条规则。

use datazen_modules::shared::error::PortError;

use crate::commands::error::CommandError;

/// `PortError` → `CommandError`。
///
/// `BackendUnavailable` 的 detail 已经是实现方给的**脱敏**文案（落盘错误、
/// 目录不可写之类），直接作为内部错误透传；其余变体一律降成内部错误字符串。
pub(crate) fn port_error_to_command(error: PortError) -> CommandError {
    match error {
        PortError::BackendUnavailable(detail) => CommandError::Internal(detail),
        PortError::NotFound(entity) => CommandError::NotFound(entity),
        other => CommandError::Internal(other.to_string()),
    }
}

/// 端口调用 → 命令结果：映射之外**照旧记一条 error 日志**。
pub(crate) fn port_cmd<T>(result: Result<T, PortError>, cmd: &str) -> Result<T, CommandError> {
    result.map_err(|error| {
        let err = port_error_to_command(error);
        let redacted = datazen_infra::redaction::redact_secrets_for_log(&err.to_string());
        tracing::error!(cmd, error = %redacted);
        err
    })
}
