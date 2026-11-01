//! 单租户收窄存储的用例（B2 schema_diff 域刀自桌面 `reviewed_repository.rs`
//! 逐字迁移；断言与测试名语义不变，仅类型更名 `DesktopReviewedPlanRepository`
//! → [`SingleTenantReviewedPlanRepository`]、路径改走 `datazen` 内部路径）。
//!
//! 夹具组织 id 用形态中性的 `"datazen-local-org"`：桌面的规范本地组织 id
//! 含形态专名，共享内核禁止该字面量（form_neutrality 闸门）；存储对 id 的
//! 具体取值不做任何假设，测试语义不受影响。
//!
//! 装配路径（宿主身份源失败 ⇒ deny-all 兜底）留在宿主侧的
//! `commands/schema_diff/reviewed_repository.rs`——那里钉的是宿主装配契约。

use super::*;
use crate::migration::schema_diff::reviewed::ReviewedPlan;
use crate::migration::schema_diff::types::SchemaDiffPlan;
use crate::shared::id::OrganizationId;

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

/// 单租户存储只承认自己那一个组织：别的组织**读不到**。
#[tokio::test]
async fn single_tenant_store_hides_plans_from_other_organizations() {
    let store = SingleTenantReviewedPlanRepository::new(OrganizationId::new("datazen-local-org"));
    let owner = scope("datazen-local-org");
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
async fn single_tenant_store_refuses_to_remove_other_organizations_plans() {
    let store = SingleTenantReviewedPlanRepository::new(OrganizationId::new("datazen-local-org"));
    let owner = scope("datazen-local-org");
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
async fn single_tenant_store_drops_writes_from_other_organizations() {
    let store = SingleTenantReviewedPlanRepository::new(OrganizationId::new("datazen-local-org"));
    let owner = scope("datazen-local-org");
    let other = scope("other-org");
    store
        .put(&other, "plan-1", plan("plan-1"))
        .await
        .expect("put plan");

    let visible = store.get(&owner, "plan-1").await.expect("read plan");
    assert!(visible.is_none(), "别的组织写入的计划不得对本组织可见");
}

/// 单租户存储**丢弃**越组织写入：别的组织写的计划**根本没有进存储**。
///
/// 上一条（`single_tenant_store_drops_writes_from_other_organizations`）只断言
/// 「本组织读不到」——包内实现按组织分桶时，本组织本来就读不到，因此那条
/// 断言**分不清**「越组织写入被丢弃」与「落进了别的组织桶」。
///
/// 本条断言**写入方自己**也读不到 ⇒ 越组织写入确实**没有落数据**
/// （单租户收窄承重）。若宿主把收窄改成「只要有身份就承认任何组织」，
/// 本条会红。
#[tokio::test]
async fn single_tenant_store_discards_cross_organization_writes_entirely() {
    let store = SingleTenantReviewedPlanRepository::new(OrganizationId::new("datazen-local-org"));
    let owner = scope("datazen-local-org");
    let other = scope("other-org");
    store
        .put(&other, "plan-1", plan("plan-1"))
        .await
        .expect("put plan");

    // 写入方自己**也**读不到：越组织写入被丢弃，不是「落进别的桶」。
    let visible_to_writer = store.get(&other, "plan-1").await.expect("read plan");
    assert!(
        visible_to_writer.is_none(),
        "越组织写入必须被丢弃（不得落进别的组织桶），实际读到了 {visible_to_writer:?}"
    );
    // 本组织同样读不到。
    let visible_to_owner = store.get(&owner, "plan-1").await.expect("read plan");
    assert!(
        visible_to_owner.is_none(),
        "别的组织写入的计划不得对本组织可见"
    );
}

/// 身份不可用（deny-all 实例）时，存储不承认任何组织：一切访问按「不存在」处理。
#[tokio::test]
async fn single_tenant_store_without_identity_denies_every_organization() {
    let store = SingleTenantReviewedPlanRepository::deny_all();
    let any = scope("datazen-local-org");
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
async fn single_tenant_store_consumes_a_plan_exactly_once() {
    let store = SingleTenantReviewedPlanRepository::new(OrganizationId::new("datazen-local-org"));
    let owner = scope("datazen-local-org");
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

/// 存储按 TTL 裁剪过期条目：写入新计划时，已过期的旧计划被清掉。
///
/// 这条钉住的是**容量/过期策略**本身（改造前是 `plans.retain(...)` 的既有魔数）。
#[tokio::test]
async fn single_tenant_store_trims_expired_plans_on_put() {
    let store = SingleTenantReviewedPlanRepository::new(OrganizationId::new("datazen-local-org"));
    let owner = scope("datazen-local-org");
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

/// 存储按 `REVIEWED_PLAN_MAX_ENTRIES`（128）封顶：超出时裁剪**最旧**的一条。
///
/// 写 130 条、每次至多裁剪 1 条 ⇒ 恰好保留最新的 128 条：
/// `plan-0` / `plan-1` 被裁掉，`plan-2` 起仍在。若上限被改成更小的数，
/// 「plan-2 仍在」这条断言会红。
///
/// 冻结时刻必须落在**过去**：落在**未来**的时间戳意味着「系统时钟相对冻结
/// 时刻被回拨过」⇒ 条目按规则层判**已失效** ⇒ 存储层会先把它**裁掉**
/// （`is_expired()`），而不是拿它参与淘汰排序。排序键改用过去的时刻后，
/// 淘汰顺序与断言完全不变（index 越小 ⇒ 冻结越早 ⇒ 越先被淘汰）。
#[tokio::test]
async fn single_tenant_store_enforces_max_entries_by_evicting_the_oldest() {
    let store = SingleTenantReviewedPlanRepository::new(OrganizationId::new("datazen-local-org"));
    let owner = scope("datazen-local-org");
    let base = now_ms();
    for index in 0..130u64 {
        let mut entry = plan(&format!("plan-{index}"));
        entry.created_at_ms = base.saturating_sub(130 - index);
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

/// 存储的 `get` **不**按 TTL 过滤：过期判定由规则层负责。
///
/// 与改造前一致：改造前的 `plans.get(id)` 同样会返回过期条目，
/// 由 `reviewed::validate` 报 "Plan expired; compare again"。
#[tokio::test]
async fn single_tenant_store_serves_expired_plans_for_the_rule_layer_to_reject() {
    let store = SingleTenantReviewedPlanRepository::new(OrganizationId::new("datazen-local-org"));
    let owner = scope("datazen-local-org");
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

/// 单租户存储 + 规则层的**一次性消费**：并发部署只有一个成功。
///
/// 这条把生产的存储实现（[`SingleTenantReviewedPlanRepository`]）与生产的规则层
/// （`reviewed::consume`）接在一起，覆盖「并发只有一个 `remove` 成功」的真实路径，
/// 而不是只在测试替身上验证。
#[tokio::test]
async fn single_tenant_store_admits_exactly_one_concurrent_consumer() {
    let store = SingleTenantReviewedPlanRepository::new(OrganizationId::new("datazen-local-org"));
    let owner = scope("datazen-local-org");
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
    frozen.target_identity = crate::migration::schema_diff::reviewed::identity(&cfg);
    frozen.target_database_scope = cfg.database.clone();
    frozen.target_schema_scope = Some("public".into());
    store
        .put(&owner, "plan-1", frozen.clone())
        .await
        .expect("put plan");

    let (a, b) = tokio::join!(
        crate::migration::schema_diff::reviewed::consume(
            &store,
            &owner,
            &frozen.plan,
            "session",
            &handle,
            &cfg,
            cfg.database.as_deref(),
            Some("public"),
        ),
        crate::migration::schema_diff::reviewed::consume(
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
