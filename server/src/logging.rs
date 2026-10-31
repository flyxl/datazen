//! `tracing` 初始化（team-server §2.3 的 B 步：logging + 脱敏挂载）。
//!
//! # 脱敏契约（D1 记录，WR-D2 开始有消费者）
//!
//! 任何会进入日志的用户可见文本（driver 错误、SQL、路径、URL）必须先过
//! `datazen-infra` 的 redaction（`packages/infra/src/redaction.rs`：
//! `redact_secrets_for_log` / `redact_url_for_log` / `sql_preview_for_log`），
//! 默认不打印密码、令牌、原始数据库错误与用户绝对路径。D1 骨架没有携带
//! 秘密的日志路径，因此该 crate 尚未出现在依赖里；WR-D2 的真实 handler
//! 引入第一条业务日志时同时引入依赖与调用点——晚于第一条秘密进日志的
//! 修复成本是数量级差异，这里先把契约钉在组装点上。

use crate::error::ServerError;

/// 初始化全局 `tracing` 订阅器。只应被 `bootstrap::serve` 调用一次。
pub fn init(filter: &str) -> Result<(), ServerError> {
    let env_filter = tracing_subscriber::EnvFilter::try_new(filter)
        .map_err(|e| ServerError::LoggingInit(e.to_string()))?;
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .try_init()
        .map_err(|e| ServerError::LoggingInit(e.to_string()))?;
    Ok(())
}
