//! 幂等提交流程（WR-D7 第二刀）：签名提交令牌 → 提交前闸门 → 幂等受理。
//!
//! [team-server §13.1](../../../../../docs/architecture/platform/team-server-and-auth.md)
//! 与[连接 §13.1](../../../../../docs/architecture/platform/connection-management.md)
//! 的提交语义在宿主侧的落地——流程是**四个严格有序的判定**，任何一个失败
//! 都在失败点截断并出审计事件：
//!
//! 1. **令牌校验**（[`SubmissionTokenIssuer::verify`]）：HMAC 签名 + 未知
//!    `keyVersion` 拒绝 + 服务端时钟过期判定（WR-D4 适配器已实现，本层不复核
//!    签名，只消费 `VerifiedSubmission` 事实）。
//! 2. **身份交叉核对**：签名载荷的 org/principal/client 必须与 `ctx` 一致
//!    ——令牌是签名事实、`ctx` 是认证链盖章事实，两处不一致即伪造，拒绝
//!    （INV-01 的宿主侧补强）。
//! 3. **提交前闸门**（[`crate::transfer::PreSubmitGate`]）：WR-D6 额度
//!    适配点；拒绝时 wire 错误原样透传（额度类 → 429）。
//! 4. **幂等受理**：`storage::ServerJobs::accept`（`JobRepository` 端口）。
//!    **同键同指纹重放原 receipt**（令牌即幂等键——重放同令牌、同 payload
//!    拿回同一 `JobRecord`）；**同键异指纹报 `IdempotencyConflict`**；
//!    apply 计划同组织唯一消费也由仓储事务内完成。本层不重造任何去重逻辑。
//!
//! 审计词表：`job_submitted`（受理成功，含重放——重放也是一次成功的受理
//! 决策）、`job_submit_gate_denied`（闸门拒绝）、`job_submit_conflicted`
//! （指纹冲突）、`job_submit_rejected`（令牌无效 / 身份不一致 / 仓储错误）。

use std::sync::Arc;

use datazen::execution::application::idempotency::IdempotentOperation;
use datazen::execution::application::submission::{SubmissionPresentation, SubmissionTokenIssuer};
use datazen::jobs::domain::job::{JobDefinition, JobRecord};
use datazen::jobs::domain::repository::JobRepository;
use datazen::shared::audit::{AuditEvent, AuditOutcome, AuditPrincipalKind, AuditSink};
use datazen::shared::clock::Clock;
use datazen::shared::context::RequestContext;
use datazen::shared::error::{ApiError, ApiErrorCode, PortError};
use datazen::shared::id::Timestamp;

use crate::storage::{ServerAudit, ServerJobs};
use crate::transfer::PreSubmitGate;

/// 受理回执。就是 `JobRepository::accept` 的原始回执：宿主不投影、不包装
/// （HTTP 形态与桌面形态消费同一份 `JobRecord`，序列化 camelCase 由 DTO 定）。
pub type SubmissionReceipt = JobRecord;

/// 幂等提交服务。
pub struct SubmissionService {
    issuer: Arc<dyn SubmissionTokenIssuer>,
    jobs: Arc<ServerJobs>,
    gate: Arc<dyn PreSubmitGate>,
    audit: Arc<ServerAudit>,
    clock: Arc<dyn Clock>,
}

impl SubmissionService {
    /// 装配。四个协作者全部由 composition root 注入（§2.3：无隐式单例）。
    pub fn new(
        issuer: Arc<dyn SubmissionTokenIssuer>,
        jobs: Arc<ServerJobs>,
        gate: Arc<dyn PreSubmitGate>,
        audit: Arc<ServerAudit>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            issuer,
            jobs,
            gate,
            audit,
            clock,
        }
    }

    /// 幂等提交一个 Job。
    ///
    /// `presentation` 的 claimed 身份必须取自 `ctx`（handler 组装的唯一合法
    /// 来源）；`definition.organization_id` 不作数——受理时仓储按 `ctx`
    /// 组织盖章（INV-01）。返回原 receipt（新受理或同键同指纹重放）。
    pub async fn submit_job(
        &self,
        ctx: &RequestContext,
        presentation: &SubmissionPresentation,
        definition: JobDefinition,
    ) -> Result<SubmissionReceipt, ApiError> {
        // 1. 令牌校验（签名 / keyVersion / 过期，服务端时钟基准）。
        let verified = match self.issuer.verify(presentation).await {
            Ok(verified) => verified,
            Err(PortError::TokenInvalid) => {
                self.audit_denied(ctx, "job_submit_rejected", "unauthenticated");
                return Err(ApiError::new(
                    ApiErrorCode::Unauthenticated,
                    "Submission token is invalid, unknown-keyed, or expired.",
                ));
            }
            Err(source) => return Err(self.audit_port_failure(ctx, "job_submit_rejected", source)),
        };

        // 2. 身份交叉核对：签名事实 vs 认证链盖章事实，不一致即伪造。
        if verified.organization_id != ctx.organization_id
            || verified.principal_id != ctx.principal_id
            || verified.client_instance_id != ctx.client_instance_id
        {
            self.audit_denied(ctx, "job_submit_rejected", "unauthenticated");
            return Err(ApiError::new(
                ApiErrorCode::Unauthenticated,
                "Submission identity does not match the authenticated context.",
            ));
        }
        // 本服务的提交面只受理 Job（令牌 operation 必须是 StartJob；
        // 配置类 / 会话类操作由各自域的提交面消费同一闸门）。
        if verified.operation != IdempotentOperation::StartJob {
            self.audit_denied(ctx, "job_submit_rejected", "invalidArgument");
            return Err(ApiError::new(
                ApiErrorCode::InvalidArgument,
                "This submission endpoint only accepts startJob tokens.",
            ));
        }

        // 3. 提交前闸门（WR-D6 QuotaGate 适配点）。
        if let Err(denial) = self.gate.check_submit(ctx, &verified).await {
            self.audit_denied(ctx, "job_submit_gate_denied", denial.code.as_str());
            return Err(denial);
        }

        // 4. 幂等受理：同键同指纹 → 原 receipt；同键异指纹 → IdempotencyConflict。
        match self.jobs.accept(ctx, definition, &verified.idempotency_key).await {
            Ok(record) => {
                self.audit_outcome(
                    ctx,
                    "job_submitted",
                    AuditOutcome::Succeeded,
                    None,
                );
                Ok(record)
            }
            Err(PortError::IdempotencyConflict) => {
                self.audit_outcome(
                    ctx,
                    "job_submit_conflicted",
                    AuditOutcome::Failed,
                    Some(ApiErrorCode::IdempotencyConflict.as_str()),
                );
                Err(ApiError::new(
                    ApiErrorCode::IdempotencyConflict,
                    "The idempotency key was already used for a different submission payload.",
                ))
            }
            Err(source) => Err(self.audit_port_failure(ctx, "job_submit_rejected", source)),
        }
    }

    // ── 审计 ────────────────────────────────────────────────────────────────

    fn audit_event(&self, ctx: &RequestContext, action: &str, outcome: AuditOutcome, error_code: Option<&str>) -> AuditEvent {
        AuditEvent {
            principal_id: Some(ctx.principal_id.clone()),
            client_instance_id: Some(ctx.client_instance_id.clone()),
            delegation_id: ctx.delegation_id.clone(),
            error_code: error_code.map(str::to_string),
            request_id: Some(ctx.request_id.clone()),
            ..AuditEvent::new(
                ctx.organization_id.clone(),
                AuditPrincipalKind::User,
                action,
                outcome,
                self.occurred_at(),
            )
        }
    }

    fn audit_outcome(&self, ctx: &RequestContext, action: &str, outcome: AuditOutcome, error_code: Option<&str>) {
        self.record(self.audit_event(ctx, action, outcome, error_code));
    }

    fn audit_denied(&self, ctx: &RequestContext, action: &str, error_code: &str) {
        self.audit_outcome(ctx, action, AuditOutcome::Denied, Some(error_code));
    }

    /// 仓储错误 → wire 错误，同时出失败审计（主错误不因审计故障被掩盖）。
    fn audit_port_failure(&self, ctx: &RequestContext, action: &str, source: PortError) -> ApiError {
        let api = port_error_to_api_error(&source);
        self.audit_outcome(ctx, action, AuditOutcome::Failed, Some(api.code.as_str()));
        api
    }

    fn record(&self, event: AuditEvent) {
        if let Err(source) = self.audit.record(event) {
            tracing::warn!(error = %source, "transfer audit append failed");
        }
    }

    fn occurred_at(&self) -> Timestamp {
        self.clock.now()
    }
}

/// `PortError` → `ApiError` 的传输面统一映射（`http::response::status_for`
/// 再落到状态码；`ArtifactExpired` 的 404 特例由下载路径显式处理）。
pub fn port_error_to_api_error(source: &PortError) -> ApiError {
    let (code, message): (ApiErrorCode, String) = match source {
        PortError::BackendUnavailable(detail) | PortError::ProviderTimeout(detail) => {
            (ApiErrorCode::ServiceUnavailable, detail.clone())
        }
        PortError::CasConflict { entity, id } => (
            ApiErrorCode::ContextConflict,
            format!("{entity} {id} changed concurrently; re-read and retry."),
        ),
        PortError::NotFound(detail) => (ApiErrorCode::NotFound, detail.clone()),
        PortError::TokenInvalid => (
            ApiErrorCode::Unauthenticated,
            "Verification failed (signature, key version, scope, or expiry).".into(),
        ),
        PortError::ArtifactExpired => (
            ApiErrorCode::NotFound,
            "Artifact is expired or no longer visible.".into(),
        ),
        PortError::QuotaExceeded(detail) => (ApiErrorCode::QuotaExceeded, detail.clone()),
        PortError::IdempotencyConflict => (
            ApiErrorCode::IdempotencyConflict,
            "Same key, different request payload.".into(),
        ),
        PortError::StaleClaim => (
            ApiErrorCode::ContextConflict,
            "Stale or expired claim: write rejected.".into(),
        ),
        PortError::PlanAlreadyConsumed(plan_id) => (
            ApiErrorCode::ContextConflict,
            format!("Plan already consumed by an apply job: {plan_id}"),
        ),
        PortError::UnsupportedVersion(detail) => (
            ApiErrorCode::CapabilityUnsupported,
            format!("Unsupported version: {detail}"),
        ),
    };
    ApiError::new(code, message)
}
