//! CM-45：session mode 生命周期 / 同 Lease 清理（§7 清理、§10 会话模式）。
//!
//! 契约要点：
//! 1. 一个 Job 内从 open → 读/写 → close 全流程绑定**同一个资源 Lease**（同一 `ConnectionHandle.id`）；
//! 2. close 路径出错（资源已销毁、句柄未归还池）时不得复用该资源，且失败资源被显式销毁；
//! 3. 下一个消费者重新开自己的会话，不继承上一个 Job 的连接模式；
//! 4. 任一路径（成功/失败）都不留下未归还的会话句柄。

#[macro_use]
mod support;

use std::collections::HashMap;
use std::sync::Arc;

use datazen_data_sync::job::{ApplySpec, DataSyncHandler, DataSyncHost, PrepareSpec};
use datazen_data_sync::model::{Row, SyncOptions, TableMapping};
use datazen_platform_api::dto::execution::{EffectOutcome, ExecutionErrorCode};
use datazen_platform_api::id::StageId;
use datazen_runtime::job::{CancelToken, JobHandler, StageOutcome, StageSpec, StageTerminal};

use support::host::{source_endpoint, target_endpoint, FakeHost, TARGET_CONN};
use support::rows::{int, row, text, PLAN_ID, TABLE};

const REVISION: u64 = 1;

fn old_row(n: i64) -> Row {
    row(vec![int(n), text(&format!("old-{n}"))])
}

fn new_row(n: i64) -> Row {
    row(vec![int(n), text(&format!("new-{n}"))])
}

fn seeded_host() -> Arc<FakeHost> {
    let fake = FakeHost::new();
    fake.source_rows(TABLE, vec![new_row(1), new_row(2)]);
    fake.target_rows(TABLE, vec![old_row(1), old_row(2)]);
    fake.seed_target(vec![(vec![int(1)], old_row(1)), (vec![int(2)], old_row(2))]);
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
    ApplySpec {
        plan_id: PLAN_ID.to_string(),
        selection_revision: REVISION,
        options: SyncOptions::default(),
    }
}

async fn run_prepare(host: Arc<dyn DataSyncHost>) -> StageOutcome {
    DataSyncHandler::for_prepare(prepare_spec(), host)
        .run_stage(&stage("prepare"), &CancelToken::new())
        .await
        .expect("prepare stage returns an outcome")
}

/// prepare → 注册 review selection，供 apply 复用同一冻结 ChangeSet。
async fn prepared(host: &Arc<FakeHost>) -> Arc<dyn DataSyncHost> {
    let dyn_host: Arc<dyn DataSyncHost> = Arc::clone(host) as Arc<dyn DataSyncHost>;
    let outcome = run_prepare(Arc::clone(&dyn_host)).await;
    assert_eq!(outcome.terminal, StageTerminal::Succeeded);
    let artifact = dyn_host
        .load_artifact(PLAN_ID)
        .await
        .expect("artifact lookup succeeds")
        .expect("prepare stored the ChangeSet");
    assert_eq!(artifact.blocks.len(), 2);
    host.selection(PLAN_ID, REVISION, artifact.blocks);
    dyn_host
}

async fn run_apply(host: Arc<dyn DataSyncHost>) -> StageOutcome {
    DataSyncHandler::for_apply(apply_spec(), host)
        .run_stage(&stage("apply"), &CancelToken::new())
        .await
        .expect("apply stage returns an outcome")
}

#[tokio::test]
async fn cm45_prepare_and_apply_bind_one_lease_each_and_return_every_session() {
    let host = seeded_host();
    let dyn_host = prepared(&host).await;

    // prepare：两端各开一次会话，Job 结束即归还。
    let after_prepare = host.opened();
    assert_eq!(after_prepare.len(), 2, "{:?}", after_prepare);
    assert_eq!(host.closed().len(), 2);
    assert!(host.leaked().is_empty());
    assert!(host.events().is_empty(), "prepare 不开事务");

    let outcome = run_apply(Arc::clone(&dyn_host)).await;
    assert_eq!(outcome.terminal, StageTerminal::Succeeded);
    assert_eq!(outcome.effect_outcome, EffectOutcome::Completed);

    // apply 另开自己的一对会话：src-3 / tgt-4（证明不复用 prepare 的句柄）。
    let opened = host.opened();
    assert_eq!(opened.len(), 4, "{opened:?}");
    assert_eq!(&opened[2..], &["src-3".to_string(), "tgt-4".to_string()]);

    // 同一 Lease：执行器绑定的就是本 Job 的目标会话句柄，且只有这一个。
    assert_eq!(host.executor_leases(), vec!["tgt-4".to_string()]);

    // 每个会话开闭一一配对，无泄漏、无遗留。
    assert_eq!(host.closed().len(), 4);
    assert!(host.leaked().is_empty());
    assert!(host.live().is_empty(), "{opened:?}");
    assert_eq!(
        host.call_count("close_endpoint"),
        host.opened().len(),
        "close 调用次数必须等于 open 次数"
    );
}

#[tokio::test]
async fn cm45_failed_close_destroys_the_resource_and_the_next_job_gets_a_fresh_lease() {
    let host = seeded_host();
    let dyn_host = prepared(&host).await;
    // 在 apply 的第一次 close 上注入「资源已销毁、句柄未归还」。
    host.fail_close_once();

    let outcome = run_apply(Arc::clone(&dyn_host)).await;
    assert_eq!(outcome.terminal, StageTerminal::Succeeded);
    assert_eq!(outcome.effect_outcome, EffectOutcome::Completed);

    // apply 的 src-3 会话被销毁且未归还池，tgt-4 正常归还。
    assert_eq!(host.leaked(), vec!["src-3".to_string()]);
    assert!(host.closed().contains(&"tgt-4".to_string()));
    assert!(
        host.calls()
            .contains(&"close_endpoint:resource-destroyed(src-3)".to_string()),
        "{:?}",
        host.calls()
    );

    // 下一个消费者重新开自己的会话，不继承被销毁的 Lease。
    // 先把目标行还原到 review 基线（这里换用新的冻结副本：同一 plan 的重放）。
    host.seed_target(vec![(vec![int(1)], old_row(1)), (vec![int(2)], old_row(2))]);
    let opened_before = host.opened().len();
    let leases_before = host.executor_leases().len();
    let outcome2 = run_apply(Arc::clone(&dyn_host)).await;
    assert_eq!(outcome2.terminal, StageTerminal::Succeeded);

    let fresh = &host.opened()[opened_before..];
    assert_eq!(fresh, &["src-5".to_string(), "tgt-6".to_string()]);
    let leases = host.executor_leases();
    assert_eq!(leases.len(), leases_before + 1);
    assert_eq!(
        leases.last().map(String::as_str),
        Some("tgt-6"),
        "第二个 Job 必须绑定自己的 Lease"
    );
    assert!(
        !leases[..leases_before].iter().any(|l| l == "src-3"),
        "被销毁的资源不再出现在执行器 Lease 里"
    );
    // 第二次的 close 恢复正常：仍然零泄漏。
    assert_eq!(host.leaked(), vec!["src-3".to_string()]);
    assert!(host.live().is_empty(), "{:?}", host.opened());
}

#[tokio::test]
async fn cm45_target_open_failure_returns_the_source_session_without_executing() {
    let host = seeded_host();
    let dyn_host = prepared(&host).await;
    let opened_before = host.opened().len();
    host.fail_open(TARGET_CONN);

    let outcome = run_apply(Arc::clone(&dyn_host)).await;

    assert_eq!(outcome.terminal, StageTerminal::Failed);
    assert_eq!(outcome.error_code, Some(ExecutionErrorCode::HostRejected));
    assert_eq!(outcome.effect_outcome, EffectOutcome::NotStarted);
    assert!(outcome.commit_boundaries.is_empty());
    assert!(host.events().is_empty(), "拿不到目标会话 ⇒ 一个批次都没跑");
    assert!(host.executor_leases().is_empty());

    // 只开出了源会话，且已归还（无泄漏）。
    let opened = &host.opened()[opened_before..];
    assert_eq!(opened, &["src-3".to_string()]);
    assert!(host.live().is_empty(), "{:?}", host.opened());
    assert!(host.leaked().is_empty());
}

#[tokio::test]
async fn cm45_permission_denial_closes_both_sessions_and_leaves_no_pending_write() {
    let host = seeded_host();
    let dyn_host = prepared(&host).await;
    let opened_before = host.opened().len();
    host.deny_permissions();

    let outcome = run_apply(Arc::clone(&dyn_host)).await;

    assert_eq!(outcome.terminal, StageTerminal::Failed);
    assert_eq!(outcome.effect_outcome, EffectOutcome::NotStarted);
    assert!(outcome.commit_boundaries.is_empty());
    assert_eq!(host.opened().len(), opened_before + 2);
    assert_eq!(host.closed().len(), opened_before + 2);
    assert!(host.live().is_empty());
    assert!(host.leaked().is_empty());
    assert_eq!(host.pending_writes(), 0);
    assert!(host.events().is_empty());
}
