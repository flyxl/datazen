//! W8-a P2：[`TargetCommandRunner`] 端口的 server 实现（tableData 读半边的
//! 执行面）。
//!
//! 端口由消费者定义（`datazen::table_browsing::api`，R1）：读/预览算法
//! （语句组装、批次布局、计数回退、结果合并）在共享层；本件只保留传输
//! 胶水——目标解析（经 WR-F7 [`ResourceExecutionHost::prepare`] 闸序）、
//! 提交令牌、执行、终态轮询与产物分块收集。桌面参照实现是
//! `datazen-desktop::commands::table_target::DesktopTargetRunner`，本件是
//! 同一端口的 server 形态（分支语义与文案逐字对齐）。
//!
//! ## 分支语义
//!
//! - **target 分支**（`manual_handle` 为 `None`）：`host.prepare`
//!   （闸序 = profile 存在 → 未禁用 → 配置版本闸 → 连接配置 → 驱动可用
//!   → 目标规范化，`resource_host.rs` 模块头登记）→
//!   `issue_submission_token(ExecuteAtTarget)` → `execute_at_target`
//!   （`prepared.target` 是规范化目标；`expected_config_revision` =
//!   binding 的调用方已读版本，CAS fail-closed）→ 轮询到终态 → 要求
//!   `Succeeded` → 逐产物分块收集。收集走 `PreparedExecution.artifacts`
//!   端口（[`ArtifactChunkReader`]）——`ConnectionUseCases` trait 无
//!   `read_artifact_chunk`，target 分支因此必须走 artifacts 端口。
//! - **manual_handle 分支**：租约**每调用现取**
//!   （[`RuntimeLeaseSource`] 纪律：runtime 可能被吊销，禁止缓存用例面）
//!   → `get_session` → 三字段校验（`connection_id` /
//!   `config_revision` / `observed_context.namespace` 任一与 binding
//!   不符 ⇒ `InvalidArgument`「Manual table session target changed」）→
//!   `issue_submission_token(ExecuteInSession, Some(handle))` →
//!   `execute_in_session`（带 `expected_context_revision` CAS）→ 轮询收集
//!   （`RuntimeConnectionUseCases` 固有 `read_artifact_chunk` 经
//!   `resource_host` 的 [`ServerArtifactChunks`] 投影——与 target 分支
//!   共享同一收集端口形状）。
//!
//! ## 授权语（P3 收口点登记）
//!
//! 执行面（token / execute / poll / collect）全部使用
//! [`TargetCommandRunner::run`] 的 `RequestContext`（请求真主体）；
//! `prepare` 内部的 profile 读取沿用 host 的合成服务主体
//! `server-mcp-tool`（`resource_host.rs` 登记第 2 条，随 P3 收口）。
//! 租约解析失败 → `ServiceUnavailable`（`lease_use_cases` 同款）；
//! `WorkflowError` → `ApiError` 沿 `driver_gateway` 既有裁决：
//! `Driver` / `ConnectionFailed` → `ServiceUnavailable`，其余 →
//! `InvalidArgument`（HTTP 状态码细化属 wire 半边，本端口只产出
//! `ApiError`）。
//!
//! 端口不重试、不缓存、不做业务判断（api.rs 端口契约）；失败一律
//! `ApiError`，不产出看似成功的结果。

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value as Json;

use datazen::execution::application::requests::{
    CommandCall, ExecuteAtTargetRequest, ExecuteInSessionRequest,
};
use datazen::execution::application::use_cases::ConnectionUseCases;
use datazen::execution::application::IdempotentOperation;
use datazen::execution::{ExecutionReceipt, ExecutionState, ExecutionView};
use datazen::shared::context::RequestContext;
use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::shared::id::{ArtifactId, Counter};
use datazen::table_browsing::api::{TargetBinding, TargetCommandRunner};
use datazen::table_editing::ports::ArtifactChunkReader;
use datazen::workflow::application::resource_execution::ResourceExecutionHost;
use datazen::workflow::domain::error::WorkflowError;

use crate::execution::lease::RuntimeLeaseSource;
use crate::execution::resource_host::ServerArtifactChunks;

/// 终态轮询间隔（桌面参照实现同款 25ms；执行生命周期归 runtime，端口不设超时）。
const POLL_INTERVAL_MS: u64 = 25;

/// server 形态的 table 目标命令执行器。
///
/// `host` 与 driverGateway 过渡域、（mcp 构建下的）目标机器共享同一实例；
/// `lease` 与 sessions 域同一租约源（共享 sink + 会话目录）——bootstrap
/// 既有纪律，本件不自建任何旁路实例。
pub struct ServerTargetRunner {
    host: Arc<dyn ResourceExecutionHost>,
    lease: Arc<dyn RuntimeLeaseSource>,
}

impl ServerTargetRunner {
    /// 构造（bootstrap 装配入口）。
    pub fn new(host: Arc<dyn ResourceExecutionHost>, lease: Arc<dyn RuntimeLeaseSource>) -> Self {
        Self { host, lease }
    }
}

#[async_trait]
impl TargetCommandRunner for ServerTargetRunner {
    async fn run(
        &self,
        context: &RequestContext,
        binding: &TargetBinding,
        command: &str,
        input: Json,
    ) -> Result<Vec<Json>, ApiError> {
        // 两分支收敛到同一执行面词汇：`ConnectionUseCases`（轮询）+
        // `ArtifactChunkReader`（收集）。target 分支来自 `PreparedExecution`
        // 的端口集合；manual 分支把租约现取的 runtime 投影到同一形状。
        let (receipt, cases, chunks): (
            ExecutionReceipt,
            Arc<dyn ConnectionUseCases>,
            Arc<dyn ArtifactChunkReader>,
        ) = if let Some(handle) = binding.manual_handle.as_ref() {
            let runtime = self
                .lease
                .resolve()
                .await
                .map_err(|message| ApiError::new(ApiErrorCode::ServiceUnavailable, message))?;
            let session = runtime.get_session(context, handle.clone()).await?;
            if session.connection_id != binding.target.connection_id
                || session.config_revision != binding.revision
                || session.observed_context.namespace != binding.target.namespace
            {
                return Err(ApiError::new(
                    ApiErrorCode::InvalidArgument,
                    "Manual table session target changed",
                ));
            }
            let token = runtime
                .issue_submission_token(
                    context,
                    IdempotentOperation::ExecuteInSession,
                    Some(handle),
                )
                .await?;
            let receipt = runtime
                .execute_in_session(
                    context,
                    ExecuteInSessionRequest {
                        handle: handle.clone(),
                        expected_context_revision: Some(session.context_revision),
                        call: CommandCall {
                            command: command.into(),
                            input,
                        },
                        idempotency_key: token.idempotency_key,
                    },
                )
                .await?;
            let chunks: Arc<dyn ArtifactChunkReader> =
                Arc::new(ServerArtifactChunks(runtime.clone()));
            let cases: Arc<dyn ConnectionUseCases> = runtime;
            (receipt, cases, chunks)
        } else {
            let prepared = self
                .host
                .prepare(
                    binding.target.connection_id.as_str(),
                    binding.revision.get(),
                    command,
                    binding.target.clone(),
                )
                .await
                .map_err(workflow_error)?;
            let token = prepared
                .use_cases
                .issue_submission_token(context, IdempotentOperation::ExecuteAtTarget, None)
                .await?;
            let receipt = prepared
                .use_cases
                .execute_at_target(
                    context,
                    ExecuteAtTargetRequest {
                        target: prepared.target,
                        expected_config_revision: binding.revision,
                        call: CommandCall {
                            command: command.into(),
                            input,
                        },
                        idempotency_key: token.idempotency_key,
                    },
                )
                .await?;
            (receipt, prepared.use_cases, prepared.artifacts)
        };
        let view = poll_to_terminal(cases.as_ref(), context, receipt).await?;
        if view.state != ExecutionState::Succeeded {
            return Err(ApiError::new(
                ApiErrorCode::InvalidArgument,
                format!(
                    "Table execution failed: {:?}; effect outcome: {:?}",
                    view.error_code, view.effect_outcome
                ),
            ));
        }
        collect_outputs(chunks.as_ref(), context, &view.artifact_ids).await
    }
}

/// 轮询一条执行到终态（桌面参照实现同款：25ms 间隔，不做业务判断）。
async fn poll_to_terminal(
    use_cases: &dyn ConnectionUseCases,
    context: &RequestContext,
    receipt: ExecutionReceipt,
) -> Result<ExecutionView, ApiError> {
    loop {
        let view = use_cases
            .get_execution(context, receipt.execution_id.clone())
            .await?;
        if view.state.is_terminal() {
            return Ok(view);
        }
        tokio::time::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
}

/// 逐产物分块收集（桌面参照实现同款序：首块先读，`published_chunk_count`
/// 决定后续取块数；`output` 为 `None` 的块是纯行集块，跳过不入列）。
async fn collect_outputs(
    chunks: &dyn ArtifactChunkReader,
    context: &RequestContext,
    artifacts: &[ArtifactId],
) -> Result<Vec<Json>, ApiError> {
    let mut outputs = Vec::new();
    for artifact in artifacts {
        let first = chunks.read_chunk(context, artifact, Counter::ZERO).await?;
        for index in 0..first.published_chunk_count.get() {
            let chunk = if index == 0 {
                first.clone()
            } else {
                chunks
                    .read_chunk(context, artifact, Counter::new(index))
                    .await?
            };
            if let Some(output) = chunk.output {
                outputs.push(output);
            }
        }
    }
    Ok(outputs)
}

/// `WorkflowError` → `ApiError`（`driver_gateway` 既有裁决的 ApiError 面）。
///
/// `Driver` / `ConnectionFailed` → `ServiceUnavailable`（目标不可服务，
/// 不是调用方的输入错误）；其余 → `InvalidArgument`。HTTP 状态码细化
/// （`GlobalTimeout` → 504 等）属 wire 半边，端口语义只产出 `ApiError`。
fn workflow_error(error: WorkflowError) -> ApiError {
    let code = match &error {
        WorkflowError::Driver(_) | WorkflowError::ConnectionFailed { .. } => {
            ApiErrorCode::ServiceUnavailable
        }
        _ => ApiErrorCode::InvalidArgument,
    };
    ApiError::new(code, error.to_string())
}
