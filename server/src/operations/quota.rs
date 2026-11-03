//! `QuotaGate`：提交闸门（WR-D6 第一优先件）。
//!
//! 语义（web-ready WR-D6）：执行提交前检查**节点与组织**两级额度——调用
//! [`BudgetCoordinator::try_acquire`]（modules 端口；`crate::execution::budget::ServerBudgetCoordinator`
//! 是其服务端适配器，装配时注入同一直属账）。立即尝试（不等待）是闸门的
//! 裁决：闸门答「放行/拒绝」，排队等待归预算协调器的 `acquire`。
//!
//! * **放行**：返回 [`QuotaAdmission`]，内含 [`BudgetPermit`]——持有者
//!   负责在执行终态调用 [`QuotaGate::release`]（`Consumed`）或提交失败时
//!   归还（`Unused`）。端口 INV-10 保证释放幂等。
//! * **拒绝**：构造 [`QuotaRejection`]（尽力从 [`BudgetCoordinator::snapshot`]
//!   收集余量信息；端口 v1 快照是**节点聚合值**，组织分账余量是已登记
//!   缺口），**审计事件落 [`AuditSink`]**，并经 [`QuotaRejection::to_api_error`]
//!   映射为 `quotaExceeded`（429，`http::response::status_for` 的既有行）。
//!   审计失败不吞业务响应（§3.2）：拒绝照样返回，失败进日志。
//! * **后端故障**：`BackendUnavailable` / `ProviderTimeout` 不是容量事实，
//!   不走 429 拒绝面；本闸门把它翻译为 `serviceUnavailable`（503）并记
//!   `quota_gate_error` 审计（`Failed`），与容量拒绝在审计上分得开。
//!
//! 消息脱敏：拒绝文案只含计数与维度词，不含连接名/用户输入。

use std::sync::Arc;

use datazen::execution::application::budget::{
    BudgetCoordinator, BudgetPermit, BudgetRequest, ReleaseOutcome,
};
use datazen::shared::audit::{AuditEvent, AuditOutcome, AuditPrincipalKind, AuditSink};
use datazen::shared::context::RequestContext;
use datazen::shared::error::{ApiError, ApiErrorCode, PortError};
use datazen::shared::id::Timestamp;

/// 提交闸门。`B` 是 modules 的 [`BudgetCoordinator`] 端口实现
/// （服务端装配注入 `ServerBudgetCoordinator`）。
pub struct QuotaGate<B: BudgetCoordinator> {
    budget: Arc<B>,
    audit: Arc<dyn AuditSink>,
}

/// 闸门放行：调用方拿走 permit 并持有到执行终态。
#[derive(Debug, Clone)]
pub struct QuotaAdmission {
    /// 产生本许可的请求（组织/连接维度，供释放路径复用）。
    pub request: BudgetRequest,
    /// 已入账的在飞许可。
    pub permit: BudgetPermit,
}

/// 拒绝原因（端口能区分的两类容量事实）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaRejectionReason {
    /// 节点或组织在飞许可已达上限（`try_acquire` 返回 `None`）。
    CapacityExhausted,
    /// 预算账处于单向排空（`PortError::QuotaExceeded`——排空期间不发新许可）。
    Draining,
}

/// 拒绝事实（含尽力收集的余量信息）。
#[derive(Debug, Clone)]
pub struct QuotaRejection {
    pub organization_id: String,
    pub connection_id: String,
    pub reason: QuotaRejectionReason,
    /// 快照在飞数（端口 v1 是节点聚合；`None` = 快照不可用）。
    pub snapshot_in_use: Option<u64>,
    /// 快照累计发放数。
    pub snapshot_granted_total: Option<u64>,
}

impl QuotaRejection {
    /// 429 `quotaExceeded`（`status_for` 既有行）。文案只含计数，脱敏。
    pub fn to_api_error(&self) -> ApiError {
        let detail = match (self.snapshot_in_use, self.snapshot_granted_total) {
            (Some(in_use), Some(granted)) => {
                format!("in flight {in_use}, granted total {granted}")
            }
            _ => "budget snapshot unavailable".to_string(),
        };
        let why = match self.reason {
            QuotaRejectionReason::CapacityExhausted => "quota exhausted",
            QuotaRejectionReason::Draining => "budget is draining",
        };
        ApiError::new(
            ApiErrorCode::QuotaExceeded,
            format!("submission rejected: {why} ({detail})"),
        )
    }
}

impl<B: BudgetCoordinator> QuotaGate<B> {
    pub fn new(budget: Arc<B>, audit: Arc<dyn AuditSink>) -> Self {
        Self { budget, audit }
    }

    /// 提交闸门：立即尝试占用一个许可；拒绝路径**先审计后返回**。
    pub async fn gate(
        &self,
        ctx: &RequestContext,
        request: BudgetRequest,
    ) -> Result<QuotaAdmission, QuotaRejection> {
        match self.budget.try_acquire(request.clone()).await {
            Ok(Some(permit)) => Ok(QuotaAdmission { request, permit }),
            Ok(None) => Err(self
                .reject(ctx, &request, QuotaRejectionReason::CapacityExhausted)
                .await),
            Err(PortError::QuotaExceeded(_)) => {
                Err(self.reject(ctx, &request, QuotaRejectionReason::Draining).await)
            }
            // 基础设施事实（后端不可用/超时）：不经容量拒绝面，直接 503。
            Err(source) => Err(self.reject_backend(ctx, &request, source)),
        }
    }

    /// 执行终态/提交失败时归还许可（端口 INV-10：幂等）。
    pub async fn release(
        &self,
        permit: &BudgetPermit,
        outcome: ReleaseOutcome,
    ) -> Result<(), ApiError> {
        self.budget.release(permit, outcome).await.map_err(|source| {
            ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                format!("budget release failed: {source}"),
            )
        })
    }

    /// 容量拒绝路径：快照余量 + 审计 + 拒绝事实。审计失败只记日志（§3.2）。
    async fn reject(
        &self,
        ctx: &RequestContext,
        request: &BudgetRequest,
        reason: QuotaRejectionReason,
    ) -> QuotaRejection {
        // 余量信息尽力收集：快照失败不阻断拒绝（拒绝事实已由 try_acquire
        // 给出）。端口 v1 快照是节点聚合值（组织分账缺口已登记）。
        let (snapshot_in_use, snapshot_granted_total) = match self.budget.snapshot().await {
            Ok(snapshot) => (Some(snapshot.in_use.get()), Some(snapshot.granted_total.get())),
            Err(source) => {
                tracing::warn!(error = %source, "quota snapshot unavailable for denial detail");
                (None, None)
            }
        };
        let rejection = QuotaRejection {
            organization_id: request.organization_id.as_str().to_string(),
            connection_id: request.connection_id.as_str().to_string(),
            reason,
            snapshot_in_use,
            snapshot_granted_total,
        };
        let event = AuditEvent {
            principal_id: Some(ctx.principal_id.clone()),
            connection_id: Some(request.connection_id.clone()),
            error_code: Some("quotaExceeded".into()),
            request_id: Some(ctx.request_id.clone()),
            ..AuditEvent::new(
                request.organization_id.clone(),
                AuditPrincipalKind::User,
                "quota_denied",
                AuditOutcome::Denied,
                Timestamp::new(super::now_string()),
            )
        };
        if let Err(source) = self.audit.record(event) {
            // 审计写失败不吞业务响应，但必须告警（§3.2 同款处置）。
            tracing::error!(error = %source, "quota denial audit append failed");
        }
        rejection
    }

    /// 后端故障路径：拒绝事实保留，wire 语义是 503（不是 429），
    /// 并按 `Failed` 落审计（容量拒绝与后端故障在审计上必须分得开）。
    fn reject_backend(
        &self,
        ctx: &RequestContext,
        request: &BudgetRequest,
        source: PortError,
    ) -> QuotaRejection {
        tracing::warn!(error = %source, "quota gate backend failure");
        let event = AuditEvent {
            principal_id: Some(ctx.principal_id.clone()),
            connection_id: Some(request.connection_id.clone()),
            error_code: Some("serviceUnavailable".into()),
            request_id: Some(ctx.request_id.clone()),
            ..AuditEvent::new(
                request.organization_id.clone(),
                AuditPrincipalKind::User,
                "quota_gate_error",
                AuditOutcome::Failed,
                Timestamp::new(super::now_string()),
            )
        };
        if let Err(audit_error) = self.audit.record(event) {
            tracing::error!(error = %audit_error, "quota gate error audit append failed");
        }
        QuotaRejection {
            organization_id: request.organization_id.as_str().to_string(),
            connection_id: request.connection_id.as_str().to_string(),
            reason: QuotaRejectionReason::CapacityExhausted,
            snapshot_in_use: None,
            snapshot_granted_total: None,
        }
    }
}

