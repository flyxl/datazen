//! CM-44：批次取消保留已提交范围（§7 取消与释放）。
//!
//! 场景：3 个批次；批次 1 已提交，批次 2 在事务中被取消，批次 3 从未开始。
//! 断言：批次 1 的已提交范围保留、批次 2 回滚、批次 3 NotStarted，
//! 终态显式「部分提交」，且不被误报成 Completed / RolledBack。

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

use support::host::{source_endpoint, target_endpoint, FakeHost};
use support::rows::{int, row, text, PLAN_ID, TABLE};
use support::{rows_equal, ExecHook};

const REVISION: u64 = 1;
/// 每批一行 ⇒ 3 行 = 3 个批次，批次号 0/1/2。
const BATCH_SIZE: u32 = 1;

fn old_row(n: i64) -> Row {
    row(vec![int(n), text(&format!("old-{n}"))])
}

fn new_row(n: i64) -> Row {
    row(vec![int(n), text(&format!("new-{n}"))])
}

fn key(n: i64) -> Vec<Value> {
    vec![int(n)]
}

fn seeded_host(hook: ExecHook) -> Arc<FakeHost> {
    let fake = FakeHost::new();
    fake.source_rows(TABLE, vec![new_row(1), new_row(2), new_row(3)]);
    fake.target_rows(TABLE, vec![old_row(1), old_row(2), old_row(3)]);
    fake.seed_target(vec![
        (key(1), old_row(1)),
        (key(2), old_row(2)),
        (key(3), old_row(3)),
    ]);
    fake.exec_hook(hook);
    Arc::new(fake)
}

fn stage(kind: &str) -> StageSpec {
    StageSpec {
        stage_id: StageId::new(kind),
        kind: kind.to_string(),
        depends_on: Vec::new(),
    }
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

fn apply_spec() -> ApplySpec {
    apply_spec_with(BATCH_SIZE)
}

fn apply_spec_with(batch_size: u32) -> ApplySpec {
    ApplySpec {
        plan_id: PLAN_ID.to_string(),
        selection_revision: REVISION,
        options: SyncOptions {
            batch_size,
            ..SyncOptions::default()
        },
    }
}

/// prepare 冻结 ChangeSet，并把 review 选中的 3 行注册成 selection。
async fn prepared_host(host: Arc<FakeHost>) -> Arc<dyn DataSyncHost> {
    let dyn_host: Arc<dyn DataSyncHost> = Arc::clone(&host) as Arc<dyn DataSyncHost>;
    let handler = DataSyncHandler::for_prepare(prepare_spec(), Arc::clone(&dyn_host));
    let outcome = handler
        .run_stage(&stage("prepare"), &CancelToken::new())
        .await
        .expect("prepare stage returns an outcome");
    assert_eq!(outcome.terminal, StageTerminal::Succeeded);
    let artifact = dyn_host
        .load_artifact(PLAN_ID)
        .await
        .expect("artifact lookup succeeds")
        .expect("prepare stored the ChangeSet");
    assert_eq!(artifact.blocks.len(), 3, "three rows ⇒ three batches");
    host.selection(PLAN_ID, REVISION, artifact.blocks);
    dyn_host
}

async fn run_apply(host: Arc<dyn DataSyncHost>) -> StageOutcome {
    run_apply_with(host, CancelToken::new()).await
}

async fn run_apply_with(host: Arc<dyn DataSyncHost>, cancel: CancelToken) -> StageOutcome {
    let handler = DataSyncHandler::for_apply(apply_spec(), host);
    handler
        .run_stage(&stage("apply"), &cancel)
        .await
        .expect("apply stage returns an outcome")
}

#[tokio::test]
async fn cm44_cancel_preserves_committed_batch_and_rolls_back_the_blocked_one() {
    // 批次号从 0 起；hook 命中 1 ⇒ 第 2 个批次的第一条 execute 返回 Cancelled。
    let host = seeded_host(ExecHook {
        cancel_on_batch: Some(1),
        ..ExecHook::default()
    });
    let dyn_host = prepared_host(Arc::clone(&host)).await;

    let outcome = run_apply(Arc::clone(&dyn_host)).await;

    assert_eq!(outcome.terminal, StageTerminal::Cancelled);
    assert_eq!(outcome.error_code, Some(ExecutionErrorCode::Cancelled));
    assert_eq!(
        outcome.effect_outcome,
        EffectOutcome::PartiallyApplied,
        "批次 1 已提交 ⇒ 必须显式部分提交，不能是 RolledBack/Completed"
    );

    // 只有批次 0 有提交边界。
    assert_eq!(outcome.commit_boundaries.len(), 1);
    assert!(
        outcome.commit_boundaries[0]
            .batch_id
            .as_deref()
            .is_some_and(|id| id.ends_with(":0")),
        "got {:?}",
        outcome.commit_boundaries[0].batch_id
    );
    assert_eq!(outcome.progress.committed.get(), u64::from(BATCH_SIZE));
    assert_eq!(outcome.progress.unknown.get(), 0);

    // 批次 1 的已提交范围保留。
    assert_row_eq!(host.committed(&key(1)), Some(new_row(1)));
    // 批次 2 被取消并回滚，未提交覆盖层整体丢弃。
    assert_row_eq!(host.committed(&key(2)), Some(old_row(2)));
    // 批次 3 从未开始。
    assert_row_eq!(host.committed(&key(3)), Some(old_row(3)));
    assert_eq!(host.pending_writes(), 0);

    let events = host.events();
    assert!(host.has_event("begin#0"), "{events:?}");
    assert!(host.has_event("commit#0"), "{events:?}");
    assert!(host.has_event("begin#1"), "{events:?}");
    assert!(host.has_event("rollback#1"), "{events:?}");
    assert!(
        !host.has_event("begin#2"),
        "批次 3 必须是 NotStarted：{events:?}"
    );

    // 取消后两端会话仍被归还，泄漏为空。
    assert!(host.live().is_empty(), "{:?}", host.opened());
    assert!(host.leaked().is_empty());
}

#[tokio::test]
async fn cm44_cancel_before_first_commit_rolls_back_everything() {
    let host = seeded_host(ExecHook {
        cancel_on_batch: Some(0),
        ..ExecHook::default()
    });
    let dyn_host = prepared_host(Arc::clone(&host)).await;

    let outcome = run_apply(Arc::clone(&dyn_host)).await;

    assert_eq!(outcome.terminal, StageTerminal::Cancelled);
    assert_eq!(outcome.effect_outcome, EffectOutcome::RolledBack);
    assert!(outcome.commit_boundaries.is_empty());
    assert_eq!(outcome.progress.committed.get(), 0);
    for n in 1..=3 {
        assert_row_eq!(host.committed(&key(n)), Some(old_row(n)));
    }
    assert_eq!(host.pending_writes(), 0);
    assert!(host.has_event("rollback#0"));
    assert!(!host.has_event("begin#1"));
}

#[tokio::test]
async fn cm44_commit_outcome_unknown_is_reported_as_unknown_not_cancelled() {
    // 提交结果不可判时不允许推断回滚，必须报 unknown / ResourceLost。
    let host = seeded_host(ExecHook {
        unknown_commit_on_batch: Some(1),
        ..ExecHook::default()
    });
    let dyn_host = prepared_host(Arc::clone(&host)).await;

    let outcome = run_apply(Arc::clone(&dyn_host)).await;

    assert_eq!(outcome.terminal, StageTerminal::Unknown);
    assert_eq!(outcome.error_code, Some(ExecutionErrorCode::ResourceLost));
    assert_eq!(outcome.effect_outcome, EffectOutcome::Unknown);
    assert_eq!(outcome.progress.unknown.get(), 1);
    assert_eq!(outcome.progress.committed.get(), u64::from(BATCH_SIZE));
    assert_eq!(outcome.commit_boundaries.len(), 1);
    // 不可判批次的暂存改动被丢弃，已提交批次保留。
    assert_eq!(host.pending_writes(), 0);
    assert!(rows_equal(&host.committed(&key(1)), &Some(new_row(1))));
    assert!(host.has_event("discard#1"));
    assert!(!host.has_event("begin#2"));
}

#[tokio::test]
async fn cm44_cancelled_token_stops_before_any_batch_is_executed() {
    let host = seeded_host(ExecHook::default());
    let dyn_host = prepared_host(Arc::clone(&host)).await;
    let opened_before = host.opened().len();
    let closed_before = host.closed().len();

    let token = CancelToken::new();
    token.cancel();
    let handler = DataSyncHandler::for_apply(apply_spec(), Arc::clone(&dyn_host));
    let outcome = handler
        .run_stage(&stage("apply"), &token)
        .await
        .expect("apply stage returns an outcome");

    assert_eq!(outcome.terminal, StageTerminal::Cancelled);
    assert_eq!(outcome.error_code, Some(ExecutionErrorCode::Cancelled));
    assert_eq!(outcome.effect_outcome, EffectOutcome::RolledBack);
    assert!(outcome.commit_boundaries.is_empty());
    assert_eq!(outcome.progress.committed.get(), 0);
    // 会话成对开闭，且一个批次都没执行。
    assert_eq!(host.opened().len(), opened_before + 2);
    assert_eq!(host.closed().len(), closed_before + 2);
    assert!(host.live().is_empty());
    // 执行器在取消检查之前就已取到（绑定目标 Lease），但一个批次都没开：
    // 只发生一次「无事务回滚」，没有 begin/execute/commit。
    assert_eq!(host.executor_leases().len(), 1);
    assert_eq!(host.events(), vec!["rollback#0".to_string()]);
    assert_eq!(host.pending_writes(), 0);
    for n in 1..=3 {
        assert_row_eq!(host.committed(&key(n)), Some(old_row(n)));
    }
}

/// 取消请求在 Job 运行途中抵达（而非 stage 开头就已取消）时，宿主执行器的
/// 写前检查点必须读到 kernel `CancelToken` 并停手。
///
/// 这里用 `batch_size = 2` 让批次 0 持有两条语句：第 1 条写完之后立刻置位
/// kernel 标志位，于是第 2 条写之前的那次检查就该拦下本次 apply。若检查点是
/// 瞎的（宿主自己铸标志位、或检查点被删），批次 0 会照常提交 2 行、再由批次
/// 循环顶部的检查拦下批次 1，终态退化成 `PartiallyApplied`。
#[tokio::test]
async fn cm44_a_cancel_arriving_mid_batch_stops_the_next_write_in_that_batch() {
    let host = seeded_host(ExecHook {
        cancel_token_after_statement: Some((0, 0)),
        ..ExecHook::default()
    });
    let dyn_host = prepared_host(Arc::clone(&host)).await;

    // 批次 0 = 前两行，批次 1 = 第三行。
    let handler = DataSyncHandler::for_apply(apply_spec_with(2), dyn_host);
    let outcome = handler
        .run_stage(&stage("apply"), &CancelToken::new())
        .await
        .expect("apply stage returns an outcome");

    assert_eq!(outcome.terminal, StageTerminal::Cancelled);
    assert_eq!(outcome.error_code, Some(ExecutionErrorCode::Cancelled));
    assert_eq!(
        outcome.effect_outcome,
        EffectOutcome::RolledBack,
        "取消在批次 0 的两次写之间抵达 ⇒ 一个提交边界都不能留下"
    );
    assert!(outcome.commit_boundaries.is_empty());
    assert_eq!(outcome.progress.committed.get(), 0);

    // 批次 0 的两次写都只被「尝试」过：第二条写被执行器的检查点拒绝，
    // 因此批次 0 既没有提交边界，批次 1 也从未开始。
    let events = host.events();
    assert!(host.has_event("begin#0"), "{events:?}");
    assert!(!host.has_event("commit#0"), "{events:?}");
    assert!(!host.has_event("begin#1"), "{events:?}");
    assert_eq!(
        events.last().map(String::as_str),
        Some("rollback#0"),
        "{events:?}"
    );
    for n in 1..=3 {
        assert_row_eq!(host.committed(&key(n)), Some(old_row(n)));
    }
    assert_eq!(host.pending_writes(), 0);
    assert!(host.live().is_empty(), "{:?}", host.opened());
    assert!(host.leaked().is_empty());
}
