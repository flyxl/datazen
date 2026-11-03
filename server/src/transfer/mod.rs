//! WR-D7 传输面：幂等提交、受控上传、受控下载。
//!
//! 三个 handler 级服务（**不接线**——路由表与 composition root 的装配归主线，
//! 见 WR-D7 报告的接线清单）：
//!
//! - [`submission`]：`SubmissionService` —— 校验签名提交令牌（WR-D4 的
//!   `ServerSubmissionTokenIssuer`）→ [`PreSubmitGate`] 提交前闸门 →
//!   `JobRepository` 幂等受理（同键同指纹重放原 receipt、同键异指纹
//!   `IdempotencyConflict`，语义完全复用 `storage::ServerJobs`，不重造）→
//!   全链路审计事件。
//! - [`upload`]：`UploadService` —— 分块受控上传：ACL 归属双重检查、大小
//!   上限对齐 L1 字节闸（`crate::http::router::MAX_BODY_BYTES`）、SHA-256
//!   内容完整性、路径遍历零容忍，落 `storage::ServerArtifacts`。
//! - [`download`]：`DownloadService` —— `readArtifact` 的三取法
//!   （`?metadata=1` / `?chunkIndex=` / `?offset=&limit=`，与
//!   `apps/web/src/platform/web/httpRoutes.ts` 的 `readArtifactQuery` 逐字
//!   对齐），逐请求 ACL + 生命周期可读性，响应形状含 `x-dz-*` 信封补充头。
//!
//! # 模块纪律
//!
//! - 本模块是 HTTP **域逻辑**层：入口收 `RequestContext`（adapter 唯一构造，
//!   INV-01——身份由 L2 认证链盖章后传入，任何请求体字段不得覆盖），出口
//!   统一 `datazen::shared::error::ApiError` wire 词汇，HTTP 状态映射走
//!   `http::response::ApiErrorResponse`。
//! - 不持有任何秘密：令牌材料在 `ServerSubmissionTokenIssuer`（WR-D4），
//!   本层只见 `VerifiedSubmission` 事实。
//! - 审计伴随每个判定路径：成功 / 拒绝 / 冲突都产生 `AuditEvent`
//!   （`ServerAudit` 的 append-only 半边）；审计落库失败只 `tracing::warn!`
//!   （不掩盖主错误，也不让审计故障放大为 5xx——对账由 `request_id` 承担）。

pub mod download;
pub mod submission;
pub mod upload;

pub use download::{ArtifactReadMode, DownloadReply, DownloadService};
pub use submission::{SubmissionReceipt, SubmissionService};
pub use upload::{UploadLimits, UploadService, UploadSession};

use async_trait::async_trait;
use datazen::execution::application::submission::VerifiedSubmission;
use datazen::shared::context::RequestContext;
use datazen::shared::error::ApiError;

/// 提交前闸门（WR-D7 定义的钩子，WR-D6 `QuotaGate` 的适配点）。
///
/// 在「令牌校验通过」之后、「`JobRepository` 幂等受理」之前调用。放行 =
/// `Ok(())`；拒绝 = `Err(ApiError)`——**wire 词汇直接给映射**（额度类拒绝
/// 用 `ApiErrorCode::QuotaExceeded`，`http::response::status_for` 落 429），
/// 本层不再二次翻译。
///
/// 主线接线方式：把 WR-D6 额度协调器包一层
/// `struct QuotaPreSubmitGate(Arc<ServerBudgetCoordinator>)`，在
/// `check_submit` 里按 `submission.organization_id` / `submission.principal_id`
/// 申请任务额度，额度不足返回 `ApiError::new(QuotaExceeded, …)`；
/// `SubmissionService::new` 注入该适配器即可，`transfer` 侧零改动。
///
/// 语义约束：
///
/// - **只读**：闸门不得有副作用（不落库、不改账本）——它发生在幂等受理
///   之前，令牌重放路径同样会穿过它；有副作用的额度扣减必须由闸门实现
///   自幂等（按 `submission.idempotency_key` 去重），否则重放被双重计费。
/// - **fail-closed**：闸门内部故障应返回 `ServiceUnavailable`，不得默认
///   放行。
#[async_trait]
pub trait PreSubmitGate: Send + Sync + 'static {
    /// 对一次已通过令牌校验的提交做准入判定。
    async fn check_submit(
        &self,
        ctx: &RequestContext,
        submission: &VerifiedSubmission,
    ) -> Result<(), ApiError>;
}

/// 全放行闸门：最小部署形态（无额度约束）与测试缺省。
#[derive(Debug, Clone, Copy, Default)]
pub struct AllowAllGate;

#[async_trait]
impl PreSubmitGate for AllowAllGate {
    async fn check_submit(
        &self,
        _ctx: &RequestContext,
        _submission: &VerifiedSubmission,
    ) -> Result<(), ApiError> {
        Ok(())
    }
}
