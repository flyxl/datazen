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
