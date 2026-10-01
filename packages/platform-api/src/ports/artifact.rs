//! `ArtifactStore`：结果落盘端口。
//!
//! 词汇表（§4.6）：`ArtifactSpec`、`ArtifactWriter`、`ChunkPayload`、`ChunkIndex`、
//! `ByteRange`、`ExportSink`、`ExportReceipt`、`RevokeReason`。
//!
//! 契约要点：
//!
//! * `create` 时绑定组织/owner 的 ACL；`finalize` 之前 artifact **不可枚举、不可导出**。
//! * `export` 的 sink 是**受控导出目标**（对话框返回的句柄、服务端授权的存储路径），
//!   **不接受任意服务端绝对路径**。
//! * TTL 过期后读取返回 `PortError::ArtifactExpired`，**不得用缓存副本继续服务**。
//! * HTTP 状态码映射不是端口契约（§4.6 尾注）：`ArtifactExpired` → 404 由 server host 决定。

use async_trait::async_trait;

use crate::context::RequestContext;
use crate::dto::artifact::ArtifactChunk;
use crate::dto::execution::ResultProvenance;
use crate::error::PortError;
use crate::id::{
    ArtifactId, Counter, ExecutionId, LeaseId, OrganizationId, PrincipalId, Timestamp,
};

/// 一次结果落盘的规格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactSpec {
    pub organization_id: OrganizationId,
    pub owner_principal_id: PrincipalId,
    /// 产生该结果集的执行。
    pub execution_id: ExecutionId,
    /// 稳定目标指纹：结果归属按目标而非按会话，切会话不改变归属。
    pub target_fingerprint: String,
    /// TTL（秒）。过期后读取返回 `ArtifactExpired`。
    pub ttl_seconds: u64,
}

impl ArtifactSpec {
    pub fn new(
        organization_id: OrganizationId,
        owner_principal_id: PrincipalId,
        execution_id: ExecutionId,
        target_fingerprint: String,
    ) -> Self {
        Self {
            organization_id,
            owner_principal_id,
            execution_id,
            target_fingerprint,
            ttl_seconds: 86_400,
        }
    }
}

/// 写入句柄。**持有者必须 finalize**；丢弃即视为未完成落盘。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactWriter {
    pub artifact_id: ArtifactId,
    pub lease_id: LeaseId,
    pub next_chunk_index: Counter,
}

/// 一块待写数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkPayload {
    pub bytes: Vec<u8>,
    /// 未知时留 `None`，不得用 0 冒充。
    pub total_chunks: Option<Counter>,
}

impl ChunkPayload {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            total_chunks: None,
        }
    }
}

/// 块索引。序列化仍是十进制字符串（JS 精度）。
pub type ChunkIndex = Counter;

/// 字节区间。闭开区间 `[start, end)`；`None` 表示到末尾。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub start: u64,
    pub end: Option<u64>,
}

impl ByteRange {
    pub fn from(start: u64) -> Self {
        Self { start, end: None }
    }

    pub fn between(start: u64, end: u64) -> Self {
        Self {
            start,
            end: Some(end),
        }
    }
}

/// 受控导出目标。**不接受任意服务端绝对路径**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportSink {
    /// 桌面：保存对话框返回的句柄。
    DesktopDialogHandle(LeaseId),
    /// server：服务端授权的存储路径（由 host 生成，不来自请求体）。
    ServerAuthorizedPath(OrganizationId, String),
}

/// 导出回执。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportReceipt {
    pub artifact_id: ArtifactId,
    pub byte_length: u64,
    pub exported_at: Timestamp,
}

/// 吊销原因。吊销后立即不可读，与 TTL 到期区分开以便审计。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevokeReason {
    /// 所属执行已被撤销。
    ExecutionRevoked,
    /// 所属 Job 被删除。
    JobDeleted,
    /// 策略判定不再允许访问。
    PolicyDenied,
    /// 组织被停用。
    OrganizationSuspended,
}

#[async_trait]
pub trait ArtifactStore: Send + Sync + 'static {
    /// create writer：返回可写 artifact 与 writer 句柄；ACL 在 create 时绑定组织/owner。
    async fn create(
        &self,
        ctx: &RequestContext,
        spec: ArtifactSpec,
    ) -> Result<(ArtifactId, ArtifactWriter), PortError>;

    async fn append_chunk(
        &self,
        writer: &mut ArtifactWriter,
        chunk: ChunkPayload,
    ) -> Result<ChunkIndex, PortError>;

    /// finalize 后才可读；未 finalize 的 artifact 不可枚举也不可导出。
    async fn finalize(&self, writer: ArtifactWriter) -> Result<ArtifactId, PortError>;

    async fn read_chunk(
        &self,
        ctx: &RequestContext,
        artifact_id: ArtifactId,
        chunk_index: ChunkIndex,
    ) -> Result<ArtifactChunk, PortError>;

    async fn read_range(
        &self,
        ctx: &RequestContext,
        artifact_id: ArtifactId,
        range: ByteRange,
    ) -> Result<ArtifactChunk, PortError>;

    /// sink 是受控导出目标（对话框返回的句柄、服务端授权存储路径）；
    /// **不接受任意服务器绝对路径**。
    async fn export(
        &self,
        ctx: &RequestContext,
        artifact_id: ArtifactId,
        sink: ExportSink,
    ) -> Result<ExportReceipt, PortError>;

    /// TTL 删除：过期后读取返回 `ArtifactExpired`，不得以缓存副本继续服务。
    async fn delete_expired(&self, now: Timestamp) -> Result<Vec<ArtifactId>, PortError>;

    async fn revoke(
        &self,
        ctx: &RequestContext,
        artifact_id: ArtifactId,
        reason: RevokeReason,
    ) -> Result<(), PortError>;
}

/// 便捷：结果归属指纹取自 provenance 的稳定目标，而不是会话（[连接 §12](connection-management.md)）。
/// 只用**规范化**标识；展示名、用户输入一律不进指纹。
pub fn provenance_fingerprint(provenance: &ResultProvenance) -> String {
    format!(
        "{}/{}",
        provenance.requested_target.connection_id.as_str(),
        provenance.requested_target.namespace_fingerprint()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{ConnectionId, StreamId};
    use crate::target::{CanonicalNamespace, CanonicalTarget};

    fn artifact_spec() -> ArtifactSpec {
        ArtifactSpec::new(
            OrganizationId::new("org-1"),
            PrincipalId::new("user-1"),
            ExecutionId::new("exec-1"),
            "conn-1/public/t/public.t".to_string(),
        )
    }

    #[test]
    fn spec_binds_the_acl_at_create_time() {
        let spec = artifact_spec();
        assert_eq!(spec.organization_id, OrganizationId::new("org-1"));
        assert_eq!(spec.owner_principal_id, PrincipalId::new("user-1"));
        assert_eq!(spec.execution_id, ExecutionId::new("exec-1"));
        // TTL 是配置，不是硬编码在端口里的语义。
        assert_eq!(spec.ttl_seconds, 86_400);
        let tuned = ArtifactSpec {
            ttl_seconds: 60,
            ..artifact_spec()
        };
        assert_eq!(tuned.ttl_seconds, 60);
    }

    #[test]
    fn chunk_index_is_a_decimal_string_counter_not_a_raw_usize() {
        // JS 侧读到的必须是字符串，避免 >2^53 精度丢失。
        let index: ChunkIndex = Counter::new(9007199254740993);
        let json = serde_json::to_string(&index).expect("serialize");
        assert_eq!(json, "\"9007199254740993\"");
        assert_eq!(
            serde_json::from_str::<ChunkIndex>(&json).expect("deserialize"),
            index
        );
    }

    #[test]
    fn unknown_total_chunks_stays_none_and_is_never_zero() {
        assert_eq!(ChunkPayload::new(vec![1, 2, 3]).total_chunks, None);
        let known = ChunkPayload {
            total_chunks: Some(Counter::new(2)),
            ..ChunkPayload::new(vec![1])
        };
        assert_eq!(known.total_chunks, Some(Counter::new(2)));
    }

    #[test]
    fn export_sink_has_no_variant_for_an_arbitrary_server_path() {
        // 两个变体都由 host 侧产生句柄或授权路径；请求体无法直接塞进任意绝对路径。
        let desktop = ExportSink::DesktopDialogHandle(LeaseId::new("dialog-1"));
        let server = ExportSink::ServerAuthorizedPath(
            OrganizationId::new("org-1"),
            "/data/exports/x.parquet".into(),
        );
        assert_ne!(desktop, server);
        let source = include_str!("artifact.rs");
        let sink_block = &source[source.find("pub enum ExportSink").unwrap()..];
        let sink_block = &sink_block[..sink_block.find("\n}").unwrap()];
        assert!(!sink_block.contains("RequestPath"));
        assert!(!sink_block.contains("UserSuppliedPath"));
    }

    #[test]
    fn revoke_reasons_stay_distinguishable_for_audit() {
        let reasons = [
            RevokeReason::ExecutionRevoked,
            RevokeReason::JobDeleted,
            RevokeReason::PolicyDenied,
            RevokeReason::OrganizationSuspended,
        ];
        let mut unique = reasons.to_vec();
        unique.sort_by_key(|r| format!("{r:?}"));
        unique.dedup();
        assert_eq!(unique.len(), reasons.len());
    }

    #[test]
    fn byte_range_supports_open_ended_reads() {
        assert_eq!(
            ByteRange::from(64),
            ByteRange {
                start: 64,
                end: None
            }
        );
        assert_eq!(
            ByteRange::between(0, 16),
            ByteRange {
                start: 0,
                end: Some(16)
            }
        );
    }

    #[test]
    fn artifact_chunk_is_the_type_the_store_returns() {
        // `ArtifactChunk` 来自 dto 层（跨宿主契约），端口不得另造一份块结构。
        let chunk = ArtifactChunk {
            artifact_id: ArtifactId::new("art-1"),
            chunk_index: Counter::new(0),
            total_chunks: None,
            offset: Counter::ZERO,
            bytes: vec![7, 8],
            result_completeness: crate::dto::execution::ResultCompleteness::Complete,
        };
        assert_eq!(chunk.end_offset(), Counter::new(2));
    }

    #[test]
    fn writer_tracks_the_next_chunk_index_so_indices_are_dense() {
        let writer = ArtifactWriter {
            artifact_id: ArtifactId::new("art-1"),
            lease_id: LeaseId::new("lease-1"),
            next_chunk_index: Counter::new(4),
        };
        assert_eq!(writer.next_chunk_index.get(), 4);
        // writer 只管字节与索引；流归属由事件信封的 streamId 承担。
        let source = include_str!("artifact.rs");
        let writer_block = &source[source.find("pub struct ArtifactWriter").unwrap()..];
        let writer_block = &writer_block[..writer_block.find("\n}").unwrap()];
        assert!(!writer_block.contains("stream_id"));
        assert_eq!(StreamId::new("stream-1").as_str(), "stream-1");
    }

    #[test]
    fn canonical_namespace_fingerprint_distinguishes_public_from_nothing() {
        // 「未指定」与「public」是两回事，指纹必须区分得开。
        let public = CanonicalTarget::new(
            ConnectionId::new("conn-1"),
            CanonicalNamespace::public(),
            None,
        );
        let unspecified = CanonicalTarget::new(
            ConnectionId::new("conn-1"),
            CanonicalNamespace::default(),
            None,
        );
        assert_eq!(public.namespace_fingerprint(), "//public");
        assert_ne!(
            public.namespace_fingerprint(),
            unspecified.namespace_fingerprint()
        );
        assert_eq!(unspecified.namespace_fingerprint(), "//");
        assert!(public.namespace.is_public());
        assert!(!unspecified.namespace.is_public());
    }
}
