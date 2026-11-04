//! 桌面形态的已评审计划存储装配与组织作用域取用。
//!
//! B2 schema_diff 域刀（commands-layer-review §3.5「下沉核对」）：单租户收窄
//! 的存储实现已逐字下沉 `datazen::migration::schema_diff::reviewed_single_tenant`
//! （`SingleTenantReviewedPlanRepository`，测试随迁 modules）；本文件只剩
//! **装配桥接**——从桌面平台身份取组织 id 组装存储，身份组装失败时落到
//! deny-all 兜底——与 [`reviewed_scope`]（当前 IPC 调用的组织作用域）。
//!
//! 为什么桌面仍然是「进程内存」、时间基准为什么是墙上时钟：见 modules 侧
//! `reviewed.rs` / `reviewed_single_tenant.rs` 的模块文档（改造时的取舍记录
//! 随实现走）。桌面测试只保留装配路径守卫（平台身份不可用 ⇒ deny-all），
//! 收窄语义与 TTL/容量策略的测试在 modules。

use std::sync::Arc;

use datazen::migration::schema_diff::reviewed::ReviewedPlanRepository;
use datazen::migration::schema_diff::reviewed_single_tenant::SingleTenantReviewedPlanRepository;
use datazen::shared::scope::OrganizationScope;

use crate::commands::{AppState, CommandError};
use crate::platform::PlatformEntry;

/// 桌面存储的组装入口。
///
/// 桌面身份可用时只承认**桌面自己的**组织（取自 `DesktopIdentity::organization_id()`）；
/// 身份组装失败时**不编造**组织身份——该存储不承认任何组织，一切访问按「不存在」
/// 处理（依赖身份的 IPC 调用本来也会明确失败）。
pub fn desktop_reviewed_plan_repository(
    platform: &PlatformEntry,
) -> Arc<dyn ReviewedPlanRepository> {
    match platform.require() {
        Ok(adapter) => Arc::new(SingleTenantReviewedPlanRepository::new(
            adapter.identity().organization_id().clone(),
        )),
        Err(error) => {
            tracing::warn!(
                code = ?error.code,
                "桌面身份不可用：Schema Diff 评审计划存储不承认任何组织"
            );
            Arc::new(SingleTenantReviewedPlanRepository::deny_all())
        }
    }
}

/// 当前 IPC 调用的组织作用域。
///
/// 身份只来自 adapter 组装的请求上下文（INV-01）：这里**不接受**任何入参里的
/// 组织字段，作用域因此只由宿主决定，客户端无法伪造。
pub(crate) fn reviewed_scope(state: &AppState) -> Result<OrganizationScope, CommandError> {
    let adapter = state
        .platform_require()
        .map_err(crate::platform::into_command_error)?;
    let call = adapter.begin_call();
    Ok(OrganizationScope::from_context(call.context()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen::migration::schema_diff::reviewed::ReviewedPlan;
    use datazen::migration::schema_diff::types::SchemaDiffPlan;
    use datazen::shared::id::OrganizationId;

    fn scope(organization: &str) -> OrganizationScope {
        OrganizationScope::new(OrganizationId::new(organization))
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as u64)
            .unwrap_or(0)
    }

    fn plan(plan_id: &str) -> ReviewedPlan {
        let plan: SchemaDiffPlan = serde_json::from_value(serde_json::json!({
            "planId": plan_id,
            "table": "t",
            "tables": ["t"],
            "sourceDialect": "postgresql",
            "targetDialect": "postgresql",
            "sameDialect": true,
            "statements": [],
            "warnings": [],
            "requirements": [],
            "rollbackCompleteness": {"complete": true, "missing": []}
        }))
        .expect("plan fixture");
        ReviewedPlan {
            plan,
            target_session: "session".into(),
            target_pool: "pool".into(),
            target_identity: serde_json::json!({}),
            target_database_scope: None,
            target_schema_scope: None,
            snapshots: vec![],
            has_complete_target_dependency_catalog: false,
            target_dependency_schema_scope: None,
            object_snapshots: vec![],
            object_dependency_catalog: vec![],
            target_table_identity_catalog: vec![],
            has_complete_target_object_catalog: false,
            target_mysql_view_scope_context: None,
            source_snapshot: None,
            created_at_ms: now_ms(),
        }
    }

    /// 【装配路径】平台组装失败 ⇒ `desktop_reviewed_plan_repository` 返回**不承认任何
    /// 组织**的存储。
    ///
    /// 这条走的是真实的组装入口（不是直接构造 `deny_all()`）：平台身份不可用时，
    /// 装配必须落到 deny-all 兜底，**不得**编造组织身份，也**不得**让任意组织
    /// 的计划进入存储。收窄语义（跨组织读/写/删、TTL、容量、一次性消费）的
    /// 测试已随实现下沉
    /// `crates/datazen/src/migration/schema_diff/reviewed_single_tenant_tests.rs`。
    #[tokio::test]
    async fn desktop_store_assembly_denies_every_organization_when_platform_failed() {
        let failed = PlatformEntry::failed(datazen::shared::error::ApiError::new(
            datazen::shared::error::ApiErrorCode::Unauthenticated,
            "桌面本地身份不可用",
        ));
        let store = desktop_reviewed_plan_repository(&failed);
        let any = scope("datazen-desktop-local");
        let other = scope("other-org");

        // 写入不落数据。
        store
            .put(&any, "plan-1", plan("plan-1"))
            .await
            .expect("put plan");
        store
            .put(&other, "plan-2", plan("plan-2"))
            .await
            .expect("put plan");

        let visible = store.get(&any, "plan-1").await.expect("read plan");
        assert!(
            visible.is_none(),
            "平台组装失败时不得承认任何组织，实际读到了 {visible:?}"
        );
        let visible_other = store.get(&other, "plan-2").await.expect("read plan");
        assert!(
            visible_other.is_none(),
            "平台组装失败时不得承认任何组织，实际读到了 {visible_other:?}"
        );
        let removed = store.remove(&any, "plan-1").await.expect("remove plan");
        assert!(!removed, "平台组装失败时不得消费任何计划");
    }
}
