//! 评审计划**存储端口**的用例：一次性消费语义 + 组织隔离。
//!
//! 规则（身份指纹、快照复核）的用例在 `reviewed_tests.rs`；这里只钉存储契约：
//! 「一次部署只消费一次」与「越组织读不到」。

use super::*;
use crate::shared::id::OrganizationId;
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::Mutex;

/// 进程内存的测试存储：按组织分桶。
///
/// 越组织访问按「不存在」处理——取不到、也删不掉别人的计划。
///
/// **这是替身，不是生产实现**：它只用来驱动「规则层语义」类用例（有效期、
/// 竞态、校验失败不烧计划）。**隔离性**类断言必须打生产实现
/// （`InMemoryReviewedPlanRepository`）——替身的键结构自带组织维度，在替身上断言
/// 隔离对生产码是同义反复。
#[derive(Default)]
struct TestPlanRepository {
    plans: Mutex<HashMap<(String, String), ReviewedPlan>>,
}

impl TestPlanRepository {
    fn bucket(scope: &OrganizationScope, plan_id: &str) -> (String, String) {
        (
            scope.organization_id().as_str().to_owned(),
            plan_id.to_owned(),
        )
    }
}

#[async_trait]
impl ReviewedPlanRepository for TestPlanRepository {
    async fn put(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
        plan: ReviewedPlan,
    ) -> Result<(), ReviewedPlanRepositoryError> {
        self.plans
            .lock()
            .await
            .insert(Self::bucket(scope, plan_id), plan);
        Ok(())
    }

    async fn get(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
    ) -> Result<Option<ReviewedPlan>, ReviewedPlanRepositoryError> {
        Ok(self
            .plans
            .lock()
            .await
            .get(&Self::bucket(scope, plan_id))
            .cloned())
    }

    async fn remove(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
    ) -> Result<bool, ReviewedPlanRepositoryError> {
        Ok(self
            .plans
            .lock()
            .await
            .remove(&Self::bucket(scope, plan_id))
            .is_some())
    }
}

fn scope(organization: &str) -> OrganizationScope {
    OrganizationScope::new(OrganizationId::new(organization))
}

fn config() -> ConnectionConfig {
    serde_json::from_value(serde_json::json!({"id":"target","name":"target","databaseType":"postgresql","host":"localhost","port":5432,"database":"test","connectionTimeout":30,"maxPoolSize":3})).unwrap()
}
fn plan() -> SchemaDiffPlan {
    serde_json::from_value(serde_json::json!({"table":"t","tables":["t"],"sourceDialect":"postgresql","targetDialect":"postgresql","sameDialect":true,"statements":[],"warnings":[],"requirements":[],"rollbackCompleteness":{"complete":true,"missing":[]}})).unwrap()
}
fn handle() -> ConnectionHandle {
    ConnectionHandle {
        id: "session".into(),
        pool_id: "pool".into(),
    }
}

#[tokio::test]
async fn reviewed_plan_rejects_client_mutation_wrong_target_and_replay() {
    let store = TestPlanRepository::default();
    let scope = scope("org-a");
    let mut plan = plan();
    let config = config();
    let handle = handle();
    freeze(
        &store,
        &scope,
        &mut plan,
        "session".into(),
        &handle,
        &config,
        vec![],
        config.database.clone(),
        Some("public".into()),
    )
    .await
    .expect("freeze plan");
    let mut tampered = plan.clone();
    tampered.warnings.push("client modification".into());
    assert!(consume(
        &store,
        &scope,
        &tampered,
        "session",
        &handle,
        &config,
        config.database.as_deref(),
        Some("public"),
    )
    .await
    .is_err());
    assert!(consume(
        &store,
        &scope,
        &plan,
        "other",
        &handle,
        &config,
        config.database.as_deref(),
        Some("public"),
    )
    .await
    .is_err());
    let mut changed = config.clone();
    changed.database = Some("other_database".into());
    assert!(consume(
        &store,
        &scope,
        &plan,
        "session",
        &handle,
        &changed,
        changed.database.as_deref(),
        Some("public"),
    )
    .await
    .is_err());
    changed = config.clone();
    changed.read_only = true;
    assert!(consume(
        &store,
        &scope,
        &plan,
        "session",
        &handle,
        &changed,
        changed.database.as_deref(),
        Some("public"),
    )
    .await
    .is_err());
    assert!(consume(
        &store,
        &scope,
        &plan,
        "session",
        &handle,
        &config,
        config.database.as_deref(),
        Some("public"),
    )
    .await
    .is_ok());
    assert!(consume(
        &store,
        &scope,
        &plan,
        "session",
        &handle,
        &config,
        config.database.as_deref(),
        Some("archive"),
    )
    .await
    .is_err());
}

#[tokio::test]
async fn test_tester_concurrent_deploy_consumes_exactly_once() {
    let store = TestPlanRepository::default();
    let scope = scope("org-a");
    let mut plan = plan();
    let cfg = config();
    let handle = ConnectionHandle {
        id: "concurrent".into(),
        pool_id: "concurrent_pool".into(),
    };
    freeze(
        &store,
        &scope,
        &mut plan,
        "concurrent".into(),
        &handle,
        &cfg,
        vec![],
        cfg.database.clone(),
        cfg.schema.clone(),
    )
    .await
    .expect("freeze plan");
    let (a, b) = tokio::join!(
        consume(
            &store,
            &scope,
            &plan,
            "concurrent",
            &handle,
            &cfg,
            cfg.database.as_deref(),
            cfg.schema.as_deref()
        ),
        consume(
            &store,
            &scope,
            &plan,
            "concurrent",
            &handle,
            &cfg,
            cfg.database.as_deref(),
            cfg.schema.as_deref()
        )
    );
    assert_ne!(a.is_ok(), b.is_ok());
}

#[tokio::test]
async fn test_tester_pool_change_rejected_without_consuming_valid_plan() {
    let store = TestPlanRepository::default();
    let scope = scope("org-a");
    let mut plan = plan();
    let cfg = config();
    let handle = ConnectionHandle {
        id: "pooltest".into(),
        pool_id: "original".into(),
    };
    freeze(
        &store,
        &scope,
        &mut plan,
        "pooltest".into(),
        &handle,
        &cfg,
        vec![],
        cfg.database.clone(),
        cfg.schema.clone(),
    )
    .await
    .expect("freeze plan");
    let changed = ConnectionHandle {
        id: "pooltest".into(),
        pool_id: "replacement".into(),
    };
    assert!(consume(
        &store,
        &scope,
        &plan,
        "pooltest",
        &changed,
        &cfg,
        cfg.database.as_deref(),
        cfg.schema.as_deref()
    )
    .await
    .is_err());
    assert!(consume(
        &store,
        &scope,
        &plan,
        "pooltest",
        &handle,
        &cfg,
        cfg.database.as_deref(),
        cfg.schema.as_deref()
    )
    .await
    .is_ok());
}

/// 跨组织读不到：别的组织既**取不到**计划，也**删不掉**它。
///
/// 逐条断言，不写「大概率读不到」这类模糊断言。
///
/// **打的是生产实现**（`InMemoryReviewedPlanRepository`，随端口一起提供的进程内实现），
/// 不是测试替身：替身的键结构自带组织维度，断言在替身上恒成立，对生产码是同义反复。
#[tokio::test]
async fn reviewed_plan_store_hides_plans_from_other_organizations() {
    let store = InMemoryReviewedPlanRepository::new();
    let owner = scope("org-a");
    let other = scope("org-b");
    let mut plan = plan();
    let config = config();
    let handle = handle();
    freeze(
        &store,
        &owner,
        &mut plan,
        "session".into(),
        &handle,
        &config,
        vec![],
        config.database.clone(),
        Some("public".into()),
    )
    .await
    .expect("freeze plan");
    let plan_id = plan.plan_id.clone().expect("frozen plan carries its id");

    // 1) 别的组织读不到。
    let leaked = store.get(&other, &plan_id).await.expect("store readable");
    assert!(
        leaked.is_none(),
        "跨组织必须读不到计划，实际读到了 {leaked:?}"
    );

    // 2) 别的组织删不掉（删除按「不存在」处理，不返回「无权限」）。
    let removed = store
        .remove(&other, &plan_id)
        .await
        .expect("store writable");
    assert!(!removed, "跨组织删除必须按「不存在」处理，实际报告已删除");

    // 3) 本组织仍然读得到：别人的删除没有影响自己的计划。
    let owned = store.get(&owner, &plan_id).await.expect("store readable");
    assert!(owned.is_some(), "本组织的计划必须仍然可读，实际读不到");
}

/// 跨组织部署读不到别人的计划：消费按「不存在」失败，且**不**烧掉本组织的计划。
///
/// 同样打生产实现。
#[tokio::test]
async fn reviewed_plan_consume_rejects_foreign_organization_scope() {
    let store = InMemoryReviewedPlanRepository::new();
    let owner = scope("org-a");
    let other = scope("org-b");
    let mut plan = plan();
    let config = config();
    let handle = handle();
    freeze(
        &store,
        &owner,
        &mut plan,
        "session".into(),
        &handle,
        &config,
        vec![],
        config.database.clone(),
        Some("public".into()),
    )
    .await
    .expect("freeze plan");

    let error = consume(
        &store,
        &other,
        &plan,
        "session",
        &handle,
        &config,
        config.database.as_deref(),
        Some("public"),
    )
    .await
    .expect_err("跨组织消费必须失败");
    assert_eq!(
        error, "Plan expired or already executed; compare again",
        "跨组织访问必须按「不存在」处理，不得泄漏「存在但无权」"
    );

    // 别的组织消费失败后，本组织的计划仍然可正常消费。
    assert!(consume(
        &store,
        &owner,
        &plan,
        "session",
        &handle,
        &config,
        config.database.as_deref(),
        Some("public"),
    )
    .await
    .is_ok());
}

/// 跨组织写入**不进本组织的桶**：别的组织写进来的计划，本组织读不到。
///
/// 打生产实现。生产实现按 `(organization_id, plan_id)` 分桶，因此别的组织的写入
/// 落在**它自己**的桶里；本组织既读不到，也消费不到。
///
/// 桌面形态的「越组织写入**直接丢弃**」是宿主在生产实现之上叠加的**单租户**
/// 策略（见 `src-tauri/src/commands/schema_diff/reviewed_repository.rs`），不是本包
/// 生产实现的语义。
#[tokio::test]
async fn reviewed_plan_store_keeps_foreign_writes_out_of_other_organizations() {
    let store = InMemoryReviewedPlanRepository::new();
    let owner = scope("org-a");
    let other = scope("org-b");
    let mut plan = plan();
    let config = config();
    let handle = handle();
    freeze(
        &store,
        &other,
        &mut plan,
        "session".into(),
        &handle,
        &config,
        vec![],
        config.database.clone(),
        Some("public".into()),
    )
    .await
    .expect("freeze plan");
    let plan_id = plan.plan_id.clone().expect("frozen plan carries its id");

    let visible = store.get(&owner, &plan_id).await.expect("store readable");
    assert!(visible.is_none(), "别的组织写入的计划不得对本组织可见");

    // 写入落在它自己的桶里（分桶，而不是丢弃）。
    let own = store.get(&other, &plan_id).await.expect("store readable");
    assert!(own.is_some(), "写入必须落在它自己的组织桶里");
}

/// 生产实现确实**按组织分桶**：同一个 `planId` 在两个组织下是两条互不影响的记录。
///
/// 这条直接打生产实现（`InMemoryReviewedPlanRepository`）：若键里少了组织维度，
/// 后写入的那条会顶掉先写入的那条，本用例的「两条都在」断言会红。
#[tokio::test]
async fn in_memory_store_buckets_plans_by_organization() {
    let store = InMemoryReviewedPlanRepository::new();
    let first = scope("org-a");
    let second = scope("org-b");

    // 同一个 planId，两个组织各写一份**内容不同**的计划。
    let plan_a = reviewed_plan("shared-plan", now_ms());
    let mut plan_b = reviewed_plan("shared-plan", now_ms());
    plan_b.plan.warnings.push("org-b copy".into());

    store
        .put(&first, "shared-plan", plan_a.clone())
        .await
        .expect("put plan for org-a");
    store
        .put(&second, "shared-plan", plan_b.clone())
        .await
        .expect("put plan for org-b");

    assert_eq!(
        store.len().await,
        2,
        "两个组织各一条记录：键里必须带组织维度"
    );
    let from_first = store.get(&first, "shared-plan").await.expect("read plan");
    let from_second = store.get(&second, "shared-plan").await.expect("read plan");
    assert_eq!(
        from_first.map(|entry| entry.plan),
        Some(plan_a.plan.clone()),
        "org-a 的记录不得被 org-b 的同名写入顶掉"
    );
    assert_eq!(
        from_second.map(|entry| entry.plan),
        Some(plan_b.plan.clone()),
        "org-b 的记录必须是自己那一份"
    );

    // 删掉一边不影响另一边。
    assert!(
        store
            .remove(&first, "shared-plan")
            .await
            .expect("remove plan"),
        "本组织的记录必须删得掉"
    );
    assert!(
        store
            .get(&second, "shared-plan")
            .await
            .expect("read plan")
            .is_some(),
        "删除一个组织的记录不得影响另一个组织"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 有效期语义：冻结时刻用 Unix 毫秒，TTL = 30 分钟
// ─────────────────────────────────────────────────────────────────────────────

/// 当前时刻（Unix 毫秒）——与被测代码同一时间基准。
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// 造一份「冻结时刻」可指定的评审计划；其余字段与 `config()` / `handle()` 对齐。
fn reviewed_plan(plan_id: &str, created_at_ms: u64) -> ReviewedPlan {
    let cfg = config();
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
        target_identity: identity(&cfg),
        target_database_scope: cfg.database.clone(),
        target_schema_scope: Some("public".into()),
        snapshots: vec![],
        has_complete_target_dependency_catalog: false,
        target_dependency_schema_scope: None,
        object_snapshots: vec![],
        object_dependency_catalog: vec![],
        target_table_identity_catalog: vec![],
        has_complete_target_object_catalog: false,
        target_mysql_view_scope_context: None,
        source_snapshot: None,
        created_at_ms,
    }
}

/// TTL 仍是 30 分钟：冻结后 29 分钟仍可消费，超过 30 分钟不可消费。
///
/// 同时钉住 `created_at_ms` 的单位是**毫秒**：若把秒当毫秒用（阈值放大 1000 倍），
/// 本用例的「29 分钟仍可消费」断言会红。
#[tokio::test]
async fn reviewed_plan_ttl_is_thirty_minutes_over_unix_milliseconds() {
    let store = TestPlanRepository::default();
    let scope = scope("org-a");
    let cfg = config();
    let handle = handle();

    // 冻结后 29 分钟：仍在有效期内，可消费。
    let fresh = reviewed_plan("fresh", now_ms().saturating_sub(29 * 60 * 1000));
    store
        .put(&scope, "fresh", fresh.clone())
        .await
        .expect("put fresh plan");
    assert!(
        consume(
            &store,
            &scope,
            &fresh.plan,
            "session",
            &handle,
            &cfg,
            cfg.database.as_deref(),
            Some("public"),
        )
        .await
        .is_ok(),
        "冻结后 29 分钟必须仍可消费（TTL = 30 分钟）"
    );

    // 冻结后 31 分钟：已过期，不可消费，过期文案逐字不变。
    let stale = reviewed_plan("stale", now_ms().saturating_sub(31 * 60 * 1000));
    store
        .put(&scope, "stale", stale.clone())
        .await
        .expect("put stale plan");
    let error = consume(
        &store,
        &scope,
        &stale.plan,
        "session",
        &handle,
        &cfg,
        cfg.database.as_deref(),
        Some("public"),
    )
    .await
    .expect_err("超过 30 分钟必须不可消费");
    assert_eq!(error, "Plan expired; compare again", "过期文案必须逐字不变");
}

/// `created_at_ms` 必须是 **Unix 毫秒**（不是秒）：秒级时间戳约 1.7e9，
/// 毫秒级约 1.7e12；且必须贴近冻结时刻。
#[tokio::test]
async fn reviewed_plan_freeze_stamps_creation_in_unix_milliseconds() {
    let store = TestPlanRepository::default();
    let scope = scope("org-a");
    let cfg = config();
    let mut plan = plan();
    freeze(
        &store,
        &scope,
        &mut plan,
        "session".into(),
        &handle(),
        &cfg,
        vec![],
        cfg.database.clone(),
        Some("public".into()),
    )
    .await
    .expect("freeze plan");
    let id = plan.plan_id.clone().expect("frozen plan carries its id");
    let stored = store
        .get(&scope, &id)
        .await
        .expect("read plan")
        .expect("plan present");
    let now = now_ms();
    assert!(
        stored.created_at_ms > now.saturating_sub(5_000) && stored.created_at_ms <= now + 5_000,
        "created_at_ms 必须贴近冻结时刻，实际 {}（现在 {}）",
        stored.created_at_ms,
        now
    );
    assert!(
        stored.created_at_ms > 1_000_000_000_000,
        "秒级时间戳约 1.7e9，毫秒级约 1.7e12；实际 {}",
        stored.created_at_ms
    );
}

/// 【刻意的失效关闭取舍】系统时钟**回拨**时，本该过期的计划被判**已过期**，
/// 而不是复活。
///
/// 改造前用单调钟（`Instant::elapsed`），回拨免疫；端口化后冻结时刻改为
/// Unix 毫秒（落盘实现需要可持久化的时刻），此时 `now < created_at_ms` 只能
/// 意味着系统时钟被回拨过、时间基准不可信。
///
/// 本实现**不**把这种情形当成「刚刚冻结」——那会让本该过期的计划复活
/// （**失效开放**）；规则层直接判**已过期**（**失效关闭**），用户重新比对即可恢复。
/// 这条用例把该取舍钉住：它断言的是**修复后**的行为。
#[tokio::test]
async fn reviewed_plan_clock_rollback_expires_the_plan_instead_of_reinstating_it() {
    let store = InMemoryReviewedPlanRepository::new();
    let scope = scope("org-a");
    let cfg = config();
    let handle = handle();
    // 冻结时刻落在「现在」之后 1 小时 ⇒ 等价于系统时钟回拨 1 小时。
    let frozen = reviewed_plan("rolled-back", now_ms() + 60 * 60 * 1000);
    store
        .put(&scope, "rolled-back", frozen.clone())
        .await
        .expect("put plan");
    let error = consume(
        &store,
        &scope,
        &frozen.plan,
        "session",
        &handle,
        &cfg,
        cfg.database.as_deref(),
        Some("public"),
    )
    .await
    .expect_err("时钟回拨时计划必须被判过期，不得复活");
    assert_eq!(
        error, "Plan expired; compare again",
        "时钟回拨必须按「已过期」处理（失效关闭），不得让本该过期的计划复活"
    );
}

/// 系统时钟**前跳**时，刚冻结的计划立刻过期——同样是**失效关闭**方向。
///
/// 用户重新比对即可恢复：前跳不会让**不该**有效的计划变得有效。
#[tokio::test]
async fn reviewed_plan_clock_forward_jump_expires_the_plan() {
    let store = InMemoryReviewedPlanRepository::new();
    let scope = scope("org-a");
    let cfg = config();
    let handle = handle();
    // 冻结时刻落在「现在」之前 31 分钟 ⇒ 等价于冻结后系统时钟前跳了 31 分钟。
    let frozen = reviewed_plan("jumped", now_ms().saturating_sub(31 * 60 * 1000));
    store
        .put(&scope, "jumped", frozen.clone())
        .await
        .expect("put plan");
    let error = consume(
        &store,
        &scope,
        &frozen.plan,
        "session",
        &handle,
        &cfg,
        cfg.database.as_deref(),
        Some("public"),
    )
    .await
    .expect_err("时钟前跳后计划必须立刻过期");
    assert_eq!(
        error, "Plan expired; compare again",
        "时钟前跳必须按「已过期」处理（失效关闭）"
    );
}

/// 回拨场景下的淘汰顺序：刚冻结的**有效**计划不会被淘汰，已失效的条目会被清掉。
///
/// 模拟：系统时钟回拨 1 小时。回拨**前**冻结的条目，其 `created_at_ms` 相对
/// 「现在」落在**未来** ⇒ 规则层判**已失效**（`now < created_at_ms` ⇒ 时间基准
/// 不可信）；回拨**后**新冻结的那条 `created_at_ms` = 现在 ⇒ 仍然**有效**。
///
/// 修复前：存储层按**年龄**裁剪（回拨条目的年龄恒为 0 ⇒ **裁不掉**它们），
/// 淘汰时按 `created_at_ms` 取最小 ⇒ 刚冻结的有效计划 `created_at_ms` 反而**更小**
/// ⇒ **先被淘汰**；回拨前已失效的条目反而留下。修复后：裁剪改用 `is_expired()`
/// ⇒ 失效条目先被清掉，有效计划留下。
#[tokio::test]
async fn in_memory_store_clears_invalid_entries_before_evicting_valid_ones_after_clock_rollback() {
    let store = InMemoryReviewedPlanRepository::new();
    let scope = scope("org-a");
    let now = now_ms();
    // 回拨**前**冻结 ⇒ 相对回拨后的「现在」落在未来 ⇒ 已失效。
    let rolled_back = now + 60 * 60 * 1000;
    for index in 0..REVIEWED_PLAN_MAX_ENTRIES as u64 {
        let id = format!("stale-{index}");
        store
            .put(&scope, &id, reviewed_plan(&id, rolled_back))
            .await
            .expect("put stale plan");
    }
    // 回拨**后**新冻结 ⇒ 有效。
    store
        .put(&scope, "fresh", reviewed_plan("fresh", now))
        .await
        .expect("put fresh plan");
    // 再写一条触发淘汰。
    store
        .put(&scope, "trigger", reviewed_plan("trigger", now))
        .await
        .expect("put trigger plan");

    assert!(
        store
            .get(&scope, "fresh")
            .await
            .expect("read plan")
            .is_some(),
        "回拨后刚冻结的有效计划不得被淘汰"
    );
    assert!(
        store
            .get(&scope, "trigger")
            .await
            .expect("read plan")
            .is_some(),
        "触发淘汰的有效计划必须留下"
    );
    assert!(
        store
            .get(&scope, "stale-0")
            .await
            .expect("read plan")
            .is_none(),
        "已失效的条目必须被清掉"
    );
    assert_eq!(store.len().await, 2, "失效条目被清掉后，存储里只剩有效条目");
}

// ─────────────────────────────────────────────────────────────────────────────
// 一次性消费：get 与 remove 之间存在竞态窗口
// ─────────────────────────────────────────────────────────────────────────────

/// 把 `get` 与 `remove` 之间的窗口撑开的存储替身。
///
/// `get` 读到条目后**先做一次会合**：自旋让出，直到两个消费请求都读到计划，
/// 于是两者都会通过规则校验；此时只有一个 `remove` 能成功。
///
/// 会合是**有界**的（最多让出固定次数）：若实现只让一个请求走到 `get`
/// （例如把 `remove` 挪到 `get` 之前），另一个请求就永远不会到达，
/// 此时必须**超时退出**而不是把测试挂死——挂死会让「变异应当变红」退化成
/// 「变异让 CI 永远跑不完」。
struct BarrierPlanRepository {
    plans: Mutex<HashMap<(String, String), ReviewedPlan>>,
    reads: std::sync::atomic::AtomicUsize,
}

/// 会合的自旋上限。超过即放弃等待（测试随后按断言失败，而不是挂死）。
const RENDEZVOUS_MAX_YIELDS: usize = 512;

impl BarrierPlanRepository {
    fn bucket(scope: &OrganizationScope, plan_id: &str) -> (String, String) {
        (
            scope.organization_id().as_str().to_owned(),
            plan_id.to_owned(),
        )
    }

    /// 等另一个消费请求读到计划；到不了就放弃（不挂死）。
    async fn rendezvous(&self) {
        for _ in 0..RENDEZVOUS_MAX_YIELDS {
            if self.reads.load(std::sync::atomic::Ordering::SeqCst) >= 2 {
                return;
            }
            tokio::task::yield_now().await;
        }
    }
}

#[async_trait]
impl ReviewedPlanRepository for BarrierPlanRepository {
    async fn put(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
        plan: ReviewedPlan,
    ) -> Result<(), ReviewedPlanRepositoryError> {
        self.plans
            .lock()
            .await
            .insert(Self::bucket(scope, plan_id), plan);
        Ok(())
    }

    async fn get(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
    ) -> Result<Option<ReviewedPlan>, ReviewedPlanRepositoryError> {
        let found = self
            .plans
            .lock()
            .await
            .get(&Self::bucket(scope, plan_id))
            .cloned();
        // 撑开 get 与 remove 之间的竞态窗口：两个消费请求都读到之后才继续。
        self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.rendezvous().await;
        Ok(found)
    }

    async fn remove(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
    ) -> Result<bool, ReviewedPlanRepositoryError> {
        Ok(self
            .plans
            .lock()
            .await
            .remove(&Self::bucket(scope, plan_id))
            .is_some())
    }
}

/// 并发消费：两个请求都读到计划、都通过校验，但**只有一个**能部署成功。
///
/// 失败方必须报「已被执行」——不得报「不存在」，也不得两个都成功。
#[tokio::test]
async fn reviewed_plan_concurrent_consume_admits_exactly_one_consumer() {
    let store = BarrierPlanRepository {
        plans: Mutex::new(HashMap::new()),
        reads: std::sync::atomic::AtomicUsize::new(0),
    };
    let scope = scope("org-a");
    let cfg = config();
    let handle = handle();
    let mut plan = plan();
    freeze(
        &store,
        &scope,
        &mut plan,
        "session".into(),
        &handle,
        &cfg,
        vec![],
        cfg.database.clone(),
        Some("public".into()),
    )
    .await
    .expect("freeze plan");

    let (a, b) = tokio::join!(
        consume(
            &store,
            &scope,
            &plan,
            "session",
            &handle,
            &cfg,
            cfg.database.as_deref(),
            Some("public"),
        ),
        consume(
            &store,
            &scope,
            &plan,
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
    let loser = if a.is_ok() { &b } else { &a };
    assert_eq!(
        loser.as_ref().unwrap_err(),
        "Plan already executed",
        "落败方必须报「已被执行」"
    );
}

/// 校验失败**不**烧掉计划：端点不匹配的失败消费之后，原计划仍可正常消费。
#[tokio::test]
async fn reviewed_plan_failed_validation_does_not_burn_the_plan() {
    let store = TestPlanRepository::default();
    let scope = scope("org-a");
    let cfg = config();
    let handle = handle();
    let mut plan = plan();
    freeze(
        &store,
        &scope,
        &mut plan,
        "session".into(),
        &handle,
        &cfg,
        vec![],
        cfg.database.clone(),
        Some("public".into()),
    )
    .await
    .expect("freeze plan");

    // 端点不匹配 ⇒ 校验失败。
    let mut wrong = cfg.clone();
    wrong.database = Some("other_database".into());
    let error = consume(
        &store,
        &scope,
        &plan,
        "session",
        &handle,
        &wrong,
        wrong.database.as_deref(),
        Some("public"),
    )
    .await
    .expect_err("端点不匹配必须失败");
    assert_eq!(
        error, "Target identity changed after review; compare again",
        "失败文案必须逐字不变"
    );

    // 计划必须仍在：改回正确目标后仍能部署。
    let still_there = store
        .get(&scope, plan.plan_id.as_ref().expect("plan id"))
        .await
        .expect("read plan")
        .expect("校验失败不得烧掉计划");
    assert!(still_there.plan == plan, "计划内容必须未被改动");
    assert!(
        consume(
            &store,
            &scope,
            &plan,
            "session",
            &handle,
            &cfg,
            cfg.database.as_deref(),
            Some("public"),
        )
        .await
        .is_ok(),
        "校验失败后计划必须仍可消费"
    );
}

/// 端口的 `get` 与 `remove` 是两次调用，中间存在竞态窗口。
///
/// 实现必须用**取出的那一份**做校验，并把**同一份**返回给部署方：
/// 若实现改成「校验之后再取一次」，就会校验到另一份内容，部署方拿到的
/// 也就不是复核过的那份计划。
///
/// 替身：第一次读返回真实计划，之后的读返回**被篡改**的副本。
struct SecondReadTamperedRepository {
    plans: Mutex<HashMap<(String, String), ReviewedPlan>>,
    reads: Mutex<HashMap<(String, String), usize>>,
}

impl Default for SecondReadTamperedRepository {
    fn default() -> Self {
        Self {
            plans: Mutex::new(HashMap::new()),
            reads: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl ReviewedPlanRepository for SecondReadTamperedRepository {
    async fn put(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
        plan: ReviewedPlan,
    ) -> Result<(), ReviewedPlanRepositoryError> {
        self.plans.lock().await.insert(
            (
                scope.organization_id().as_str().to_owned(),
                plan_id.to_owned(),
            ),
            plan,
        );
        Ok(())
    }

    async fn get(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
    ) -> Result<Option<ReviewedPlan>, ReviewedPlanRepositoryError> {
        let key = (
            scope.organization_id().as_str().to_owned(),
            plan_id.to_owned(),
        );
        let attempt = {
            let mut reads = self.reads.lock().await;
            let counter = reads.entry(key.clone()).or_insert(0);
            *counter += 1;
            *counter
        };
        let mut found = self.plans.lock().await.get(&key).cloned();
        if attempt >= 2 {
            if let Some(plan) = found.as_mut() {
                plan.plan
                    .warnings
                    .push("tampered after the first read".into());
            }
        }
        Ok(found)
    }

    async fn remove(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
    ) -> Result<bool, ReviewedPlanRepositoryError> {
        Ok(self
            .plans
            .lock()
            .await
            .remove(&(
                scope.organization_id().as_str().to_owned(),
                plan_id.to_owned(),
            ))
            .is_some())
    }
}

/// `consume` 校验的必须是它**取出的那一份**，返回给部署方的也必须是同一份。
#[tokio::test]
async fn reviewed_plan_consume_validates_the_copy_it_read() {
    let store = SecondReadTamperedRepository::default();
    let scope = scope("org-a");
    let cfg = config();
    let handle = handle();
    let mut frozen = plan();
    freeze(
        &store,
        &scope,
        &mut frozen,
        "session".into(),
        &handle,
        &cfg,
        vec![],
        cfg.database.clone(),
        Some("public".into()),
    )
    .await
    .expect("freeze plan");

    let consumed = consume(
        &store,
        &scope,
        &frozen,
        "session",
        &handle,
        &cfg,
        cfg.database.as_deref(),
        Some("public"),
    )
    .await
    .expect("consume 必须校验它取出的那一份");
    assert!(
        consumed.plan == frozen,
        "返回给部署方的必须是复核过的那一份计划"
    );

    // 替身生效：第二次读取确实返回了被篡改的副本。
    // 用另一条计划，避免受上面那次消费（已删除条目）影响。
    let mut other = plan();
    freeze(
        &store,
        &scope,
        &mut other,
        "session".into(),
        &handle,
        &cfg,
        vec![],
        cfg.database.clone(),
        Some("public".into()),
    )
    .await
    .expect("freeze another plan");
    let other_id = other.plan_id.clone().expect("frozen plan carries its id");
    let first = store
        .get(&scope, &other_id)
        .await
        .expect("read plan")
        .expect("plan present");
    let second = store
        .get(&scope, &other_id)
        .await
        .expect("read plan")
        .expect("plan present");
    assert!(first.plan == other, "第一次读取必须返回真实计划");
    assert!(
        second.plan != other,
        "替身第二次读取必须返回被篡改的副本，否则本用例不承重"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 独立探针（Tester 第 3 轮终验）：回拨场景下的裁剪谓词与淘汰顺序
//
// 这几条刻意与编码代理那条用例**形状不同**：
//   * 编码代理的用例用「128 条失效条目 + 2 条有效条目」铺满容量上限；
//   * 本组探针**不**触容量淘汰（除探针 B 外），只隔离「裁剪谓词」一个变量，
//     并额外覆盖「跨组织互不影响」与「谓词与规则层逐条一致」。
// ─────────────────────────────────────────────────────────────────────────────

/// 【探针 A】有效计划的 `created_at_ms` 比失效条目**更小**时，有效计划必须留下。
///
/// 这正是「淘汰顺序反转」的核心：按**年龄**裁剪时，回拨条目（未来时间戳）的
/// 年龄恒为 0 ⇒ **裁不掉**它们；而有效条目的 `created_at_ms` 反而**更小**
/// ⇒ 淘汰排序（`min_by_key(created_at_ms)`）会**先**淘汰它。
///
/// 本探针只放 1 条有效 + 1 条失效 + 1 条触发写入，**不**触容量淘汰，
/// 因此失败只可能来自裁剪谓词本身。
#[tokio::test]
async fn probe_valid_plan_survives_when_its_timestamp_is_older_than_an_invalid_entry() {
    let store = InMemoryReviewedPlanRepository::new();
    let scope = scope("org-a");
    let now = now_ms();

    // 有效：冻结于 1 秒前。
    let valid = reviewed_plan("valid", now.saturating_sub(1_000));
    store
        .put(&scope, "valid", valid.clone())
        .await
        .expect("put valid plan");
    // 失效：冻结时刻落在「现在」之后 1 小时 ⇒ 等价于系统时钟回拨 ⇒ 规则层判已失效。
    let invalid = reviewed_plan("invalid", now + 60 * 60 * 1000);
    store
        .put(&scope, "invalid", invalid.clone())
        .await
        .expect("put invalid plan");
    // 再写一条，触发一次「写入时的清理」。
    store
        .put(
            &scope,
            "trigger",
            reviewed_plan("trigger", now.saturating_sub(500)),
        )
        .await
        .expect("put trigger plan");

    assert!(
        store
            .get(&scope, "valid")
            .await
            .expect("read plan")
            .is_some(),
        "有效计划必须留下：它的 created_at_ms 比失效条目更小，淘汰排序不得反转"
    );
    assert!(
        store
            .get(&scope, "invalid")
            .await
            .expect("read plan")
            .is_none(),
        "失效条目必须在写入时被清掉（回拨条目不得靠「年龄恒为 0」躲过裁剪）"
    );
    assert!(
        store
            .get(&scope, "trigger")
            .await
            .expect("read plan")
            .is_some(),
        "触发写入的有效计划必须留下"
    );
}

/// 【探针 B】容量上限仍然是 128：写满时淘汰**最旧**的一条，新写入的必须留下。
///
/// 排序键全部落在**过去**（冻结于 1～128 毫秒前），因此这些条目**都有效**
/// （远未到 30 分钟 TTL），必然参与淘汰排序而不是被裁掉。
#[tokio::test]
async fn probe_capacity_evicts_the_oldest_valid_entry_and_keeps_the_new_one() {
    let store = InMemoryReviewedPlanRepository::new();
    let scope = scope("org-a");
    let now = now_ms();
    let count = REVIEWED_PLAN_MAX_ENTRIES as u64;
    for index in 0..count {
        let id = format!("entry-{index}");
        // index 越小 ⇒ 冻结越早（但仍是过去时刻 ⇒ 有效）。
        let created = now.saturating_sub(count - index);
        store
            .put(&scope, &id, reviewed_plan(&id, created))
            .await
            .expect("put entry");
    }
    assert_eq!(
        store.len().await,
        REVIEWED_PLAN_MAX_ENTRIES,
        "写满上限时不得提前裁剪"
    );

    store
        .put(&scope, "newest", reviewed_plan("newest", now))
        .await
        .expect("put newest plan");

    assert!(
        store
            .get(&scope, "entry-0")
            .await
            .expect("read plan")
            .is_none(),
        "最旧的条目必须被淘汰"
    );
    assert!(
        store
            .get(&scope, "entry-1")
            .await
            .expect("read plan")
            .is_some(),
        "次旧的条目必须留下（只淘汰一条）"
    );
    assert!(
        store
            .get(&scope, "newest")
            .await
            .expect("read plan")
            .is_some(),
        "新写入的条目必须留下"
    );
    assert_eq!(
        store.len().await,
        REVIEWED_PLAN_MAX_ENTRIES,
        "条目总数必须仍被压在上限内"
    );
}

/// 【探针 C】容量上限：130 条**过去**时刻的有效条目 ⇒ 恰好保留最新的 128 条。
///
/// 与宿主那条用例同形，但打在**包内生产实现**上（宿主那条打的是宿主委派层）。
#[tokio::test]
async fn probe_capacity_keeps_the_newest_128_of_130_past_dated_entries() {
    let store = InMemoryReviewedPlanRepository::new();
    let scope = scope("org-a");
    let now = now_ms();
    for index in 0..130u64 {
        let id = format!("plan-{index}");
        store
            .put(
                &scope,
                &id,
                reviewed_plan(&id, now.saturating_sub(130 - index)),
            )
            .await
            .expect("put plan");
    }
    assert!(
        store
            .get(&scope, "plan-0")
            .await
            .expect("read plan")
            .is_none(),
        "最旧的条目必须被裁剪"
    );
    assert!(
        store
            .get(&scope, "plan-1")
            .await
            .expect("read plan")
            .is_none(),
        "次旧的条目必须被裁剪"
    );
    assert!(
        store
            .get(&scope, "plan-2")
            .await
            .expect("read plan")
            .is_some(),
        "第 3 条必须仍在（上限正是 128，而不是更小的数）"
    );
    assert!(
        store
            .get(&scope, "plan-129")
            .await
            .expect("read plan")
            .is_some(),
        "最新的条目必须仍在"
    );
    assert_eq!(store.len().await, REVIEWED_PLAN_MAX_ENTRIES);
}

/// 【探针 D】裁剪是**逐条目**的：一个组织的写入不得清掉另一个组织的有效条目。
#[tokio::test]
async fn probe_rollback_trimming_does_not_touch_other_organizations() {
    let store = InMemoryReviewedPlanRepository::new();
    let first = scope("org-a");
    let second = scope("org-b");
    let now = now_ms();

    store
        .put(
            &first,
            "rolled-back",
            reviewed_plan("rolled-back", now + 60 * 60 * 1000),
        )
        .await
        .expect("put rolled-back plan");
    store
        .put(&second, "valid", reviewed_plan("valid", now))
        .await
        .expect("put valid plan");
    store
        .put(&first, "fresh", reviewed_plan("fresh", now))
        .await
        .expect("put fresh plan");

    assert!(
        store
            .get(&first, "rolled-back")
            .await
            .expect("read plan")
            .is_none(),
        "本组织的失效条目必须被清掉"
    );
    assert!(
        store
            .get(&second, "valid")
            .await
            .expect("read plan")
            .is_some(),
        "别的组织的有效条目不得被本组织的写入清掉"
    );
}

/// 【探针 E】存储层的裁剪谓词与规则层的 [`ReviewedPlan::is_expired`] **逐条一致**。
///
/// 对照物是规则层自己的判定：凡是被规则层判「已过期」的条目，写入后必须被裁掉；
/// 凡是被规则层判「有效」的条目，写入后必须留下。
///
/// 若存储层改用按**年龄**裁剪（`age() < REVIEWED_PLAN_TTL`），「回拨」那条
/// （未来时间戳、年龄恒为 0）会与规则层判定**不一致** ⇒ 本探针变红。
#[tokio::test]
async fn probe_store_trim_predicate_matches_the_rule_layer_expiry_decision() {
    let store = InMemoryReviewedPlanRepository::new();
    let scope = scope("org-a");
    let now = now_ms();

    let cases: Vec<(&str, u64)> = vec![
        ("fresh", now.saturating_sub(1_000)),
        ("near-ttl", now.saturating_sub(29 * 60 * 1000)),
        ("past-ttl", now.saturating_sub(31 * 60 * 1000)),
        ("rolled-back", now + 60 * 60 * 1000),
    ];
    for (id, created) in &cases {
        store
            .put(&scope, id, reviewed_plan(id, *created))
            .await
            .expect("put plan");
    }
    // 再写一条，触发一次清理，让上面四条都被同一谓词过一遍。
    store
        .put(&scope, "trigger", reviewed_plan("trigger", now))
        .await
        .expect("put trigger plan");

    for (id, created) in &cases {
        let rule_says_expired = reviewed_plan(id, *created).is_expired();
        let present = store.get(&scope, id).await.expect("read plan").is_some();
        assert_eq!(
            present, !rule_says_expired,
            "存储层裁剪判定必须与规则层一致：{id}（规则层判已过期={rule_says_expired}）"
        );
    }
}

/// 【探针 F】**未来**时刻的条目会在下一次写入时被裁掉。
///
/// 这条钉住「容量/淘汰语义的用例为什么不能用未来时间戳做排序键」：
/// 未来时刻意味着「系统时钟相对冻结时刻被回拨过」⇒ 规则层判**已失效**
/// ⇒ 存储层的写入清理会把它裁掉 ⇒ 每次写入都把上一条裁掉，条目数**达不到**
/// 容量上限，淘汰逻辑根本不会执行。
///
/// 因此容量/淘汰用例必须用**过去**时刻（有效条目）做排序键。
#[tokio::test]
async fn probe_future_dated_entries_are_trimmed_by_the_next_put() {
    let store = InMemoryReviewedPlanRepository::new();
    let scope = scope("org-a");
    let future = now_ms() + 60 * 60 * 1000;

    store
        .put(&scope, "a", reviewed_plan("a", future))
        .await
        .expect("put a");
    store
        .put(&scope, "b", reviewed_plan("b", future))
        .await
        .expect("put b");

    assert!(
        store.get(&scope, "a").await.expect("read plan").is_none(),
        "未来时刻的条目必须在下一次写入时被裁掉：容量用例不能用未来时间戳做排序键"
    );
    assert!(
        store.get(&scope, "b").await.expect("read plan").is_some(),
        "最后写入的那条仍在（它还没有被下一次写入清理）"
    );
}

/// 【探针 G】越组织消费与「计划不存在」的错误文案**逐字相同**。
///
/// 两者都走 `consume` 里同一条「`get` 返回 `None`」的路径，必须落在**同一个**
/// 字面量上。不得给越组织访问换一个更「具体」的措辞——那会泄漏「存在但无权」
/// （CM-05）。
#[tokio::test]
async fn probe_foreign_organization_and_missing_plan_produce_identical_messages() {
    let store = InMemoryReviewedPlanRepository::new();
    let owner = scope("org-a");
    let other = scope("org-b");
    let cfg = config();
    let handle = handle();
    let mut plan = plan();
    freeze(
        &store,
        &owner,
        &mut plan,
        "session".into(),
        &handle,
        &cfg,
        vec![],
        cfg.database.clone(),
        Some("public".into()),
    )
    .await
    .expect("freeze plan");

    // 1) 越组织消费：计划确实存在，只是组织不对。
    let foreign = consume(
        &store,
        &other,
        &plan,
        "session",
        &handle,
        &cfg,
        cfg.database.as_deref(),
        Some("public"),
    )
    .await
    .expect_err("越组织消费必须失败");

    // 2) 计划根本不存在（本组织，但 id 不存在）。
    let mut missing = plan.clone();
    missing.plan_id = Some("no-such-plan".into());
    let absent = consume(
        &store,
        &owner,
        &missing,
        "session",
        &handle,
        &cfg,
        cfg.database.as_deref(),
        Some("public"),
    )
    .await
    .expect_err("不存在的计划必须失败");

    assert_eq!(
        foreign, absent,
        "越组织与「不存在」的错误文案必须逐字相同（不得泄漏「存在但无权」）"
    );
    assert_eq!(
        foreign, "Plan expired or already executed; compare again",
        "文案必须逐字不变"
    );
}
