//! SchemaDiffJobBackend：handler 与物理资源之间的接缝。
//!
//! handler 不直接持有 driver/连接（§2.3：handler 不直接改 Job 状态、不自续租）。
//! 物理读取、部署与只读核验都经此 trait：宿主实现（`src-tauri/src/commands/schema_diff/job.rs`）
//! 用真实 driver session 执行，测试用假实现。

use async_trait::async_trait;

use datazen_runtime::job::CancelToken;

use crate::job::plan::{SchemaDiffFrozenPlan, SchemaDiffPlanError};
use crate::types::SchemaDiffDeployResult;

/// 准备请求：与既有 prepare_* IPC 相同的稳定输入，但由 handler 统一执行。
#[derive(Debug, Clone)]
pub enum PrepareRequest {
    /// 表级比对（prepare_schema_diff_plan）。
    Table {
        source_db_session_id: String,
        target_db_session_id: String,
        table_names: Vec<String>,
        target_table_names: Vec<String>,
        target_only_table_names: Vec<String>,
        source_schema: Option<String>,
        target_schema: Option<String>,
        allow_destructive: bool,
        include_indexes: Option<bool>,
        type_overrides: Vec<crate::types::ColumnTypeOverride>,
    },
    /// 统一对象（unified plan：表 + view/function/procedure/trigger/sequence/type）。
    Unified {
        source_db_session_id: String,
        target_db_session_id: String,
        table_names: Vec<String>,
        target_table_names: Vec<String>,
        target_only_table_names: Vec<String>,
        source_objects: Vec<datazen_driver_api::DatabaseObject>,
        target_objects: Vec<datazen_driver_api::DatabaseObject>,
        source_schema: Option<String>,
        target_schema: Option<String>,
        allow_destructive: bool,
        include_indexes: Option<bool>,
        type_overrides: Vec<crate::types::ColumnTypeOverride>,
    },
}

/// 准备阶段产出：渲染好的计划 + 冻结计划元数据（§2.2）。
#[derive(Debug)]
pub struct PreparedPlan {
    pub meta: SchemaDiffFrozenPlan,
    pub plan: crate::types::SchemaDiffPlan,
    pub body_digest: String,
}

/// 应用阶段输入：由 handler 从 FrozenPlan 投影 + PlanStore 解析。
#[derive(Debug, Clone)]
pub struct ApplyRequest {
    pub target_db_session_id: String,
    pub use_transaction: bool,
    pub require_rollback: bool,
    pub confirm_destructive: Option<String>,
    pub job_id: Option<String>,
    pub target_database: Option<String>,
    pub target_schema: Option<String>,
    /// 迁移配置档引用 `{id, revision}`（沿用既有 deployment history 记账语义）。
    pub profile: Option<(String, String)>,
}

/// 只读核验结论（§7：DDL 响应丢失 / commit 应答丢失 → 只读核验）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadOnlyVerdict {
    /// 目标结构唯一证明此 operation 已提交。
    UniqueProofCommitted,
    /// 目标结构唯一证明此 operation 未执行。
    UniqueProofNotExecuted,
    /// 无法唯一证明。
    Indeterminate,
}

/// Backend 接缝：准备（短读→比对→渲染）/ 执行前复验 / 部署 / 只读核验。
#[async_trait]
pub trait SchemaDiffJobBackend: Send + Sync {
    /// 短 Lease 读源/目标结构，比较、产出差异 + 风险 + 渲染计划（§4.1）。
    async fn prepare_plan(
        &self,
        request: &PrepareRequest,
        cancel: &CancelToken,
    ) -> Result<PreparedPlan, SchemaDiffPlanError>;

    /// 执行前：重新解析 capability/版本/credential 快照并比对（§2.2 抑制重复授权）。
    async fn verify_authorization(
        &self,
        plan_meta: &SchemaDiffFrozenPlan,
    ) -> Result<(), SchemaDiffPlanError>;

    /// 执行前：重读目标结构并与 before fingerprint 比对（§4.2 PlanStale）。
    async fn read_target_fingerprint(
        &self,
        plan_meta: &SchemaDiffFrozenPlan,
    ) -> Result<String, SchemaDiffPlanError>;

    /// 按 DAG 拓扑执行；transaction group 固定目标 Lease；返回 driver 实际终态。
    async fn deploy(
        &self,
        plan: &crate::types::SchemaDiffPlan,
        request: &ApplyRequest,
        cancel: &CancelToken,
    ) -> Result<SchemaDiffDeployResult, SchemaDiffPlanError>;

    /// §7：DDL 响应丢失/commit 应答丢失时的只读后置条件核验。
    async fn read_only_verify(
        &self,
        plan_meta: &SchemaDiffFrozenPlan,
        operation_id: &str,
    ) -> Result<ReadOnlyVerdict, SchemaDiffPlanError>;
}
