//! 产物契约 DTO：结果块与产物摘要。
//!
//! 产物**内容**由 `ArtifactStore` 端口持有（`ChunkPayload`、`ByteRange` 等私有词汇），
//! 本模块只放客户端可见的 DTO 家族。
//!
//! **线缆编码**：`bytes` 是进程内的原始字节，serde 默认按字节数组编码。
//! base64 等线缆编码由 host adapter 负责，契约层不做传输决策（§4.4「传输无关」）。

use serde::{Deserialize, Serialize};

use crate::dto::execution::{ResultCompleteness, StatementResultSource};
use crate::id::{ArtifactId, Counter, ExecutionId, OrganizationId, Timestamp};

/// 结果块。`totalChunks` 在长度未知时为 `null`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactChunk {
    pub artifact_id: ArtifactId,
    pub chunk_index: Counter,
    pub total_chunks: Option<Counter>,
    pub offset: Counter,
    pub bytes: Vec<u8>,
    pub result_completeness: ResultCompleteness,
}

impl ArtifactChunk {
    pub fn end_offset(&self) -> Counter {
        self.offset.saturating_increment_by(self.bytes.len() as u64)
    }
}

/// 产物摘要。**不含**内容、也不含任何可还原凭据的材料。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactSummary {
    pub artifact_id: ArtifactId,
    pub organization_id: OrganizationId,
    pub execution_id: Option<ExecutionId>,
    pub size_bytes: Counter,
    pub chunk_count: Option<Counter>,
    pub result_completeness: ResultCompleteness,
    pub created_at: Timestamp,
    /// 过期时刻。过期后 `PortError::ArtifactExpired`，端口层取值，HTTP 映射由 host 负责。
    pub expires_at: Option<Timestamp>,
}

/// 产物生命周期。写入期与两个不可读终态（[共享边界 §4.6](../shared-boundaries-and-ports.md#46-结果与事件端口)）。
///
/// `Writing` 的产物**已发布块可读**：读取权随发布走，不随终结走。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArtifactLifecycle {
    /// 写入中。`totalChunks` 尚不确定，但仍可读已发布块。
    Writing,
    /// 正常终结。
    Complete,
    /// 显式中止或进程异常后由恢复扫描标记；已发布块保留。
    Truncated,
    /// 吊销，不可读。
    Revoked,
    /// TTL 到期，不可读。
    Expired,
}

impl ArtifactLifecycle {
    /// 不可读终态。`revoked` 与 `expired` 都不可读，但二者语义分开以便审计。
    pub fn is_unreadable(self) -> bool {
        matches!(self, Self::Revoked | Self::Expired)
    }

    /// 是否已终结：终结后不再接受 `append_chunk`。
    pub fn is_finalized(self) -> bool {
        !matches!(self, Self::Writing)
    }

    /// 常规受控导出是否放行。`truncated` 不在此列——
    /// 它需要用户明确确认（§4.6），由 host 决定，不进端口默认路径。
    pub fn allows_plain_export(self) -> bool {
        matches!(self, Self::Complete)
    }
}

/// 截断/中止原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AbortReason {
    /// 进程异常退出，由恢复扫描标记；**不得**据此宣称结果完整。
    WriterLost,
    /// 所属执行失败。
    ExecutionFailed,
    /// 执行被取消。
    Cancelled,
    /// 消费超限。
    ConsumerLimitExceeded,
}

/// 产物元数据。客户端每次查询都先授权（§4.6）。
///
/// 这是 **P3 目标契约**，当前无任何实现；不得当作已实现行为。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactMetadata {
    pub artifact_id: ArtifactId,
    pub lifecycle: ArtifactLifecycle,
    /// 已发布（提交后）的块数。写入期只增不减。
    pub published_chunk_count: Counter,
    /// 已发布字节数。写入期只增不减。
    pub published_byte_size: Counter,
    /// 总量未知时为 `null`；`null` **只**表示尚未终结，不禁止读取已发布块。
    pub total_chunks: Option<Counter>,
    pub result_completeness: ResultCompleteness,
    /// 仅 `truncated` 时有值。
    pub truncation_reason: Option<AbortReason>,
}

impl ArtifactMetadata {
    /// 已发布块数——`writing` 期间这就是可读块的上界。
    pub fn published_chunk_count(&self) -> Counter {
        self.published_chunk_count
    }

    /// `chunk_index` 是否已发布。越界与未发布**都必须**由调用方当参数错误处理，
    /// 不能当作空块（§4.6）。
    pub fn is_published(&self, chunk_index: Counter) -> bool {
        chunk_index.get() < self.published_chunk_count.get()
    }
}

/// 产物与其来源的绑定：客户端据此把结果块还原到正确的语句/上下文上。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactProvenance {
    pub artifact_id: ArtifactId,
    pub execution_id: ExecutionId,
    pub statement_index: u32,
    pub source: StatementResultSource,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::session::{ContextConfidence, SessionContext, TransactionState};
    use crate::id::ConnectionId;
    use crate::target::{ExecutionTarget, NamespaceTarget};
    use serde_json::json;

    fn session_context() -> SessionContext {
        SessionContext {
            namespace: NamespaceTarget::database("app"),
            search_path: None,
            effective_identity: None,
            transaction_state: TransactionState::None,
            autocommit: Some(true),
            confidence: ContextConfidence::Confirmed,
        }
    }

    fn chunk() -> ArtifactChunk {
        ArtifactChunk {
            artifact_id: ArtifactId::new("art-1"),
            chunk_index: Counter::new(0),
            total_chunks: Some(Counter::new(2)),
            offset: Counter::ZERO,
            bytes: vec![0x41, 0x42, 0x43],
            result_completeness: ResultCompleteness::Complete,
        }
    }

    #[test]
    fn artifact_chunk_round_trips_with_decimal_string_indices() {
        let original = chunk();
        let value = serde_json::to_value(&original).expect("serialize");
        assert_eq!(value["chunkIndex"], json!("0"));
        assert_eq!(value["totalChunks"], json!("2"));
        assert_eq!(value["offset"], json!("0"));
        assert_eq!(value["bytes"], json!([65, 66, 67]));
        assert_eq!(value["resultCompleteness"], json!("complete"));
        assert_eq!(
            serde_json::from_value::<ArtifactChunk>(value).expect("deserialize"),
            original
        );
    }

    #[test]
    fn unknown_total_chunks_stays_null() {
        let mut streaming = chunk();
        streaming.total_chunks = None;
        let value = serde_json::to_value(&streaming).expect("serialize");
        assert_eq!(value["totalChunks"], json!(null));
        assert_eq!(
            serde_json::from_value::<ArtifactChunk>(value).expect("deserialize"),
            streaming
        );
    }

    #[test]
    fn end_offset_saturates_instead_of_overflowing() {
        let at_max = ArtifactChunk {
            offset: Counter::new(u64::MAX),
            bytes: vec![1, 2],
            ..chunk()
        };
        assert_eq!(at_max.end_offset(), Counter::new(u64::MAX));
        assert_eq!(chunk().end_offset(), Counter::new(3));
    }

    #[test]
    fn artifact_summary_round_trips() {
        let summary = ArtifactSummary {
            artifact_id: ArtifactId::new("art-1"),
            organization_id: OrganizationId::new("org-1"),
            execution_id: Some(ExecutionId::new("exec-1")),
            size_bytes: Counter::new(1024),
            chunk_count: None,
            result_completeness: ResultCompleteness::Truncated,
            created_at: Timestamp::new("2026-01-01T00:00:00Z"),
            expires_at: None,
        };
        let value = serde_json::to_value(&summary).expect("serialize");
        assert_eq!(value["sizeBytes"], json!("1024"));
        assert_eq!(value["resultCompleteness"], json!("truncated"));
        assert_eq!(
            serde_json::from_value::<ArtifactSummary>(value).expect("deserialize"),
            summary
        );
    }

    #[test]
    fn artifact_provenance_keeps_the_statement_source() {
        let provenance = ArtifactProvenance {
            artifact_id: ArtifactId::new("art-1"),
            execution_id: ExecutionId::new("exec-1"),
            statement_index: 0,
            source: StatementResultSource {
                execution_id: ExecutionId::new("exec-1"),
                statement_index: 0,
                context: session_context(),
                relation: Some(ExecutionTarget::new(
                    ConnectionId::new("conn-1"),
                    NamespaceTarget::database("app"),
                )),
                writable_mapping: crate::dto::execution::WritableMapping::Verified,
            },
        };
        let value = serde_json::to_value(&provenance).expect("serialize");
        assert_eq!(value["source"]["writableMapping"], json!("verified"));
        assert_eq!(
            serde_json::from_value::<ArtifactProvenance>(value).expect("deserialize"),
            provenance
        );
    }
}
