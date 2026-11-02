//! `ServerSessionDirectory` 行为钉子：端口域文档四条不可协商约束 + 形态
//! 裁决（碰撞重生成 / 候选不可路由 / 释放幂等 / 过期清扫）的逐条断言。

use std::time::Duration;

use datazen::execution::domain::session_directory::{
    CloseDisposition, InvalidationReason, ReplacementCommit, ReplacementOperation,
    SessionDirectory, SessionOwner,
};
use datazen::execution::domain::SessionHandle;
use datazen::shared::context::OwnerRef;
use datazen::shared::id::{
    ClientInstanceId, ConnectionId, DbSessionId, EditorSessionId, OrganizationId, PrincipalId,
    RuntimeEpoch, Timestamp, WorkerId,
};

use super::ServerSessionDirectory;

fn owner(id: &str, epoch: &str) -> SessionOwner {
    SessionOwner {
        db_session_id: DbSessionId::new(id),
        organization_id: OrganizationId::new("org-1"),
        principal_id: PrincipalId::new("user-1"),
        connection_id: ConnectionId::new("conn-1"),
        owner: OwnerRef::Editor {
            client_instance_id: ClientInstanceId::new("client-1"),
            editor_session_id: EditorSessionId::new("ed-1"),
        },
        worker_id: WorkerId::new("worker-1"),
        runtime_epoch: RuntimeEpoch::new(epoch),
        resource_epoch: 1,
        last_business_activity: Timestamp::new("2026-01-01T00:00:00.000Z"),
    }
}

fn handle(id: &str, epoch: &str) -> SessionHandle {
    SessionHandle::new(DbSessionId::new(id), RuntimeEpoch::new(epoch))
}

#[tokio::test]
async fn register_returns_a_handle_and_lookup_round_trips() {
    let directory = ServerSessionDirectory::new();
    let entry = owner("sess-1", "epoch-1");
    let registered = directory.register(entry.clone()).await.expect("register");
    assert_eq!(registered.db_session_id, entry.db_session_id);
    assert_eq!(registered.runtime_epoch, entry.runtime_epoch);

    let found = directory
        .lookup(DbSessionId::new("sess-1"))
        .await
        .expect("lookup");
    assert_eq!(found, Some(entry));
}

#[tokio::test]
async fn register_regenerates_on_a_db_session_id_collision() {
    let directory = ServerSessionDirectory::new();
    directory
        .register(owner("sess-collide", "epoch-1"))
        .await
        .expect("first register");

    let second = directory
        .register(owner("sess-collide", "epoch-2"))
        .await
        .expect("collision register regenerates");
    assert_ne!(
        second.db_session_id,
        DbSessionId::new("sess-collide"),
        "碰撞的 id 必须被重生成，不得顶掉活条目"
    );
    // 原条目未被覆盖：epoch 仍是 epoch-1。
    let original = directory
        .lookup(DbSessionId::new("sess-collide"))
        .await
        .expect("lookup");
    assert_eq!(
        original.expect("original alive").runtime_epoch,
        RuntimeEpoch::new("epoch-1")
    );
}

#[tokio::test]
async fn touch_updates_only_the_business_activity_time() {
    let directory = ServerSessionDirectory::new();
    let entry = owner("sess-touch", "epoch-1");
    directory.register(entry.clone()).await.expect("register");

    directory
        .touch(
            &handle("sess-touch", "epoch-1"),
            Timestamp::new("2026-02-02T00:00:00.000Z"),
        )
        .await
        .expect("touch");

    let found = directory
        .lookup(DbSessionId::new("sess-touch"))
        .await
        .expect("lookup")
        .expect("alive");
    assert_eq!(
        found.last_business_activity,
        Timestamp::new("2026-02-02T00:00:00.000Z")
    );
    // 其余字段全部原样（touch 只改一个字段）。
    assert_eq!(found.runtime_epoch, entry.runtime_epoch);
    assert_eq!(found.resource_epoch, entry.resource_epoch);
    assert_eq!(found.owner, entry.owner);
}

#[tokio::test]
async fn stale_epoch_or_unknown_handles_are_not_found_for_touch() {
    let directory = ServerSessionDirectory::new();
    directory
        .register(owner("sess-stale", "epoch-1"))
        .await
        .expect("register");

    // 旧纪元句柄不得触到新条目。
    let stale = directory
        .touch(
            &handle("sess-stale", "epoch-0"),
            Timestamp::new("2026-02-02T00:00:00.000Z"),
        )
        .await;
    assert!(
        matches!(stale, Err(datazen::shared::error::PortError::NotFound(_))),
        "{stale:?}"
    );
    let unknown = directory
        .touch(
            &handle("sess-none", "epoch-1"),
            Timestamp::new("2026-02-02T00:00:00.000Z"),
        )
        .await;
    assert!(matches!(
        unknown,
        Err(datazen::shared::error::PortError::NotFound(_))
    ));
}

#[tokio::test]
async fn prepared_candidates_are_not_routable_until_committed() {
    let directory = ServerSessionDirectory::new();
    directory
        .register(owner("sess-old", "epoch-1"))
        .await
        .expect("register old");
    let candidate = owner("sess-new", "epoch-2");

    directory
        .commit_replacement(ReplacementCommit {
            old: handle("sess-old", "epoch-1"),
            new_owner: candidate.clone(),
            operation: ReplacementOperation::Prepared,
        })
        .await
        .expect("prepare");
    // 候选不可路由。
    assert!(
        directory
            .lookup(DbSessionId::new("sess-new"))
            .await
            .expect("lookup")
            .is_none(),
        "prepared 候选不得被 lookup 命中"
    );
    // 旧条目保持服务。
    assert!(directory
        .lookup(DbSessionId::new("sess-old"))
        .await
        .expect("lookup")
        .is_some());

    let outcome = directory
        .commit_replacement(ReplacementCommit {
            old: handle("sess-old", "epoch-1"),
            new_owner: candidate,
            operation: ReplacementOperation::Committed,
        })
        .await
        .expect("commit");
    assert_eq!(outcome.operation, ReplacementOperation::Committed);
    assert_eq!(outcome.replaced, handle("sess-old", "epoch-1"));
    // 提交后：新条目可路由，旧条目关闭路由。
    assert!(directory
        .lookup(DbSessionId::new("sess-new"))
        .await
        .expect("lookup")
        .is_some());
    assert!(directory
        .lookup(DbSessionId::new("sess-old"))
        .await
        .expect("lookup")
        .is_none());
}

#[tokio::test]
async fn rolled_back_replacement_keeps_the_old_session_and_drops_the_candidate() {
    let directory = ServerSessionDirectory::new();
    directory
        .register(owner("sess-keep", "epoch-1"))
        .await
        .expect("register");
    let candidate = owner("sess-cand", "epoch-2");

    directory
        .commit_replacement(ReplacementCommit {
            old: handle("sess-keep", "epoch-1"),
            new_owner: candidate.clone(),
            operation: ReplacementOperation::Prepared,
        })
        .await
        .expect("prepare");
    let outcome = directory
        .commit_replacement(ReplacementCommit {
            old: handle("sess-keep", "epoch-1"),
            new_owner: candidate,
            operation: ReplacementOperation::RolledBack,
        })
        .await
        .expect("rollback");
    assert_eq!(outcome.operation, ReplacementOperation::RolledBack);
    assert!(
        directory
            .lookup(DbSessionId::new("sess-keep"))
            .await
            .expect("lookup")
            .is_some(),
        "回滚后旧会话保持原状"
    );
    assert!(
        directory
            .lookup(DbSessionId::new("sess-cand"))
            .await
            .expect("lookup")
            .is_none(),
        "回滚后候选被销毁且不可路由"
    );
}

#[tokio::test]
async fn committed_replacement_without_a_prepared_candidate_is_rejected() {
    let directory = ServerSessionDirectory::new();
    let outcome = directory
        .commit_replacement(ReplacementCommit {
            old: handle("sess-none", "epoch-1"),
            new_owner: owner("sess-ghost", "epoch-2"),
            operation: ReplacementOperation::Committed,
        })
        .await;
    assert!(
        matches!(outcome, Err(datazen::shared::error::PortError::NotFound(_))),
        "没有 prepared 候选的 committed 必须拒绝，{outcome:?}"
    );
}

#[tokio::test]
async fn invalidate_removes_the_entry_so_requests_must_fail() {
    let directory = ServerSessionDirectory::new();
    directory
        .register(owner("sess-lost", "epoch-1"))
        .await
        .expect("register");

    directory
        .invalidate(
            DbSessionId::new("sess-lost"),
            InvalidationReason::WorkerLost,
        )
        .await
        .expect("invalidate");
    assert!(
        directory
            .lookup(DbSessionId::new("sess-lost"))
            .await
            .expect("lookup")
            .is_none(),
        "作废后不得透明重建（lookup 恒 None）"
    );
    // 重复作废同一 id → NotFound（没有可作废的条目）。
    let again = directory
        .invalidate(
            DbSessionId::new("sess-lost"),
            InvalidationReason::ResourceLost,
        )
        .await;
    assert!(matches!(
        again,
        Err(datazen::shared::error::PortError::NotFound(_))
    ));
}

#[tokio::test]
async fn release_is_idempotent_and_never_releases_across_epochs() {
    let directory = ServerSessionDirectory::new();
    directory
        .register(owner("sess-close", "epoch-1"))
        .await
        .expect("register");

    directory
        .release(&handle("sess-close", "epoch-1"), CloseDisposition::Closed)
        .await
        .expect("release");
    // 重复释放幂等。
    directory
        .release(
            &handle("sess-close", "epoch-1"),
            CloseDisposition::ForceClosed,
        )
        .await
        .expect("release is idempotent");
    // 旧纪元句柄不释放新纪元条目。
    directory
        .register(owner("sess-close", "epoch-2"))
        .await
        .expect("re-register");
    directory
        .release(&handle("sess-close", "epoch-1"), CloseDisposition::Closed)
        .await
        .expect("stale epoch release is a no-op");
    assert!(
        directory
            .lookup(DbSessionId::new("sess-close"))
            .await
            .expect("lookup")
            .is_some(),
        "旧纪元句柄不得关闭新纪元的条目"
    );
}

#[tokio::test]
async fn sweep_expired_returns_only_stale_entries() {
    let directory = ServerSessionDirectory::with_idle_ttl(Duration::from_secs(60));
    let mut fresh = owner("sess-fresh", "epoch-1");
    // fresh 的业务活动贴着 sweep 时刻（未超 60s idle TTL）。
    fresh.last_business_activity = Timestamp::new("2026-01-01T00:05:00.000Z");
    let mut stale = owner("sess-stale", "epoch-1");
    stale.last_business_activity = Timestamp::new("2026-01-01T00:00:00.000Z");
    directory
        .register(fresh.clone())
        .await
        .expect("register fresh");
    directory
        .register(stale.clone())
        .await
        .expect("register stale");

    let swept = directory
        .sweep_expired(Timestamp::new("2026-01-01T00:05:00.000Z"))
        .await
        .expect("sweep");
    assert_eq!(
        swept
            .iter()
            .map(|o| o.db_session_id.as_str())
            .collect::<Vec<_>>(),
        vec!["sess-stale"],
        "只清扫超过 TTL 的条目"
    );
    assert!(directory
        .lookup(DbSessionId::new("sess-fresh"))
        .await
        .expect("lookup")
        .is_some());
    assert!(directory
        .lookup(DbSessionId::new("sess-stale"))
        .await
        .expect("lookup")
        .is_none());
}
