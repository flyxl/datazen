//! 可落盘的执行记录（连接 §4.4「与实时状态分离」）。
//!
//! ## 为什么要单独一个类型
//!
//! 实时 `ExecutionView` 带 `runtimeBinding: Option<RuntimeResultBinding>`，里面装着
//! `SessionHandle { dbSessionId, runtimeEpoch }` 与 `resourceBindingId`。这些是**运行时态**：
//! 进程重启后全部失效。连接 §4.4 的硬要求是「仓储只接受 `DurableExecutionRecord`」，
//! 因此持久化形状必须与实时形状**结构上分离**，而不能靠调用方自觉不传。
//!
//! 本模块用两道测试把这件事钉死：
//!
//! 1. [`the_durable_record_has_no_field_that_can_hold_a_handle`]：扫描本文件的**声明形状**
//!    （不是散文），确保没有任何字段能装下句柄、租约、游标或 `ExecutionView` 展开。
//! 2. [`projecting_a_bound_view_drops_the_runtime_binding`]：拿一个**确实带 runtimeBinding**
//!    的 `ExecutionView` 走投影，序列化后断言 JSON 里没有 `runtimeBinding` / `dbSessionId` /
//!    `runtimeEpoch` 键。
//!
//! ## 可以落盘 / 不可落盘的判定依据
//!
//! | 落盘 | 不落盘 |
//! |---|---|
//! | `ResultProvenance`（组织、主体、连接、configRevision、前后上下文、能力快照、执行时刻） | `SessionHandle` / `RuntimeResultBinding` |
//! | `StatementResultSource`（statement 序号、上下文、关系目标、写入映射） | 取消句柄（cancel handle） |
//! | `artifactIds`（产物 id 本身是持久引用） | lease、cursor、租约与隧道句柄 |
//! | `state` / `effectOutcome` / `resultCompleteness` / `errorCode` / 时间 | 实时事件流位置（sequence 由事件归档另管） |
//!
//! 依据：[连接 §4.4](../../../docs/architecture/platform/connection-management.md)、
//! [概要 §6.2](../../../docs/architecture/platform/system-overview.md)。

use datazen_platform_api::dto::execution::{
    EffectOutcome, ExecutionErrorCode, ExecutionState, ExecutionView, ResultCompleteness,
    ResultProvenance, StatementResultSource,
};
use datazen_platform_api::id::{ArtifactId, ExecutionId, OrganizationId, PrincipalId, Timestamp};
use serde::{Deserialize, Serialize};

/// 可落盘的执行记录。仓储层的入参类型（连接 §4.4）。
///
/// 字段集合逐条对应连接 §4.4 的「`executionId`、身份、`state`/`effectOutcome`、provenance、
/// `statementSources`、`artifactIds`、`resultCompleteness`/`truncationReason`、`errorCode`
/// 和时间」，**没有** `runtimeBinding`：句柄、租约、游标、取消句柄一律不进落盘形状。
///
/// `provenance` 为 `None` 表示执行在拿到来源快照之前就结束了（例如参数非法、目标不可定位）；
/// 此时顶层的 `organization_id` / `principal_id` 仍然是必填的——归属判定是仓储的过滤条件，
/// 不能因为一次失败执行就丢失归属。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DurableExecutionRecord {
    /// 执行 id（持久化引用）。
    pub execution_id: ExecutionId,
    /// 归属组织。
    pub organization_id: OrganizationId,
    /// 归属主体。
    pub principal_id: PrincipalId,
    /// 状态；终态失败用 `ExecutionErrorCode` 表达，不用 `ApiError`（连接 §13）。
    pub state: ExecutionState,
    /// 效果结论。`cancelled` 不是回滚证明，无法证明时是 `unknown`（连接 §13）。
    pub effect_outcome: EffectOutcome,
    /// 结果来源快照；未开始执行时为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<ResultProvenance>,
    /// 逐语句结果来源。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub statement_sources: Vec<StatementResultSource>,
    /// 产物 id 列表（产物字节在 ArtifactStore 里，记录只留引用）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifact_ids: Vec<ArtifactId>,
    /// 结果完整性。
    pub result_completeness: ResultCompleteness,
    /// 截断原因（仅 `truncated` 时有意义）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncation_reason: Option<String>,
    /// 执行错误码；与 `effect_outcome` 正交（连接 §13）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<ExecutionErrorCode>,
    /// 记录创建时刻。
    pub created_at: Timestamp,
    /// 最后一次落盘更新时刻。
    pub updated_at: Timestamp,
}

impl DurableExecutionRecord {
    /// 从实时视图投影出落盘记录。
    ///
    /// 这是**唯一**的构造路径之一（另一个是仓储自己新建记录），投影是**单向**的：
    /// `ExecutionView` 的运行时字段在这里被直接丢弃，不做任何形式的保留。
    pub fn from_view(
        view: &ExecutionView,
        organization_id: OrganizationId,
        principal_id: PrincipalId,
        created_at: Timestamp,
        updated_at: Timestamp,
    ) -> Self {
        Self {
            execution_id: view.execution_id.clone(),
            organization_id,
            principal_id,
            state: view.state,
            effect_outcome: view.effect_outcome,
            provenance: view.provenance.clone(),
            statement_sources: Vec::new(),
            artifact_ids: view.artifact_ids.clone(),
            result_completeness: view.result_completeness,
            truncation_reason: view.truncation_reason.clone(),
            error_code: view.error_code,
            created_at,
            updated_at,
        }
    }

    /// 逐语句结果来源的写入（执行完成后回填）。
    pub fn with_statement_sources(mut self, sources: Vec<StatementResultSource>) -> Self {
        self.statement_sources = sources;
        self
    }

    /// 是否已进入终态（连接 §13 的 `hostRejected` 只在记录已存在后才成立）。
    pub fn is_terminal(&self) -> bool {
        self.state.is_terminal()
    }

    /// 落盘形状绝不能描述运行时绑定（连接 §4.4）。
    pub fn has_runtime_binding(&self) -> bool {
        false
    }
}

/// 把实时视图与落盘记录对账时的事实：实时视图可以带运行时绑定，落盘记录一定不带。
///
/// 这是一个纯函数式的事实提取，供仓储在写盘前自检（例如在 debug 构建里断言）。
pub fn binding_would_be_lost(view: &ExecutionView) -> bool {
    view.has_live_runtime()
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_platform_api::dto::execution::{CapabilitySnapshot, RuntimeResultBinding};
    use datazen_platform_api::dto::session::{
        ContextConfidence, SessionContext, SessionHandle, TransactionState,
    };
    use datazen_platform_api::id::{
        ConfigRevision, ConnectionId, DbSessionId, ResourceId, RuntimeEpoch,
    };
    use datazen_platform_api::target::{
        CanonicalNamespace, CanonicalNamespaceId, CanonicalTarget, ExecutionTarget,
        NamespaceTarget, ObjectTarget,
    };

    fn code_only(source: &str) -> String {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn context() -> SessionContext {
        SessionContext {
            namespace: NamespaceTarget::new(
                Some("app".into()),
                None,
                Some("public".into()),
                Vec::new(),
            ),
            search_path: None,
            effective_identity: Some("app_user".into()),
            transaction_state: TransactionState::None,
            autocommit: Some(true),
            confidence: ContextConfidence::Confirmed,
        }
    }

    fn requested_target() -> ExecutionTarget {
        ExecutionTarget::new(ConnectionId::new("conn-1"), NamespaceTarget::default())
            .with_object(ObjectTarget::new("table", "orders"))
    }

    fn provenance() -> ResultProvenance {
        ResultProvenance {
            organization_id: OrganizationId::new("org-1"),
            principal_id: PrincipalId::new("principal-1"),
            connection_id: ConnectionId::new("conn-1"),
            config_revision: ConfigRevision::new(3),
            context_before: context(),
            context_after: context(),
            requested_target: requested_target(),
            capability_snapshot: CapabilitySnapshot::new("postgres", "1.0.0", 3)
                .with_confirmed("transaction", serde_json::Value::Bool(true)),
            executed_at: Timestamp::new("2026-01-01T00:00:00Z"),
        }
    }

    fn bound_view() -> ExecutionView {
        ExecutionView {
            execution_id: ExecutionId::new("exec-1"),
            state: ExecutionState::Running,
            effect_outcome: EffectOutcome::Unknown,
            provenance: Some(provenance()),
            artifact_ids: vec![ArtifactId::new("artifact-1")],
            result_completeness: ResultCompleteness::Pending,
            truncation_reason: None,
            error_code: None,
            runtime_binding: Some(RuntimeResultBinding {
                handle: SessionHandle::new(
                    DbSessionId::new("sess-live"),
                    RuntimeEpoch::new("epoch-7"),
                ),
                resource_binding_id: ResourceId::new("binding-1"),
                execution_id: ExecutionId::new("exec-1"),
            }),
        }
    }

    fn record() -> DurableExecutionRecord {
        DurableExecutionRecord::from_view(
            &bound_view(),
            OrganizationId::new("org-1"),
            PrincipalId::new("principal-1"),
            Timestamp::new("2026-01-01T00:00:00Z"),
            Timestamp::new("2026-01-01T00:00:05Z"),
        )
    }

    #[test]
    fn the_durable_record_has_no_field_that_can_hold_a_handle() {
        let source = code_only(include_str!("execution.rs"));
        let body = source
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default()
            .to_string();
        let declaration = body
            .split("pub struct DurableExecutionRecord {")
            .nth(1)
            .and_then(|rest| rest.split("\n}").next())
            .expect("durable record declaration")
            .to_string();
        assert!(declaration.contains("execution_id"));

        for forbidden in [
            "db_session_id",
            "runtime_epoch",
            "runtime_binding",
            "SessionHandle",
            "DbSessionId",
            "RuntimeEpoch",
            "RuntimeResultBinding",
            "ResourceId",
            "LeaseId",
            "cancel_handle",
            "cursor",
            "#[serde(flatten)]",
            "ExecutionView",
        ] {
            assert!(
                !declaration.contains(forbidden),
                "DurableExecutionRecord 声明不得包含 {forbidden}"
            );
        }
    }

    #[test]
    fn projecting_a_bound_view_drops_the_runtime_binding() {
        let view = bound_view();
        assert!(view.has_live_runtime(), "前置条件：视图确实带运行时绑定");
        assert!(binding_would_be_lost(&view));

        let durable = record();
        assert!(!durable.has_runtime_binding());
        let json = serde_json::to_value(&durable).expect("serialize durable record");
        let text = json.to_string();
        for forbidden in [
            "runtimeBinding",
            "dbSessionId",
            "runtimeEpoch",
            "resourceBindingId",
        ] {
            assert!(!text.contains(forbidden), "落盘 JSON 不得包含 {forbidden}");
        }
        // 实时视图里有、落盘记录里没有：证明投影是丢弃而不是搬运。
        let view_json = serde_json::to_value(&view).expect("serialize view");
        assert!(view_json["runtimeBinding"]["handle"]["dbSessionId"] == "sess-live");
    }

    #[test]
    fn durable_record_round_trips_through_serde() {
        let durable = record().with_statement_sources(vec![StatementResultSource {
            execution_id: ExecutionId::new("exec-1"),
            statement_index: 0,
            context: context(),
            relation: Some(requested_target()),
            writable_mapping: datazen_platform_api::dto::execution::WritableMapping::Verified,
        }]);
        let encoded = serde_json::to_string(&durable).expect("serialize");
        let decoded: DurableExecutionRecord = serde_json::from_str(&encoded).expect("deserialize");
        assert_eq!(decoded, durable);

        let json: serde_json::Value = serde_json::from_str(&encoded).expect("json");
        assert_eq!(json["executionId"], "exec-1");
        assert_eq!(json["organizationId"], "org-1");
        assert_eq!(json["principalId"], "principal-1");
        assert_eq!(json["state"], "running");
        assert_eq!(json["effectOutcome"], "unknown");
        assert_eq!(json["resultCompleteness"], "pending");
        assert_eq!(json["artifactIds"][0], "artifact-1");
        assert_eq!(json["statementSources"][0]["writableMapping"], "verified");
        assert!(json["truncationReason"].is_null());
        assert_eq!(json["createdAt"], "2026-01-01T00:00:00Z");
        assert_eq!(json["updatedAt"], "2026-01-01T00:00:05Z");
    }

    #[test]
    fn failed_terminal_record_carries_an_error_code_not_an_api_error() {
        let mut view = bound_view();
        view.runtime_binding = None;
        view.state = ExecutionState::Failed;
        view.effect_outcome = EffectOutcome::RolledBack;
        view.error_code = Some(ExecutionErrorCode::SqlError);
        let durable = DurableExecutionRecord::from_view(
            &view,
            OrganizationId::new("org-1"),
            PrincipalId::new("principal-1"),
            Timestamp::new("t0"),
            Timestamp::new("t1"),
        );
        assert!(durable.is_terminal());
        assert_eq!(durable.error_code, Some(ExecutionErrorCode::SqlError));
        assert_eq!(durable.effect_outcome, EffectOutcome::RolledBack);
        // errorCode 与 effectOutcome 正交：cancelled 不等于 rolledBack。
        view.error_code = Some(ExecutionErrorCode::Cancelled);
        view.effect_outcome = EffectOutcome::Unknown;
        let cancelled = DurableExecutionRecord::from_view(
            &view,
            OrganizationId::new("org-1"),
            PrincipalId::new("principal-1"),
            Timestamp::new("t0"),
            Timestamp::new("t1"),
        );
        assert_eq!(cancelled.error_code, Some(ExecutionErrorCode::Cancelled));
        assert_eq!(cancelled.effect_outcome, EffectOutcome::Unknown);
    }

    #[test]
    fn a_record_without_provenance_keeps_its_ownership() {
        let view = ExecutionView {
            execution_id: ExecutionId::new("exec-2"),
            state: ExecutionState::Failed,
            effect_outcome: EffectOutcome::NotStarted,
            provenance: None,
            artifact_ids: Vec::new(),
            result_completeness: ResultCompleteness::Complete,
            truncation_reason: None,
            error_code: None,
            runtime_binding: None,
        };
        let durable = DurableExecutionRecord::from_view(
            &view,
            OrganizationId::new("org-9"),
            PrincipalId::new("principal-9"),
            Timestamp::new("t0"),
            Timestamp::new("t0"),
        );
        assert_eq!(durable.organization_id, OrganizationId::new("org-9"));
        assert_eq!(durable.principal_id, PrincipalId::new("principal-9"));
        assert!(durable.provenance.is_none());
        let json = serde_json::to_value(&durable).expect("serialize");
        assert!(json.get("provenance").is_none());
        assert!(json.get("artifactIds").is_none());
    }

    #[test]
    fn canonical_target_is_the_only_identity_compared_at_runtime() {
        // 落盘记录里可对比的目标指纹来自 CanonicalTarget，而不是显示名或原始请求。
        let canonical = CanonicalTarget::new(
            ConnectionId::new("conn-1"),
            CanonicalNamespace {
                database: Some(CanonicalNamespaceId::new("app")),
                catalog: None,
                schema: Some(CanonicalNamespaceId::new("public")),
                path: Vec::new(),
            },
            None,
        );
        let durable = record();
        assert_eq!(canonical.namespace_fingerprint(), "app//public");
        assert_eq!(
            durable
                .provenance
                .as_ref()
                .map(|p| p.requested_target.namespace_fingerprint()),
            Some("//".to_string())
        );
    }
}
