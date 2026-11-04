//! §6.4 等待能力：等待执行终态，而不是在 Controller 轮询。
//!
//! **第一步（本文件）**：把桌面命令层既有的 25ms 轮询封装进应用机制——行为
//! 与旧 `table_target` 的「查询终态」完全一致，调用方不再自旋。第二步（后续
//! 切片）换成通知实现（registry 终态观察 + `Notify`），两步都有行为测试钉住。
//!
//! 语义（§6.4 逐条落地）：
//!
//! * **重复等待已经完成的执行返回同一终态，不重新执行**：终态查询是幂等的
//!   只读投影；`wait_on_a_terminal_execution_returns_the_same_terminal_state`
//!   钉住「第二次等待不再产生新的执行 ID」。
//! * **先注册观察再复核状态**：轮询形态天然满足——检查与下一次检查之间
//!   完成的执行不会被漏掉（下一次循环必然看到终态）。
//! * **等待超时只说明等待超时**：超时以 [`WaitOutcome::DeadlineExceeded`]
//!   表达——它**不是**错误，更不构成「SQL 已取消」的证据；显式取消必须走
//!   `cancel_execution`。刻意的签名决策：不新增 `ApiErrorCode` 变体（§5.1
//!   错误码兼容由测试决定，不夹带在能力封装里）。
//! * **执行不存在 / 无权访问映射既有契约**：直接透传 `get_execution` 的
//!   `NotFound` / `PermissionDenied`，不吞错返回空结果。
//! * 取消一个等待者不影响执行本身与他人共享的等待。

use crate::execution::application::use_cases::ConnectionUseCases;
use crate::execution::{ExecutionId, ExecutionView};
use crate::shared::context::RequestContext;
use crate::shared::error::ApiError;

use super::RuntimeConnectionUseCases;

/// 等待结局。
#[derive(Debug, Clone, PartialEq)]
pub enum WaitOutcome {
    /// 执行已终态（`succeeded` / `failed` / `cancelled`）。
    /// 重复等待已完成执行返回同一终态，不重新执行。
    Terminal(ExecutionView),
    /// 等待超时。**只说明等待超时**：执行仍在原状态继续，取消必须走显式
    /// 取消用例；`waited` 是实际等待时长（毫秒）。
    DeadlineExceeded { waited_ms: u64 },
}

impl RuntimeConnectionUseCases {
    /// 轮询间隔：与桌面 `table_target` 既有 25ms 口径一致（§6.4 第一步：
    /// 保持行为；第二步换通知实现时此常量随之退役）。
    const WAIT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(25);

    /// 等待执行到达终态，或截止期到。
    ///
    /// 这是应用层能力，不是新的 IPC 面：宿主/用例层用它替代自旋轮询。
    /// 结果读取仍走 [`ConnectionUseCases::get_execution`] 的授权与投影，
    /// 本方法不绕过任何既有闸门。
    pub async fn wait_for_execution(
        &self,
        ctx: &RequestContext,
        id: ExecutionId,
        deadline: std::time::Duration,
    ) -> Result<WaitOutcome, ApiError> {
        let started = tokio::time::Instant::now();
        loop {
            // 先看状态再判超时：完成发生在「最后一次检查之后、返回之前」的
            // 竞态不存在——循环的下一轮必然看到终态；这里先查状态保证
            // 「截止期到的那一轮」也不会放过已完成的执行。
            let view = self.get_execution(ctx, id.clone()).await?;
            if view.state.is_terminal() {
                return Ok(WaitOutcome::Terminal(view));
            }
            let waited = started.elapsed();
            if waited >= deadline {
                // 超时不是错误，更不是取消证据（§6.4）。
                return Ok(WaitOutcome::DeadlineExceeded {
                    waited_ms: waited.as_millis() as u64,
                });
            }
            tokio::time::sleep(Self::WAIT_POLL_INTERVAL).await;
        }
    }
}
