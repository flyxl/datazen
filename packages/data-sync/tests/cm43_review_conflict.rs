//! CM-43：review 之后的行冲突 —— 默认策略拒绝受影响批次（§7 冲突裁决）。
//!
//! 场景：prepare 冻结 ChangeSet（review 完成）→ 另一连接改写目标行 → apply 执行。
//! 断言：冲突被识别为 stale-value 冲突（不做静默覆盖）、已提交批次保留、后续批次
//! 不开始、终态显式部分提交；同时结构漂移仍在 apply 先行拒绝，且重新比较后可继续。

#[macro_use]
mod support;

use std::collections::HashMap;
use std::sync::Arc;

use datazen_data_sync::job::{ApplySpec, DataSyncHandler, DataSyncHost, PrepareSpec};
use datazen_data_sync::model::{Row, SyncOptions, TableMapping};
use datazen_driver_api::Value;
use datazen_platform_api::dto::execution::{EffectOutcome, ExecutionErrorCode};
use datazen_platform_api::id::StageId;
use datazen_runtime::job::{CancelToken, JobHandler, StageOutcome, StageSpec, StageTerminal};

use support::host::{source_endpoint, target_endpoint, FakeHost, SchemaMode};
use support::rows::{int, row, rows_equal, text, PLAN_ID, TABLE};

const REVISION: u64 = 1;

fn old_row(n: i64) -> Row {
    row(vec![int(n), text(&format!("old-{n}"))])
}

fn new_row(n: i64) -> Row {
    row(vec![int(n), text(&format!("new-{n}"))])
}

fn foreign_row(n: i64) -> Row {
    row(vec![int(n), text("written-by-other-connection")])
}

fn key(n: i64) -> Vec<Value> {
    vec![int(n)]
}

fn prepare_spec() -> PrepareSpec {
    PrepareSpec {
        source: source_endpoint(),
        target: target_endpoint(),
        mappings: vec![TableMapping::auto(TABLE)],
        options: SyncOptions::default(),
        filters: HashMap::new(),
        plan_id: Some(PLAN_ID.to_string()),
    }
}

fn apply_spec(batch_size: u32) -> ApplySpec {
    ApplySpec {
        plan_id: PLAN_ID.to_string(),
        selection_revision: REVISION,
        options: SyncOptions {
            batch_size,
            ..SyncOptions::default()
        },
    }
}

fn stage(kind: &str) -> StageSpec {
    StageSpec {
        stage_id: StageId::new(kind),
        kind: kind.to_string(),
        depends_on: Vec::new(),
    }
}

/// 源 3 行已更新、目标 3 行仍是旧值，且目标已提交态已预置。
fn seeded_host() -> Arc<FakeHost> {
    let fake = FakeHost::new();
    fake.source_rows(TABLE, vec![new_row(1), new_row(2), new_row(3)]);
    fake.target_rows(TABLE, vec![old_row(1), old_row(2), old_row(3)]);
    fake.seed_target(vec![
        (key(1), old_row(1)),
        (key(2), old_row(2)),
        (key(3), old_row(3)),
    ]);
    Arc::new(fake)
}

async fn run_prepare(host: Arc<dyn DataSyncHost>) -> StageOutcome {
    let handler = DataSyncHandler::for_prepare(prepare_spec(), host);
    let outcome = handler
        .run_stage(&stage("prepare"), &CancelToken::new())
        .await
        .expect("prepare stage returns an outcome");
    assert_eq!(outcome.terminal, StageTerminal::Succeeded);
    outcome
}

async fn load_artifact(host: &Arc<dyn DataSyncHost>) -> datazen_data_sync::job::ChangeSetArtifact {
    host.load_artifact(PLAN_ID)
        .await
        .expect("artifact lookup succeeds")
        .expect("prepare stored the ChangeSet")
}

async fn run_apply(host: Arc<dyn DataSyncHost>, batch_size: u32) -> StageOutcome {
    let handler = DataSyncHandler::for_apply(apply_spec(batch_size), host);
    handler
        .run_stage(&stage("apply"), &CancelToken::new())
        .await
        .expect("apply stage returns an outcome")
}

#[tokio::test]
async fn cm43_review_conflict_rejects_affected_batch_and_keeps_committed_ones() {
    let host = seeded_host();
    let dyn_host: Arc<dyn DataSyncHost> = Arc::clone(&host) as Arc<dyn DataSyncHost>;

    // 1) review：prepare 冻结 ChangeSet。
    let prepared = run_prepare(Arc::clone(&dyn_host)).await;
    assert_eq!(prepared.effect_outcome, EffectOutcome::Completed);
    assert!(prepared.commit_boundaries.is_empty());
    assert_eq!(prepared.artifact_ids.len(), 1);
    assert_eq!(prepared.artifact_ids[0].as_str(), "artifact-plan-1");
    assert_eq!(prepared.execution_ids.len(), 1);
    assert_eq!(prepared.progress.converted.get(), 3);

    let artifact = load_artifact(&dyn_host).await;
    assert_eq!(artifact.blocks.len(), 3);
    assert!(
        artifact.blocks.iter().all(|b| b.before.is_some()),
        "UPDATE blocks must carry the target-side before image"
    );

    // review 确认的选择子集 = 冻结块全集。
    host.selection(PLAN_ID, REVISION, artifact.blocks.clone());

    // 2) review 之后，另一连接改写了 key=2 的目标行。
    host.concurrent_write(&key(2), foreign_row(2));

    // 3) apply：默认冲突策略（Abort）拒绝受影响批次。
    let outcome = run_apply(Arc::clone(&dyn_host), 1).await;

    assert_eq!(outcome.terminal, StageTerminal::Failed);
    assert_eq!(outcome.error_code, Some(ExecutionErrorCode::SqlError));
    assert_eq!(
        outcome.effect_outcome,
        EffectOutcome::PartiallyApplied,
        "已提交批次保留 ⇒ 终态必须显式部分提交，而不是 Completed/RolledBack"
    );

    // 批次 0（key=1）已提交并留下 boundary；批次 1 冲突；批次 2 未开始。
    assert_eq!(outcome.commit_boundaries.len(), 1);
    let boundary = &outcome.commit_boundaries[0];
    assert!(
        boundary
            .batch_id
            .as_deref()
            .is_some_and(|id| id.ends_with(":0")),
        "only the first batch may carry a commit boundary, got {:?}",
        boundary.batch_id
    );
    assert_eq!(boundary.stable_target_fingerprint, artifact.digest);
    assert_eq!(boundary.evidence.len(), 3);
    assert_eq!(outcome.progress.committed.get(), 1);
    assert_eq!(outcome.progress.unknown.get(), 0);

    // 冲突行不被静默覆盖，另一连接的写入保留。
    assert_row_eq!(host.committed(&key(2)), Some(foreign_row(2)));
    // 已提交批次的数据保留。
    assert_row_eq!(host.committed(&key(1)), Some(new_row(1)));
    // 未提交的覆盖层已回滚，第三批从未开始。
    assert_eq!(host.pending_writes(), 0);
    assert_row_eq!(host.committed(&key(3)), Some(old_row(3)));
    assert!(host.has_event("begin#1"), "conflicting batch began");
    assert!(
        host.has_event("rollback#1"),
        "conflicting batch rolled back"
    );
    assert!(
        !host.has_event("begin#2"),
        "an unaffected later batch must not start after the abort"
    );

    // 执行器全程绑定同一个目标 Lease（prepare 不取执行器），
    // 且 prepare + apply 共 4 个会话全部归还、无泄漏。
    assert_eq!(host.executor_leases().len(), 1);
    assert_eq!(host.opened().len(), 4);
    assert_eq!(host.closed().len(), 4);
    assert!(host.live().is_empty(), "sessions: {:?}", host.opened());
    assert!(host.leaked().is_empty());
}

#[tokio::test]
async fn cm43_structure_drift_after_review_is_rejected_before_any_write() {
    let host = seeded_host();
    let dyn_host: Arc<dyn DataSyncHost> = Arc::clone(&host) as Arc<dyn DataSyncHost>;

    run_prepare(Arc::clone(&dyn_host)).await;
    let artifact = load_artifact(&dyn_host).await;
    host.selection(PLAN_ID, REVISION, artifact.blocks.clone());

    // review 之后目标结构漂移（多出一列 nullable extra）。
    host.schema_mode(SchemaMode::Drifted);

    let outcome = run_apply(Arc::clone(&dyn_host), 10).await;

    assert_eq!(outcome.terminal, StageTerminal::Failed);
    assert_eq!(outcome.error_code, Some(ExecutionErrorCode::HostRejected));
    assert_eq!(outcome.effect_outcome, EffectOutcome::NotStarted);
    assert!(outcome.commit_boundaries.is_empty());
    assert!(host.executor_leases().is_empty(), "no executor was pinned");
    assert!(host.events().is_empty(), "no statement was executed");
    assert_row_eq!(host.committed(&key(1)), Some(old_row(1)));
}

#[tokio::test]
async fn cm43_recompare_after_conflict_can_continue() {
    let host = seeded_host();
    let dyn_host: Arc<dyn DataSyncHost> = Arc::clone(&host) as Arc<dyn DataSyncHost>;

    run_prepare(Arc::clone(&dyn_host)).await;
    // 另一连接同时改了目标，并被 prepare 再次读到。
    host.concurrent_write(&key(2), foreign_row(2));
    host.target_rows(TABLE, vec![old_row(1), foreign_row(2), old_row(3)]);
    host.source_rows(TABLE, vec![new_row(1), foreign_row(2), new_row(3)]);

    let refreshed = run_prepare(Arc::clone(&dyn_host)).await;
    assert_eq!(refreshed.terminal, StageTerminal::Succeeded);
    let artifact = load_artifact(&dyn_host).await;
    assert_eq!(
        artifact.blocks.len(),
        2,
        "key=2 is now identical on both sides"
    );
    // 产物元数据冻结 review 基线：key=2 两侧一致 ⇒ unchanged_count=1；无过滤 ⇒ None。
    assert_eq!(artifact.table_meta.len(), 1);
    assert_eq!(artifact.table_meta[0].unchanged_count, 1);
    assert!(artifact.table_meta[0].source_filter.is_none());
    host.selection(PLAN_ID, REVISION, artifact.blocks.clone());

    let outcome = run_apply(Arc::clone(&dyn_host), 10).await;
    assert_eq!(outcome.terminal, StageTerminal::Succeeded);
    assert_eq!(outcome.effect_outcome, EffectOutcome::Completed);
    assert_row_eq!(host.committed(&key(1)), Some(new_row(1)));
    assert_row_eq!(host.committed(&key(2)), Some(foreign_row(2)));
}
