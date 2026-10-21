//! 桌面形态的已评审计划存储（`ReviewedPlanStore` 端口实现）与组织作用域取用。
//!
//! ## 为什么桌面仍然是「进程内存」
//!
//! 评审计划是**一次性**的：准备 → 用户复核 → 部署 → 消费。桌面是单租户形态，
//! 计划本来就活在进程内；落盘只会把「一次性计划」变成跨会话残留，还得额外处理
//! 过期清理与并发消费。因此桌面选**进程内存**实现，行为与改造前完全一致。
//!
//! 真正的收益不在桌面，而在**契约**：评审计划不再寄居在包里的进程级全局变量上，
//! 而是寄居在宿主注入的实例上，并按组织作用域隔离——多租户宿主可以按组织分桶，
//! A 组织的计划不会被 B 组织读到、顶掉或提前消费掉。
//!
//! ## 单租户边界
//!
//! 桌面只承认**自己那一个**组织（组织 id 取自 `DesktopIdentity::organization_id()`）。
//! 传别的组织的 scope 一律按「不存在」处理：`get` 返回 `None`、`remove` 返回
//! `false`、`put` 不落数据——**不**返回「无权限」错误，那会泄漏「存在但无权」（CM-05）。

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use datazen_platform_api::id::OrganizationId;
use datazen_platform_api::ports::scope::OrganizationScope;
use datazen_schema_diff::reviewed::{
    ReviewedPlan, ReviewedPlanStore, ReviewedPlanStoreError, REVIEWED_PLAN_MAX_ENTRIES,
    REVIEWED_PLAN_TTL,
};
use tokio::sync::Mutex;

use crate::commands::{AppState, CommandError};
use crate::platform::PlatformEntry;

/// 桌面存储的组装入口。
///
/// 桌面身份可用时只承认**桌面自己的**组织（取自 `DesktopIdentity::organization_id()`）；
/// 身份组装失败时**不编造**组织身份——该存储不承认任何组织，一切访问按「不存在」
/// 处理（依赖身份的 IPC 调用本来也会明确失败）。
pub fn desktop_reviewed_plan_store(platform: &PlatformEntry) -> Arc<dyn ReviewedPlanStore> {
    match platform.require() {
        Ok(adapter) => Arc::new(DesktopReviewedPlanStore::new(
            adapter.identity().organization_id().clone(),
        )),
        Err(error) => {
            tracing::warn!(
                code = ?error.code,
                "桌面身份不可用：Schema Diff 评审计划存储不承认任何组织"
            );
            Arc::new(DesktopReviewedPlanStore::deny_all())
        }
    }
}

/// 桌面形态的已评审计划存储：进程内存实现。
pub struct DesktopReviewedPlanStore {
    /// 桌面自己的组织。`None` 表示桌面身份不可用 ⇒ 不承认任何组织。
    organization_id: Option<OrganizationId>,
    plans: Mutex<HashMap<String, ReviewedPlan>>,
}

impl DesktopReviewedPlanStore {
    /// 只承认桌面自己的组织。
    pub fn new(organization_id: OrganizationId) -> Self {
        Self {
            organization_id: Some(organization_id),
            plans: Mutex::new(HashMap::new()),
        }
    }

    /// 桌面身份组装失败时的实例：不承认任何组织，一切访问按「不存在」处理。
    pub fn deny_all() -> Self {
        Self {
            organization_id: None,
            plans: Mutex::new(HashMap::new()),
        }
    }

    /// 本存储是否承认该作用域。
    fn acknowledges(&self, scope: &OrganizationScope) -> bool {
        self.organization_id.as_ref() == Some(scope.organization_id())
    }
}

#[async_trait]
impl ReviewedPlanStore for DesktopReviewedPlanStore {
    async fn put(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
        plan: ReviewedPlan,
    ) -> Result<(), ReviewedPlanStoreError> {
        if !self.acknowledges(scope) {
            // 越组织写入按「未发生」处理：不落数据，也不返回「无权限」。
            return Ok(());
        }
        let mut plans = self.plans.lock().await;
        plans.retain(|_, plan| plan.age() < REVIEWED_PLAN_TTL);
        // Keep memory bounded even when a window repeatedly prepares a plan.
        if plans.len() >= REVIEWED_PLAN_MAX_ENTRIES {
            if let Some(oldest) = plans
                .iter()
                .min_by_key(|(_, plan)| plan.created_at_ms)
                .map(|(id, _)| id.clone())
            {
                plans.remove(&oldest);
            }
        }
        plans.insert(plan_id.to_owned(), plan);
        Ok(())
    }

    async fn get(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
    ) -> Result<Option<ReviewedPlan>, ReviewedPlanStoreError> {
        if !self.acknowledges(scope) {
            return Ok(None);
        }
        Ok(self.plans.lock().await.get(plan_id).cloned())
    }

    async fn remove(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
    ) -> Result<bool, ReviewedPlanStoreError> {
        if !self.acknowledges(scope) {
            return Ok(false);
        }
        Ok(self.plans.lock().await.remove(plan_id).is_some())
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
    use datazen_platform_api::id::OrganizationId;
    use datazen_schema_diff::reviewed::ReviewedPlan;
    use datazen_schema_diff::types::SchemaDiffPlan;

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

    /// 桌面只承认自己那一个组织：别的组织**读不到**。
    #[tokio::test]
    async fn desktop_store_hides_plans_from_other_organizations() {
        let store = DesktopReviewedPlanStore::new(OrganizationId::new("datazen-desktop-local"));
        let owner = scope("datazen-desktop-local");
        let other = scope("other-org");
        store
            .put(&owner, "plan-1", plan("plan-1"))
            .await
            .expect("put plan");

        let leaked = store.get(&other, "plan-1").await.expect("read plan");
        assert!(
            leaked.is_none(),
            "跨组织必须读不到计划，实际读到了 {leaked:?}"
        );

        let owned = store.get(&owner, "plan-1").await.expect("read plan");
        assert!(owned.is_some(), "本组织的计划必须读得到");
    }

    /// 跨组织删除按「不存在」处理：删不掉别人的计划，也不返回「无权限」。
    #[tokio::test]
    async fn desktop_store_refuses_to_remove_other_organizations_plans() {
        let store = DesktopReviewedPlanStore::new(OrganizationId::new("datazen-desktop-local"));
        let owner = scope("datazen-desktop-local");
        let other = scope("other-org");
        store
            .put(&owner, "plan-1", plan("plan-1"))
            .await
            .expect("put plan");

        let removed = store.remove(&other, "plan-1").await.expect("remove plan");
        assert!(!removed, "跨组织删除必须按「不存在」处理");

        // 别人的删除没有影响本组织的计划。
        let owned = store.get(&owner, "plan-1").await.expect("read plan");
        assert!(owned.is_some(), "本组织的计划必须仍然可读");
    }

    /// 跨组织写入不落数据：本组织读不到「别的组织写进来」的计划。
    #[tokio::test]
    async fn desktop_store_drops_writes_from_other_organizations() {
        let store = DesktopReviewedPlanStore::new(OrganizationId::new("datazen-desktop-local"));
        let owner = scope("datazen-desktop-local");
        let other = scope("other-org");
        store
            .put(&other, "plan-1", plan("plan-1"))
            .await
            .expect("put plan");

        let visible = store.get(&owner, "plan-1").await.expect("read plan");
        assert!(visible.is_none(), "别的组织写入的计划不得对本组织可见");
    }

    /// 桌面身份不可用时，存储不承认任何组织：一切访问按「不存在」处理。
    #[tokio::test]
    async fn desktop_store_without_identity_denies_every_organization() {
        let store = DesktopReviewedPlanStore::deny_all();
        let any = scope("datazen-desktop-local");
        store
            .put(&any, "plan-1", plan("plan-1"))
            .await
            .expect("put plan");

        let visible = store.get(&any, "plan-1").await.expect("read plan");
        assert!(visible.is_none(), "身份不可用时不得承认任何组织");
        let removed = store.remove(&any, "plan-1").await.expect("remove plan");
        assert!(!removed, "身份不可用时不得消费任何计划");
    }

    /// 一次性消费：同一个计划只能被消费一次。
    #[tokio::test]
    async fn desktop_store_consumes_a_plan_exactly_once() {
        let store = DesktopReviewedPlanStore::new(OrganizationId::new("datazen-desktop-local"));
        let owner = scope("datazen-desktop-local");
        store
            .put(&owner, "plan-1", plan("plan-1"))
            .await
            .expect("put plan");

        assert!(
            store.remove(&owner, "plan-1").await.expect("remove plan"),
            "第一次消费必须成功"
        );
        assert!(
            !store.remove(&owner, "plan-1").await.expect("remove plan"),
            "第二次消费必须失败"
        );
    }

    /// 桌面存储按 TTL 裁剪过期条目：写入新计划时，已过期的旧计划被清掉。
    ///
    /// 这条钉住的是**容量/过期策略**本身（改造前是 `plans.retain(...)` 的既有魔数）。
    #[tokio::test]
    async fn desktop_store_trims_expired_plans_on_put() {
        let store = DesktopReviewedPlanStore::new(OrganizationId::new("datazen-desktop-local"));
        let owner = scope("datazen-desktop-local");
        let mut stale = plan("stale");
        stale.created_at_ms = now_ms().saturating_sub(31 * 60 * 1000);
        store
            .put(&owner, "stale", stale)
            .await
            .expect("put stale plan");
        store
            .put(&owner, "fresh", plan("fresh"))
            .await
            .expect("put fresh plan");

        assert!(
            store
                .get(&owner, "stale")
                .await
                .expect("read stale plan")
                .is_none(),
            "过期计划必须在下次写入时被裁剪"
        );
        assert!(
            store
                .get(&owner, "fresh")
                .await
                .expect("read fresh plan")
                .is_some(),
            "新写入的计划必须留下"
        );
    }

    /// 桌面存储按 `REVIEWED_PLAN_MAX_ENTRIES`（128）封顶：超出时裁剪**最旧**的一条。
    ///
    /// 写 130 条、每次至多裁剪 1 条 ⇒ 恰好保留最新的 128 条：
    /// `plan-0` / `plan-1` 被裁掉，`plan-2` 起仍在。若上限被改成更小的数，
    /// 「plan-2 仍在」这条断言会红。
    #[tokio::test]
    async fn desktop_store_enforces_max_entries_by_evicting_the_oldest() {
        let store = DesktopReviewedPlanStore::new(OrganizationId::new("datazen-desktop-local"));
        let owner = scope("datazen-desktop-local");
        let base = now_ms();
        for index in 0..130u64 {
            let mut entry = plan(&format!("plan-{index}"));
            entry.created_at_ms = base + index;
            store
                .put(&owner, &format!("plan-{index}"), entry)
                .await
                .expect("put plan");
        }
        assert!(
            store
                .get(&owner, "plan-0")
                .await
                .expect("read plan")
                .is_none(),
            "最旧的条目必须被裁剪"
        );
        assert!(
            store
                .get(&owner, "plan-1")
                .await
                .expect("read plan")
                .is_none(),
            "次旧的条目必须被裁剪"
        );
        assert!(
            store
                .get(&owner, "plan-2")
                .await
                .expect("read plan")
                .is_some(),
            "第 3 条必须仍在（上限正是 128，而不是更小的数）"
        );
        assert!(
            store
                .get(&owner, "plan-129")
                .await
                .expect("read plan")
                .is_some(),
            "最新的条目必须仍在"
        );
    }

    /// 桌面存储的 `get` **不**按 TTL 过滤：过期判定由规则层负责。
    ///
    /// 与改造前一致：改造前的 `plans.get(id)` 同样会返回过期条目，
    /// 由 `reviewed::validate` 报 "Plan expired; compare again"。
    #[tokio::test]
    async fn desktop_store_serves_expired_plans_for_the_rule_layer_to_reject() {
        let store = DesktopReviewedPlanStore::new(OrganizationId::new("datazen-desktop-local"));
        let owner = scope("datazen-desktop-local");
        let mut stale = plan("stale");
        stale.created_at_ms = now_ms().saturating_sub(31 * 60 * 1000);
        store
            .put(&owner, "stale", stale)
            .await
            .expect("put stale plan");
        assert!(
            store
                .get(&owner, "stale")
                .await
                .expect("read stale plan")
                .is_some(),
            "存储层不按 TTL 过滤，过期判定在规则层"
        );
    }

    /// 桌面存储 + 规则层的**一次性消费**：并发部署只有一个成功。
    ///
    /// 这条把生产的存储实现（`DesktopReviewedPlanStore`）与生产的规则层
    /// （`reviewed::consume`）接在一起，覆盖「并发只有一个 `remove` 成功」的真实路径，
    /// 而不是只在测试替身上验证。
    #[tokio::test]
    async fn desktop_store_admits_exactly_one_concurrent_consumer() {
        let store = DesktopReviewedPlanStore::new(OrganizationId::new("datazen-desktop-local"));
        let owner = scope("datazen-desktop-local");
        let cfg: datazen_driver_api::ConnectionConfig = serde_json::from_value(serde_json::json!({
            "id": "target",
            "name": "target",
            "databaseType": "postgresql",
            "host": "localhost",
            "port": 5432,
            "database": "test",
            "connectionTimeout": 30,
            "maxPoolSize": 3
        }))
        .expect("config fixture");
        let handle = datazen_driver_api::ConnectionHandle {
            id: "session".into(),
            pool_id: "pool".into(),
        };
        let mut frozen = plan("plan-1");
        frozen.target_session = "session".into();
        frozen.target_pool = "pool".into();
        frozen.target_identity = datazen_schema_diff::reviewed::identity(&cfg);
        frozen.target_database_scope = cfg.database.clone();
        frozen.target_schema_scope = Some("public".into());
        store
            .put(&owner, "plan-1", frozen.clone())
            .await
            .expect("put plan");

        let (a, b) = tokio::join!(
            datazen_schema_diff::reviewed::consume(
                &store,
                &owner,
                &frozen.plan,
                "session",
                &handle,
                &cfg,
                cfg.database.as_deref(),
                Some("public"),
            ),
            datazen_schema_diff::reviewed::consume(
                &store,
                &owner,
                &frozen.plan,
                "session",
                &handle,
                &cfg,
                cfg.database.as_deref(),
                Some("public"),
            ),
        );
        assert!(
            a.is_ok() ^ b.is_ok(),
            "两个并发消费必须只有一个成功（a={:?} b={:?}）",
            a.as_ref().err(),
            b.as_ref().err()
        );
        // 落败方不得部署成功；文案取决于调度顺序，但必须落在「已执行 / 已过期」两种之一。
        let loser = if a.is_ok() { &b } else { &a };
        let message = loser.as_ref().unwrap_err().as_str();
        assert!(
            message == "Plan already executed"
                || message == "Plan expired or already executed; compare again",
            "落败方必须报「已执行 / 已过期」，实际 {message:?}"
        );
        assert!(
            store
                .get(&owner, "plan-1")
                .await
                .expect("read plan")
                .is_none(),
            "消费之后计划必须已被删除"
        );
    }
}
