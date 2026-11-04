//! 受控下载（WR-D7 第四刀）：`readArtifact` 三取法 + 逐请求 ACL + 流式
//! 分块输出形状。
//!
//! 契约位置：http-api-contract §6.5
//! `GET /api/v1/artifacts/{id}?metadata=1 | ?chunkIndex= | ?offset=&limit=`
//! ——三种取法**互斥**（与前端
//! `packages/web/src/platform/web/httpRoutes.ts::readArtifactQuery` 逐字对齐；
//! 前端 `readArtifactDecode` 在原始字节响应上消费 `x-dz-*` 信封补充头，键名
//! 见 [`HEADER_TOTAL_CHUNKS`] 等——**这是 wire 契约的一部分**，handler 必须
//! 原样输出）。
//!
//! 判定顺序（每个请求独立走一遍，无跨请求状态）：
//!
//! 1. **标识符零容忍**：[`crate::transfer::upload::ensure_artifact_id_is_safe`]
//!    白名单校验（路径遍历、分隔符、NUL 全拒），然后才进仓储。
//! 2. **ACL（组织隔离）**：仓储的读路径按 `ctx.organization_id` 谓词过滤，
//!    越组织读取答「不存在」（CM-05，防 ID 枚举）；主体级授权由 L6 授权
//!    中间件按策略判定（`policy::PolicyService`），本层不重复实现。
//! 3. **生命周期可读性**：`revoked` / `expired` 不可读——统一答
//!    `NotFound`（team-server §11 的有意 404：不存在 / 无权 / 过期三者
//!    不可区分）。handler 直接经 `ApiErrorResponse` 落 404，无特殊分支。
//! 4. **取法解析**：[`ArtifactReadMode::from_query`] 强制三取法互斥——
//!    `metadata=1`、`chunkIndex`、`offset+limit` 同时出现即 `InvalidArgument`
//!    （与前端 `readArtifactQuery` 抛错的条件一致）。
//!
//! 输出形状：[`DownloadReply::into_axum_response`] 给出完整的 axum 响应
//! 描述（元数据 JSON 或「原始字节 + `x-dz-*` 头」），**不接线**——挂到
//! `GET /api/v1/artifacts/{id}` 归主线。流式分块输出：字节分支天然是一块
//! 一响应（`ArtifactChunk.bytes`）；handler 若要 `Body::from_stream` 化，
//! 以 [`DownloadReply::Bytes`] 的 `chunk.bytes` 为源分片即可，wire 形状不变
//! （体是原始字节，不是 JSON）。

use std::sync::Arc;

use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use datazen::execution::domain::artifact_repository::{ArtifactRepository, ByteRange};
use datazen::execution::domain::{ArtifactLifecycle, ArtifactMetadata, ResultCompleteness};
use datazen::shared::audit::{AuditEvent, AuditOutcome, AuditPrincipalKind, AuditSink};
use datazen::shared::clock::Clock;
use datazen::shared::context::RequestContext;
use datazen::shared::error::{ApiError, ApiErrorCode, PortError};
use datazen::shared::id::Counter;

use crate::storage::{ServerArtifacts, ServerAudit};
use crate::transfer::upload::ensure_artifact_id_is_safe;

/// 信封补充头（`httpRoutes.ts` 的 `ARTIFACT_HEADER_*` 键名，WR-D7 复核
/// 定形——前端按这些键读发布元数据）。
pub const HEADER_TOTAL_CHUNKS: &str = "x-dz-total-chunks";
pub const HEADER_PUBLISHED_CHUNKS: &str = "x-dz-published-chunk-count";
pub const HEADER_PUBLISHED_BYTES: &str = "x-dz-published-byte-length";
pub const HEADER_COMPLETENESS: &str = "x-dz-result-completeness";

/// `readArtifact` 的三种取法（§6.5 互斥三元组）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactReadMode {
    /// `?metadata=1` → 元数据 JSON。
    Metadata,
    /// `?chunkIndex=<n>` → 该已发布块的原始字节。
    Chunk { chunk_index: u64 },
    /// `?offset=<n>&limit=<n>` → 已发布连续前缀的字节区间。
    Range { offset: u64, limit: u64 },
}

impl ArtifactReadMode {
    /// 从查询参数解析（handler 把 axum 的 `Query` 拆成标量后传入）。
    /// 互斥判定是**契约行为**：三个分量里必须恰好选中一个，否则
    /// `InvalidArgument`（与前端抛错条件一致）。
    pub fn from_query(
        metadata: bool,
        chunk_index: Option<u64>,
        offset: Option<u64>,
        limit: Option<u64>,
    ) -> Result<Self, ApiError> {
        let mut selected = 0usize;
        if metadata {
            selected += 1;
        }
        if chunk_index.is_some() {
            selected += 1;
        }
        // 区间取法必须 offset 与 limit **成对**出现——offset 无 limit 不构成
        // 区间（与前端 readArtifactQuery 的抛错条件一致，测试钉住）。
        let range_selected = offset.is_some() && limit.is_some();
        if range_selected {
            selected += 1;
        }
        if selected != 1 {
            return Err(ApiError::new(
                ApiErrorCode::InvalidArgument,
                "readArtifact requires exactly one of metadata=1, chunkIndex, or offset+limit (§6.5).",
            ));
        }
        if let Some(chunk_index) = chunk_index {
            return Ok(Self::Chunk { chunk_index });
        }
        if let (Some(offset), Some(limit)) = (offset, limit) {
            if limit == 0 {
                return Err(ApiError::new(
                    ApiErrorCode::InvalidArgument,
                    "limit must be a positive byte count.",
                ));
            }
            return Ok(Self::Range { offset, limit });
        }
        // metadata=1 是唯一剩下的合法单选。
        Ok(Self::Metadata)
    }
}

/// 受控下载的响应描述：handler 只做 `into_axum_response()` 一件事。
#[derive(Debug, Clone, PartialEq)]
pub enum DownloadReply {
    /// 元数据模式：200 + JSON 信封（`ArtifactMetadata` camelCase 序列化）。
    Metadata(ArtifactMetadata),
    /// 字节模式：200 + 原始字节 + `x-dz-*` 信封补充头（与
    /// `readArtifactDecode` 的解码逻辑逐字段对齐；`totalChunks` 未终结时
    /// 头缺席——前端 `headerCounter` 的 null 分支）。
    Bytes {
        chunk: datazen::execution::domain::ArtifactChunk,
        /// 发布元数据（同请求的 describe 一次往返同时充当 ACL 与信封头来源）。
        published_chunk_count: Counter,
        published_byte_size: Counter,
    },
}

impl DownloadReply {
    /// axum 响应落地（本模块给出形状；路由接线归主线）。
    pub fn into_axum_response(self) -> Response {
        match self {
            Self::Metadata(metadata) => (StatusCode::OK, Json(metadata)).into_response(),
            Self::Bytes {
                chunk,
                published_chunk_count,
                published_byte_size,
            } => {
                let (mut parts, body) = (StatusCode::OK, chunk.bytes).into_response().into_parts();
                let headers = &mut parts.headers;
                if let Some(total) = chunk.total_chunks {
                    insert_header(headers, HEADER_TOTAL_CHUNKS, &total.get().to_string());
                }
                insert_header(
                    headers,
                    HEADER_PUBLISHED_CHUNKS,
                    &published_chunk_count.get().to_string(),
                );
                insert_header(
                    headers,
                    HEADER_PUBLISHED_BYTES,
                    &published_byte_size.get().to_string(),
                );
                insert_header(
                    headers,
                    HEADER_COMPLETENESS,
                    completeness_literal(chunk.result_completeness),
                );
                Response::from_parts(parts, body)
            }
        }
    }
}

fn insert_header(headers: &mut axum::http::HeaderMap, name: &'static str, value: &str) {
    let name = HeaderName::from_static(name);
    if let Ok(value) = HeaderValue::from_str(value) {
        headers.insert(name, value);
    }
}

/// 完整性字面量（前端 `headerCompleteness` 认 `pending` / `truncated` /
/// 其余按 `complete`）。
fn completeness_literal(completeness: ResultCompleteness) -> &'static str {
    match completeness {
        ResultCompleteness::Pending => "pending",
        ResultCompleteness::Complete => "complete",
        ResultCompleteness::Truncated => "truncated",
    }
}

/// 受控下载服务。
pub struct DownloadService {
    artifacts: Arc<ServerArtifacts>,
    audit: Arc<ServerAudit>,
    clock: Arc<dyn Clock>,
}

impl DownloadService {
    pub fn new(
        artifacts: Arc<ServerArtifacts>,
        audit: Arc<ServerAudit>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            artifacts,
            audit,
            clock,
        }
    }

    /// 统一读入口：元数据 / 块 / 区间三取法分流。
    pub async fn read(
        &self,
        ctx: &RequestContext,
        artifact_id: &str,
        mode: ArtifactReadMode,
    ) -> Result<DownloadReply, ApiError> {
        // 零容忍标识符校验：路径遍历活不过这一行。
        let artifact_id = ensure_artifact_id_is_safe(artifact_id)?;

        // describe 先行：一次往返同时承担 ACL（组织隔离由仓储谓词执行）、
        // 生命周期可读性判定与信封补充头的取值来源。
        let metadata = match self.artifacts.describe(ctx, artifact_id.clone()).await {
            Ok(metadata) => metadata,
            Err(PortError::ArtifactExpired) | Err(PortError::NotFound(_)) => {
                // 有意统一 404（team-server §11）：不存在 / 无权 / 过期不可区分。
                self.audit_denied(ctx, "artifact_download_rejected", "notFound");
                return Err(ApiError::new(
                    ApiErrorCode::NotFound,
                    "Artifact is not visible to this organization or has expired.",
                ));
            }
            Err(source) => return Err(self.audit_failure(ctx, "artifact_download_failed", source)),
        };
        if matches!(
            metadata.lifecycle,
            ArtifactLifecycle::Revoked | ArtifactLifecycle::Expired
        ) {
            self.audit_denied(ctx, "artifact_download_rejected", "notFound");
            return Err(ApiError::new(
                ApiErrorCode::NotFound,
                "Artifact is not visible to this organization or has expired.",
            ));
        }

        match mode {
            ArtifactReadMode::Metadata => Ok(DownloadReply::Metadata(metadata)),
            ArtifactReadMode::Chunk { chunk_index } => {
                match self
                    .artifacts
                    .read_chunk(ctx, artifact_id, Counter::new(chunk_index))
                    .await
                {
                    Ok(chunk) => Ok(DownloadReply::Bytes {
                        chunk,
                        published_chunk_count: metadata.published_chunk_count,
                        published_byte_size: metadata.published_byte_size,
                    }),
                    // 未发布索引 / 越界是参数错误（端口答 CasConflict），
                    // 不是空块——客户端据元数据重读。
                    Err(PortError::CasConflict { .. }) => Err(ApiError::new(
                        ApiErrorCode::InvalidArgument,
                        "Requested chunk is not published (client should re-read metadata).",
                    )),
                    Err(source) => Err(self.audit_failure(ctx, "artifact_download_failed", source)),
                }
            }
            ArtifactReadMode::Range { offset, limit } => {
                let end = offset.saturating_add(limit);
                match self
                    .artifacts
                    .read_range(ctx, artifact_id.clone(), ByteRange::between(offset, end))
                    .await
                {
                    Ok(chunk) => Ok(DownloadReply::Bytes {
                        chunk,
                        published_chunk_count: metadata.published_chunk_count,
                        published_byte_size: metadata.published_byte_size,
                    }),
                    Err(PortError::CasConflict { .. }) => Err(ApiError::new(
                        ApiErrorCode::InvalidArgument,
                        "Requested range is not a published prefix (client should re-read metadata).",
                    )),
                    Err(source) => Err(self.audit_failure(ctx, "artifact_download_failed", source)),
                }
            }
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

    fn audit_denied(&self, ctx: &RequestContext, action: &str, error_code: &str) {
        let event = self.audit_event(ctx, action, AuditOutcome::Denied, Some(error_code));
        if let Err(source) = self.audit.record(event) {
            tracing::warn!(error = %source, "transfer audit append failed");
        }
    }

    fn audit_failure(&self, ctx: &RequestContext, action: &str, source: PortError) -> ApiError {
        let api = crate::transfer::submission::port_error_to_api_error(&source);
        let event = self.audit_event(ctx, action, AuditOutcome::Failed, Some(api.code.as_str()));
        if let Err(audit_source) = self.audit.record(event) {
            tracing::warn!(error = %audit_source, "transfer audit append failed");
        }
        api
    }
}
