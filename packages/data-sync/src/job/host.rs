//! DataSyncHost：领域 handler 与桌面宿主之间的端口。
//!
//! handler 需要会话/条款/工件存储访问，但 packages/data-sync 不得依赖
//! src-tauri。本 trait 由 src-tauri 的 `commands/sync/host.rs` 实现；
//! 测试由内存 fake 实现（见 handler 单测）。

use std::sync::Arc;

use async_trait::async_trait;
use datazen_driver_api::{ConnectionHandle, DatabaseDriver, SyncKeyValue, TableSchema, Value};

use crate::error::DataSyncError;
use crate::filter::SyncSourceFilter;
use crate::model::{Endpoint, Row, SyncOptions, TableMapping};
use crate::sql::SqlStatement;

use super::artifact::{ChangeBlock, ChangeSetArtifact, RelationIdentity};

/// 一次端点会话：driver 句柄 + 物理会话。释放语义由 host 决定
///（dedicated 才真正释放；共享会话仅解除订阅，复用桌面约定）。
pub struct EndpointSession {
    pub driver: Arc<dyn DatabaseDriver>,
    pub handle: ConnectionHandle,
    pub family: String,
    pub database: String,
    pub schema: Option<String>,
}

/// 动态分发的 keyset 行读取器（连接 RowPageSource 到 host 提供的实现）。
#[async_trait]
pub trait KeysetPageSource: Send {
    async fn next_page(
        &mut self,
        after_key: Option<&[Value]>,
        limit: u32,
    ) -> Result<Vec<Row>, DataSyncError>;
    fn normalize_key(&self, key: &[Value]) -> Result<Vec<SyncKeyValue>, DataSyncError>;
}

/// 固定目标 Lease 的批次执行器。每批 begin/read_by_key/execute/commit/rollback
/// 都在同一会话上；失败清理失败时 discard。
#[async_trait]
pub trait TargetExecutor: Send {
    async fn begin(&mut self) -> Result<(), DataSyncError>;
    /// 按类型化 PK 读取目标行（before 证据核验/INSERT 键不存在重验）。
    async fn read_by_key(
        &mut self,
        relation: &RelationIdentity,
        key: &[Value],
    ) -> Result<Option<Row>, DataSyncError>;
    /// 执行一条参数化写入语句；返回受影响行数（用于 TargetConflictRows 校验）。
    async fn execute(&mut self, statement: &SqlStatement) -> Result<u64, DataSyncError>;
    async fn commit(&mut self) -> Result<(), DataSyncError>;
    async fn rollback(&mut self) -> Result<(), DataSyncError>;
    async fn discard(&mut self) -> Result<(), DataSyncError>;
}

/// 目标证明的事务范围。不支持批次事务则 apply 拒绝。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionScope {
    Batch,
    Table,
    Task,
    NonAtomic,
}

impl TransactionScope {
    pub fn supports_batch_atomicity(&self) -> bool {
        matches!(self, Self::Batch | Self::Table | Self::Task)
    }
}

/// DataSyncHost 端口：prepare/apply 所需的资源与工件访问。
#[async_trait]
pub trait DataSyncHost: Send + Sync {
    async fn open_endpoint(&self, endpoint: &Endpoint) -> Result<EndpointSession, DataSyncError>;
    async fn close_endpoint(&self, session: EndpointSession);
    /// 读取参与表的 source/target 两个结构快照；prepare 产出指纹，apply 重验。
    async fn mapped_table_schemas(
        &self,
        source: &EndpointSession,
        target: &EndpointSession,
        mappings: &[TableMapping],
    ) -> Result<Vec<TableSchemaPair>, DataSyncError>;
    /// 每表一个 keyset 行读取器（源或目标侧）。
    async fn table_reader(
        &self,
        session: &EndpointSession,
        table: &str,
        filter: Option<&SyncSourceFilter>,
    ) -> Result<Box<dyn KeysetPageSource>, DataSyncError>;
    /// 存储 prepare 产出的不可变 ChangeSet 工件。
    async fn store_artifact(
        &self,
        artifact: ChangeSetArtifact,
    ) -> Result<ArtifactStoreGuard, DataSyncError>;
    /// apply 加载工件并核验 plan 未过期。
    async fn load_artifact(
        &self,
        plan_id: &str,
    ) -> Result<Option<ChangeSetArtifact>, DataSyncError>;
    /// apply 时用户最终确认的块子集（必须是 ChangeSet 的子集）。
    async fn selection(
        &self,
        plan_id: &str,
        selection_revision: u64,
    ) -> Result<Vec<ChangeBlock>, DataSyncError>;
    /// 目标写权限预检。
    async fn check_permissions(&self, session: &EndpointSession) -> Result<(), DataSyncError>;
    /// 目标事务能力证明。
    async fn transaction_scope(
        &self,
        session: &EndpointSession,
    ) -> Result<TransactionScope, DataSyncError>;
    /// 开一个固定目标 Lease 的批次执行器（贯穿整个 apply）。
    async fn target_executor(
        &self,
        session: &EndpointSession,
    ) -> Result<Box<dyn TargetExecutor>, DataSyncError>;
    /// 生成参数化写入语句（方言特定，由 host 持有 sync adapters）。
    async fn generate_statements(
        &self,
        relation: &RelationIdentity,
        source_table: &str,
        target_table: &str,
        changes: Vec<crate::model::RowChange>,
        options: &SyncOptions,
    ) -> Result<Vec<SqlStatement>, DataSyncError>;
    /// ISO-8601 UTC 时间戳。
    fn now(&self) -> String;
}

/// 一条映射的两个结构快照。
pub struct TableSchemaPair {
    pub source_table: String,
    pub target_table: String,
    pub source: TableSchema,
    pub target: TableSchema,
}

/// store_artifact 的确认句（保留扩展位）。
pub struct ArtifactStoreGuard {
    pub artifact_id: String,
}
