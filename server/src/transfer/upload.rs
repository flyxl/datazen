//! 受控上传（WR-D7 第三刀）：分块上传 → SHA-256 完整性 → `ServerArtifacts`。
//!
//! 契约位置：http-api-contract §6.5 的在账端点
//! `POST /api/v1/artifacts/uploads`（分块受控上传；收口动作随 web 导出流
//! 立项——本模块提供的是 handler 级函数，路由接线归主线）。
//!
//! 五条硬约束（全在字节进入 `ArtifactRepository` **之前**判定）：
//!
//! 1. **ACL 归属双重检查**（组织 + 资源）：`spec.organization_id` 必须等于
//!    `ctx` 组织、`spec.owner_principal_id` 必须等于 `ctx` 主体、委托上下文
//!    一律拒绝（产物 create 绑定个人 owner，服务端形态没有委托注册通道）。
//!    仓储的 create 还会再做一次同样的绑定校验——双道防线，不是冗余。
//! 2. **大小上限**（对齐 L1 字节闸）：单块上限缺省取
//!    [`crate::http::router::MAX_BODY_BYTES`]（8 MiB，L1a 已把请求体物理
//!    闸在进 handler 之前；这里是 handler 级的第二道语义闸，两者共用同一
//!    配置常量，测试可注入更小的值验证边界）；累计上限取契约配额
//!    [`ARTIFACT_BYTE_QUOTA`]（256 MiB）。超限：单块 → `PayloadTooLarge`，
//!    累计 → `QuotaExceeded` 并显式 `abort`（消费超限走 abort 的既定口径，
//!    已发布块保留、标记 truncated，不宣称完整）。
//! 3. **content-hash 完整性**：SHA-256（`sha2`——server 已有直接依赖，
//!    WR-D3 的 CSRF/HMAC 同款原语，**无新增 crate**）随写随算；finalize 时
//!    与调用方声明的摘要比对，不一致 → `abort` + `InvalidArgument`。
//! 4. **路径遍历零容忍**：客户端提供的任何标识符只有在通过
//!    [`ensure_artifact_id_is_safe`] 白名单校验后才允许进入仓储层；本层
//!    **从不**用请求输入拼文件系统路径——字节半边路径
//!    `<root>/<artifact_id>/<index>.chunk` 的两个分量分别是装配层配置与
//!    服务端铸造的 `ArtifactId`（UUID），与请求体无关。
//! 5. **全链路审计**：受理 / 拒绝 / 冲突 / 完成各出一条事件
//!    （`artifact_upload_started` / `artifact_chunk_rejected` /
//!    `artifact_upload_hash_mismatch` / `artifact_uploaded` /
//!    `artifact_upload_aborted`）。

use std::sync::Arc;

use sha2::{Digest, Sha256};

use datazen::execution::domain::artifact_repository::{
    ArtifactRepository, ArtifactSpec, ArtifactWriter, ChunkIndex, ChunkPayload, ARTIFACT_BYTE_QUOTA,
};
use datazen::execution::domain::AbortReason;
use datazen::shared::audit::{AuditEvent, AuditOutcome, AuditPrincipalKind, AuditSink};
use datazen::shared::clock::Clock;
use datazen::shared::context::RequestContext;
use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::shared::id::{ArtifactId, Timestamp};

use crate::storage::{ServerArtifacts, ServerAudit};

/// 上传限额。缺省值对齐两条既有权威：单块 = L1 字节闸（router 的
/// `MAX_BODY_BYTES`），累计 = 契约配额（`ARTIFACT_BYTE_QUOTA`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadLimits {
    /// 单块字节上限（含请求体解 JSON 的现实——L1 闸在前，这里语义收口）。
    pub max_chunk_bytes: usize,
    /// 单产物累计字节上限。
    pub max_total_bytes: u64,
}

impl Default for UploadLimits {
    fn default() -> Self {
        Self {
            max_chunk_bytes: crate::http::router::MAX_BODY_BYTES,
            max_total_bytes: ARTIFACT_BYTE_QUOTA,
        }
    }
}

/// 一次受控上传的进行时状态：仓储 writer 句柄 + 随写随算的内容摘要。
///
/// 持有者必须 [`UploadService::finalize_upload`] 或
/// [`UploadService::abort_upload`]——丢弃由 `ServerArtifacts` 的打开期恢复
/// 扫描兜底为 truncated（WriterLost），不宣称完整。
pub struct UploadSession {
    /// create 时的规格（ACL 已双重绑定）。
    pub spec: ArtifactSpec,
    /// 服务端铸造的产物标识（唯一的文件系统分量，客户端提供的标识符
    /// 永不参与路径拼接）。
    pub artifact_id: ArtifactId,
    /// 仓储写入句柄；index 只进不退。
    pub writer: ArtifactWriter,
    /// 已发布字节数（服务侧口径，来自 append 回执的累计）。
    pub published_bytes: u64,
    digest: Sha256,
}

impl UploadSession {
    /// 到当前为止已写字节的内容摘要（hex 小写）。写完 finalize 前 client
    /// 可用同一函数自行预检。
    pub fn content_sha256(&self) -> String {
        to_hex(self.digest.clone().finalize())
    }
}

/// 受控上传服务。
pub struct UploadService {
    artifacts: Arc<ServerArtifacts>,
    audit: Arc<ServerAudit>,
    clock: Arc<dyn Clock>,
    limits: UploadLimits,
}

impl UploadService {
    /// 缺省限额（L1 字节闸 + 契约配额）。
    pub fn new(
        artifacts: Arc<ServerArtifacts>,
        audit: Arc<ServerAudit>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self::with_limits(artifacts, audit, clock, UploadLimits::default())
    }

    /// 注入限额（测试边界与部署覆盖；`max_chunk_bytes` 只允许 ≤ L1 物理闸
    /// ——更小是部署选择，更大没有意义：请求体根本到不了 handler）。
    pub fn with_limits(
        artifacts: Arc<ServerArtifacts>,
        audit: Arc<ServerAudit>,
        clock: Arc<dyn Clock>,
        limits: UploadLimits,
    ) -> Self {
        Self {
            artifacts,
            audit,
            clock,
            limits,
        }
    }

    /// ── 审计（`audit_event` / `audit_outcome` 见 impl 尾部的既有定义；
    /// 此处只补 `audit_denied` 拒绝路径）──

    fn audit_denied(&self, ctx: &RequestContext, action: &str, error_code: &str) {
        self.audit_outcome(ctx, action, AuditOutcome::Denied, Some(error_code));
    }

    /// 开启一次上传：ACL 双重检查 → `ArtifactRepository::create`。
    pub async fn begin_upload(
        &self,
        ctx: &RequestContext,
        spec: ArtifactSpec,
    ) -> Result<UploadSession, ApiError> {
        check_upload_acl(ctx, &spec)?;
        let (artifact_id, writer) = self
            .artifacts
            .create(ctx, spec.clone())
            .await
            .map_err(|e| super::submission::port_error_to_api_error(&e))?;
        self.audit_outcome(ctx, "artifact_upload_started", AuditOutcome::Succeeded, None);
        Ok(UploadSession {
            spec,
            artifact_id,
            writer,
            published_bytes: 0,
            digest: Sha256::new(),
        })
    }

    /// 追加一块：大小闸 → 摘要更新 → `append_chunk`（仓储侧顺序分配 index，
    /// 已发布块不可覆盖）。
    pub async fn append_upload_chunk(
        &self,
        ctx: &RequestContext,
        session: &mut UploadSession,
        bytes: Vec<u8>,
    ) -> Result<ChunkIndex, ApiError> {
        if bytes.len() > self.limits.max_chunk_bytes {
            self.audit_denied(ctx, "artifact_chunk_rejected", "payloadTooLarge");
            return Err(ApiError::new(
                ApiErrorCode::PayloadTooLarge,
                "Upload chunk exceeds the per-chunk limit (aligned with the L1 body gate).",
            ));
        }
        let incoming = bytes.len() as u64;
        if session.published_bytes + incoming > self.limits.max_total_bytes {
            // 消费超限：显式 abort（保留已发布块、标记 truncated），writer
            // 句柄随之作废——超限不是可恢复状态。
            let stale = dead_writer(&session.writer);
            let writer = std::mem::replace(&mut session.writer, stale);
            let _ = self.artifacts.abort(writer, AbortReason::ConsumerLimitExceeded).await;
            self.audit_denied(ctx, "artifact_chunk_rejected", "quotaExceeded");
            return Err(ApiError::new(
                ApiErrorCode::QuotaExceeded,
                "Upload exceeds the artifact byte quota.",
            ));
        }
        session.digest.update(&bytes);
        match self.artifacts.append_chunk(&mut session.writer, ChunkPayload::new(bytes)).await {
            Ok(index) => {
                session.published_bytes += incoming;
                Ok(index)
            }
            Err(source) => Err(audit_mapped(self, ctx, "artifact_chunk_rejected", &source)),
        }
    }

    /// 完成：SHA-256 比对 → `finalize`。
    ///
    /// `expected_sha256` 为 `None` 时不做比对（调用方未声明摘要的场景，如
    /// 内部数据面），声明了就必须一致——篡改 / 断流一律 `InvalidArgument`，
    /// 产物 `abort`（ExecutionFailed：声明与事实不符的写入按失败执行处置）。
    pub async fn finalize_upload(
        &self,
        ctx: &RequestContext,
        session: UploadSession,
        expected_sha256: Option<&str>,
    ) -> Result<ArtifactId, ApiError> {
        let actual = session.content_sha256();
        if let Some(expected) = expected_sha256 {
            if !actual.eq_ignore_ascii_case(expected.trim()) {
                let _ = self
                    .artifacts
                    .abort(session.writer, AbortReason::ExecutionFailed)
                    .await;
                self.audit_outcome(
                    ctx,
                    "artifact_upload_hash_mismatch",
                    AuditOutcome::Failed,
                    Some(ApiErrorCode::InvalidArgument.as_str()),
                );
                return Err(ApiError::new(
                    ApiErrorCode::InvalidArgument,
                    "Content hash mismatch: uploaded bytes do not match the declared digest.",
                ));
            }
        }
        match self.artifacts.finalize(session.writer).await {
            Ok(artifact_id) => {
                self.audit_outcome(ctx, "artifact_uploaded", AuditOutcome::Succeeded, None);
                Ok(artifact_id)
            }
            Err(source) => Err(audit_mapped(self, ctx, "artifact_upload_aborted", &source)),
        }
    }

    /// 显式中止（取消 / 失败 / 上层消费超限的直通道）：保留已发布块并标记
    /// truncated，与 finalize 的分岔语义同端口契约。
    pub async fn abort_upload(
        &self,
        ctx: &RequestContext,
        session: UploadSession,
        reason: AbortReason,
    ) -> Result<ArtifactId, ApiError> {
        match self.artifacts.abort(session.writer, reason).await {
            Ok(artifact_id) => {
                self.audit_outcome(ctx, "artifact_upload_aborted", AuditOutcome::Succeeded, None);
                Ok(artifact_id)
            }
            Err(source) => Err(audit_mapped(self, ctx, "artifact_upload_aborted", &source)),
        }
    }

    fn audit_event(
        &self,
        ctx: &RequestContext,
        action: &str,
        outcome: AuditOutcome,
        error_code: Option<&str>,
    ) -> AuditEvent {
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
                self.clock.now(),
            )
        }
    }

    fn audit_outcome(
        &self,
        ctx: &RequestContext,
        action: &str,
        outcome: AuditOutcome,
        error_code: Option<&str>,
    ) {
        let event = self.audit_event(ctx, action, outcome, error_code);
        if let Err(source) = self.audit.record(event) {
            tracing::warn!(error = %source, "transfer audit append failed");
        }
    }
}

/// 上传 ACL：组织 + 资源（owner）双重归属检查。
pub fn check_upload_acl(ctx: &RequestContext, spec: &ArtifactSpec) -> Result<(), ApiError> {
    // 产物 create 绑定个人 owner；服务端形态没有委托注册通道（与
    // `ServerArtifacts` 的 create 裁决一致，这里先拒给更干净的 wire 错误）。
    if ctx.is_delegated()
        || spec.organization_id != ctx.organization_id
        || spec.owner_principal_id != ctx.principal_id
    {
        return Err(ApiError::new(
            ApiErrorCode::PermissionDenied,
            "Artifact ownership must match the authenticated organization and principal.",
        ));
    }
    Ok(())
}

/// 客户端标识符进入仓储前的白名单：UUID 形态字符集 + 长度 + 显式拒绝
/// 路径分量。**零容忍**：`..`、分隔符、NUL、空白一律 `InvalidArgument`。
pub fn ensure_artifact_id_is_safe(raw: &str) -> Result<ArtifactId, ApiError> {
    let trimmed = raw.trim();
    let ok_shape = !trimmed.is_empty()
        && trimmed.len() <= 128
        && !trimmed.contains("..")
        && !trimmed.contains('/')
        && !trimmed.contains('\\')
        && !trimmed.contains('\0')
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if !ok_shape {
        return Err(ApiError::new(
            ApiErrorCode::InvalidArgument,
            "Artifact id must be a bare identifier (no path separators or traversal).",
        ));
    }
    Ok(ArtifactId::new(trimmed))
}

/// 审计 + 映射的便捷封装（`audit_mapped(self, …)` 形态避免闭包借用冲突）。
fn audit_mapped(service: &UploadService, ctx: &RequestContext, action: &str, source: &datazen::shared::error::PortError) -> ApiError {
    let api = crate::transfer::submission::port_error_to_api_error(source);
    service.audit_outcome(ctx, action, AuditOutcome::Failed, Some(api.code.as_str()));
    api
}

/// 超限 abort 后给 `session.writer` 留一个不指任何活的占位（句柄已被仓储
/// 消费；再使用会被仓储按 fencing 拒绝——占位本身不该被继续依赖）。
fn dead_writer(previous: &ArtifactWriter) -> ArtifactWriter {
    ArtifactWriter {
        artifact_id: previous.artifact_id.clone(),
        lease_id: previous.lease_id.clone(),
        next_chunk_index: previous.next_chunk_index,
        published_chunk_count: previous.published_chunk_count,
        published_byte_size: previous.published_byte_size,
    }
}

/// hex 小写（不引 hex crate：摘要只有 32 字节，手写循环是最小的表面积）。
fn to_hex(bytes: impl AsRef<[u8]>) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.as_ref().len() * 2);
    for byte in bytes.as_ref() {
        let _ = write!(out, "{byte:02x}");
    }
    out
}
