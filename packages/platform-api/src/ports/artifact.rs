//! `ArtifactStore`：结果落盘端口。
//!
//! 词汇表（§4.6）：`ArtifactSpec`、`ArtifactWriter`、`ChunkPayload`、`ChunkIndex`、
//! `ByteRange`、`ExportSink`、`ExportReceipt`、`RevokeReason`。
//!
//! 契约要点：
//!
//! * `create` 时绑定组织/owner 的 ACL。
//! * `export` 的 sink 是**受控导出目标**（对话框返回的句柄、服务端授权的存储路径），
//!   **不接受任意服务端绝对路径**。
//! * TTL 过期后读取返回 `PortError::ArtifactExpired`，**不得用缓存副本继续服务**。
//! * HTTP 状态码映射不是端口契约（§4.6 尾注）：`ArtifactExpired` → 404 由 server host 决定。
//!
//! ## P3 目标契约（[共享边界 §4.6](../shared-boundaries-and-ports.md#46-结果与事件端口)）
//!
//! **本节描述尚未实现的目标行为，当前无任何 `ArtifactStore` 实现。**
//!
//! * 生命周期 `writing → complete | truncated`，另有 `revoked/expired` 不可读终态。
//! * **`append_chunk` 顺序分配 index；字节与块元数据提交后才返回并发布 `resultChunk`。
//!   已发布块不可覆盖。**
//! * **写入期已发布块可读**：`read_chunk` 只读取已发布 index，`read_range` 只读已发布
//!   连续前缀，**不等待未来字节**。
//! * 未发布索引/越界返回参数错误，客户端据事件重读，**不把它当空块**。
//! * `totalChunks=null` 只表示尚未终结，**不禁止**读取已发布块。
//! * 正常完成走 `finalize`；取消/失败/消费超限走显式 `abort`（保留已发布块并标记
//!   truncated）。进程异常退出的 writer 由恢复扫描标记 truncated，**不宣称完整**。
//! * 常规受控导出仅允许 `complete`；`truncated` 需用户明确确认并保留截断标记。

use async_trait::async_trait;

use crate::context::RequestContext;
use crate::dto::artifact::{AbortReason, ArtifactChunk, ArtifactMetadata};
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

/// 写入句柄。**持有者必须 finalize 或 abort**；丢弃即视为未完成落盘，
/// 由恢复扫描标记 truncated（不宣称完整）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactWriter {
    pub artifact_id: ArtifactId,
    pub lease_id: LeaseId,
    /// 下一个将被分配的 index。已发布块的 index **不得**复用。
    pub next_chunk_index: Counter,
    /// 已提交（已发布）块数。只增不减。
    pub published_chunk_count: Counter,
    /// 已提交字节数。只增不减。
    pub published_byte_size: Counter,
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

    /// 顺序分配 index 追加一块；**字节与块元数据提交后才返回**并发布 `resultChunk`。
    /// 已发布块不可覆盖——复用已发布 index 必须是错误，不能静默改写查询结果。
    async fn append_chunk(
        &self,
        writer: &mut ArtifactWriter,
        chunk: ChunkPayload,
    ) -> Result<ChunkIndex, PortError>;

    /// 查询产物元数据。**写入期也可调用**：这是客户端判断「读到哪」的唯一来源。
    async fn describe(
        &self,
        ctx: &RequestContext,
        artifact_id: ArtifactId,
    ) -> Result<ArtifactMetadata, PortError>;

    /// finalize 固化块数、完整性与总量，之后才可枚举与常规导出。
    async fn finalize(&self, writer: ArtifactWriter) -> Result<ArtifactId, PortError>;

    /// 显式中止：**保留已发布块**并标记 truncated，用于取消/失败/消费超限。
    /// 正常完成走 [`ArtifactStore::finalize`]，二者不得互相顶替。
    async fn abort(
        &self,
        writer: ArtifactWriter,
        reason: AbortReason,
    ) -> Result<ArtifactId, PortError>;

    /// 只读取**已发布**的 `chunk_index`。未发布索引或越界是参数错误，不是空块。
    async fn read_chunk(
        &self,
        ctx: &RequestContext,
        artifact_id: ArtifactId,
        chunk_index: ChunkIndex,
    ) -> Result<ArtifactChunk, PortError>;

    /// 只读取**已发布连续前缀**的字节区间；**不等待未来字节**。
    async fn read_range(
        &self,
        ctx: &RequestContext,
        artifact_id: ArtifactId,
        range: ByteRange,
    ) -> Result<ArtifactChunk, PortError>;

    /// sink 是受控导出目标（对话框返回的句柄、服务端授权存储路径）；
    /// **不接受任意服务器绝对路径**。
    ///
    /// 常规路径仅允许 `complete`；`truncated` 需用户明确确认且必须保留截断标记
    /// （§4.6），`revoked/expired` 一律不可导出。
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
    use crate::dto::artifact::ArtifactLifecycle;
    use crate::dto::execution::ResultCompleteness;
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

    fn metadata() -> ArtifactMetadata {
        ArtifactMetadata {
            artifact_id: ArtifactId::new("art-1"),
            lifecycle: ArtifactLifecycle::Writing,
            published_chunk_count: Counter::new(2),
            published_byte_size: Counter::new(6),
            total_chunks: None,
            result_completeness: ResultCompleteness::Pending,
            truncation_reason: None,
        }
    }

    #[test]
    fn a_writing_artifact_still_publishes_readable_chunks() {
        // 「写入期不可读」是旧契约。新契约：读取权随发布走，不随终结走。
        let meta = metadata();
        assert_eq!(meta.lifecycle, ArtifactLifecycle::Writing);
        assert!(meta.is_published(Counter::ZERO));
        assert!(meta.is_published(Counter::new(1)));
        // 但未发布的 index 必须能被分辨出来——否则客户端只能当空块处理。
        assert!(!meta.is_published(Counter::new(2)));
    }

    #[test]
    fn an_unknown_total_never_blocks_reading_what_is_already_published() {
        // totalChunks=null 只表示尚未终结，不表示「还没开始」也不是「没有内容」。
        let meta = metadata();
        assert_eq!(meta.total_chunks, None);
        assert_eq!(meta.published_chunk_count(), Counter::new(2));
        assert_eq!(meta.published_byte_size, Counter::new(6));
        assert!(meta.is_published(Counter::new(1)));
    }

    #[test]
    fn an_aborted_artifact_keeps_its_published_chunks_and_records_why() {
        let meta = ArtifactMetadata {
            lifecycle: ArtifactLifecycle::Truncated,
            result_completeness: ResultCompleteness::Truncated,
            truncation_reason: Some(AbortReason::WriterLost),
            ..metadata()
        };
        // 保留已发布块：截断不是清空。
        assert!(meta.is_published(Counter::new(1)));
        assert_eq!(meta.truncation_reason, Some(AbortReason::WriterLost));
        // 但不宣称完整，也不走常规导出。
        assert!(!meta.lifecycle.allows_plain_export());
    }

    #[test]
    fn complete_is_the_only_lifecycle_with_a_plain_export_path() {
        assert!(ArtifactLifecycle::Complete.allows_plain_export());
        for state in [
            ArtifactLifecycle::Writing,
            ArtifactLifecycle::Truncated,
            ArtifactLifecycle::Revoked,
            ArtifactLifecycle::Expired,
        ] {
            assert!(
                !state.allows_plain_export(),
                "{state:?} must not take the plain export path"
            );
        }
    }

    #[test]
    fn revoked_and_expired_are_unreadable_but_mean_different_things() {
        assert!(ArtifactLifecycle::Revoked.is_unreadable());
        assert!(ArtifactLifecycle::Expired.is_unreadable());
        // 写入中与正常完成都还可读——可读性不是「终结」的同义词。
        assert!(!ArtifactLifecycle::Writing.is_unreadable());
        assert!(!ArtifactLifecycle::Truncated.is_unreadable());
        // 终态判定只关心「还在不在写」。
        assert!(!ArtifactLifecycle::Writing.is_finalized());
        assert!(ArtifactLifecycle::Truncated.is_finalized());
    }

    #[test]
    fn a_writer_never_reuses_an_index_it_already_published() {
        // 已发布块不可覆盖：next_chunk_index 只能前进。
        let mut writer = ArtifactWriter {
            artifact_id: ArtifactId::new("art-1"),
            lease_id: LeaseId::new("lease-1"),
            next_chunk_index: Counter::new(3),
            published_chunk_count: Counter::new(3),
            published_byte_size: Counter::new(9),
        };
        assert_eq!(writer.next_chunk_index, writer.published_chunk_count);
        writer.next_chunk_index = writer.next_chunk_index.saturating_increment();
        writer.published_chunk_count = writer.published_chunk_count.saturating_increment();
        writer.published_byte_size = writer.published_byte_size.saturating_increment_by(4);
        assert_eq!(writer.next_chunk_index, writer.published_chunk_count);
        assert!(writer.published_byte_size.get() > Counter::new(9).get());
    }

    #[test]
    fn artifact_metadata_serializes_counters_as_decimal_strings() {
        let json = serde_json::to_value(metadata()).expect("serialize");
        assert_eq!(json["artifactId"], "art-1");
        assert_eq!(json["publishedChunkCount"], "2");
        assert_eq!(json["publishedByteSize"], "6");
        // 未终结的 totalChunks 必须是 null，不能是 0。
        assert_eq!(json["totalChunks"], serde_json::Value::Null);
        assert_eq!(json["lifecycle"], "writing");
        assert_eq!(json["truncationReason"], serde_json::Value::Null);
    }

    #[test]
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
            published_chunk_count: Counter::new(4),
            published_byte_size: Counter::new(16),
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
