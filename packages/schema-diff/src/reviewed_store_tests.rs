//! 评审计划**存储端口**的用例：一次性消费语义 + 组织隔离。
//!
//! 规则（身份指纹、快照复核）的用例在 `reviewed_tests.rs`；这里只钉存储契约：
//! 「一次部署只消费一次」与「越组织读不到」。

use super::*;
use async_trait::async_trait;
use datazen_platform_api::id::OrganizationId;
use std::collections::HashMap;
use tokio::sync::Mutex;

/// 进程内存的测试存储：按组织分桶。
///
/// 越组织访问按「不存在」处理——取不到、也删不掉别人的计划。
#[derive(Default)]
struct TestPlanStore {
    plans: Mutex<HashMap<(String, String), ReviewedPlan>>,
}

impl TestPlanStore {
    fn bucket(scope: &OrganizationScope, plan_id: &str) -> (String, String) {
        (
            scope.organization_id().as_str().to_owned(),
            plan_id.to_owned(),
        )
    }
}

#[async_trait]
impl ReviewedPlanStore for TestPlanStore {
    async fn put(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
        plan: ReviewedPlan,
    ) -> Result<(), ReviewedPlanStoreError> {
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
    ) -> Result<Option<ReviewedPlan>, ReviewedPlanStoreError> {
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
    ) -> Result<bool, ReviewedPlanStoreError> {
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
    let store = TestPlanStore::default();
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
    let store = TestPlanStore::default();
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
    let store = TestPlanStore::default();
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
#[tokio::test]
async fn reviewed_plan_store_hides_plans_from_other_organizations() {
    let store = TestPlanStore::default();
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
#[tokio::test]
async fn reviewed_plan_consume_rejects_foreign_organization_scope() {
    let store = TestPlanStore::default();
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

/// 跨组织写入不落数据：本组织读不到「别的组织写进来」的计划。
#[tokio::test]
async fn reviewed_plan_store_drops_writes_from_other_organizations() {
    let store = TestPlanStore::default();
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
    let store = TestPlanStore::default();
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
    let store = TestPlanStore::default();
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

/// 【已知退化】系统时钟**回拨**会让本该过期的计划重新有效。
///
/// 改造前用单调钟（`Instant::elapsed`），回拨免疫；端口化后冻结时刻改为
/// Unix 毫秒，`age()` 用 `saturating_sub`：`created_at_ms` 落在「现在」之后时
/// age 归零 ⇒ 计划被判为「刚刚冻结」。本用例把该行为钉住，使取舍显式可见。
#[tokio::test]
async fn reviewed_plan_clock_rollback_reinstates_an_expired_plan() {
    let store = TestPlanStore::default();
    let scope = scope("org-a");
    let cfg = config();
    let handle = handle();
    // 冻结时刻落在「现在」之后 1 小时 ⇒ 等价于系统时钟回拨 1 小时。
    let frozen = reviewed_plan("rolled-back", now_ms() + 60 * 60 * 1000);
    store
        .put(&scope, "rolled-back", frozen.clone())
        .await
        .expect("put plan");
    assert!(
        consume(
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
        .is_ok(),
        "时钟回拨后 age 归零：本该过期的计划重新有效（已知退化）"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 一次性消费：get 与 remove 之间存在竞态窗口
// ─────────────────────────────────────────────────────────────────────────────

/// 把 `get` 与 `remove` 之间的窗口撑开的存储替身。
///
/// `get` 读到条目后**先等一道屏障**：两个消费请求都读到计划之后才允许继续，
/// 于是两者都会通过规则校验；此时只有一个 `remove` 能成功。
struct BarrierPlanStore {
    plans: Mutex<HashMap<(String, String), ReviewedPlan>>,
    barrier: tokio::sync::Barrier,
}

impl BarrierPlanStore {
    fn bucket(scope: &OrganizationScope, plan_id: &str) -> (String, String) {
        (
            scope.organization_id().as_str().to_owned(),
            plan_id.to_owned(),
        )
    }
}

#[async_trait]
impl ReviewedPlanStore for BarrierPlanStore {
    async fn put(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
        plan: ReviewedPlan,
    ) -> Result<(), ReviewedPlanStoreError> {
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
    ) -> Result<Option<ReviewedPlan>, ReviewedPlanStoreError> {
        let found = self
            .plans
            .lock()
            .await
            .get(&Self::bucket(scope, plan_id))
            .cloned();
        // 撑开 get 与 remove 之间的竞态窗口：两个消费请求都读到之后才继续。
        self.barrier.wait().await;
        Ok(found)
    }

    async fn remove(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
    ) -> Result<bool, ReviewedPlanStoreError> {
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
    let store = BarrierPlanStore {
        plans: Mutex::new(HashMap::new()),
        barrier: tokio::sync::Barrier::new(2),
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
    let store = TestPlanStore::default();
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
