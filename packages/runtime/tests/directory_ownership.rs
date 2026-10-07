//! `dbSessionId` 的登记语义。
//!
//! 覆盖点：
//!
//! - 同一个 `dbSessionId` 并发登记 ⇒ 只有一个成功，且只有一个 owner；
//! - 随机源碰撞时**重新生成**，预算耗尽时报错，**绝不返回成功**；
//! - 没有中心发号器：ID 来自随机源，不是进程内自增计数器；
//! - worker 重启后 epoch 变化，旧句柄**显式失败**，不替它找配置里的替代会话。
//!
//! 断言全部非空：把目录实现删掉，或者让它改成「撞 id 就重绑」「注册失败也返回
//! Ok」「句柄过期就换一条来路由」，这些用例都会红。

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use common::{
    client, cycling_directory, directory, editor_draft, CyclicEntropy, FixedEntropy, TestClock,
};
use datazen_platform_api::context::OwnerRef;
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{
    ConnectionId, DbSessionId, EditorSessionId, OrganizationId, PrincipalId, RuntimeEpoch,
    Timestamp, WorkerId,
};
use datazen_platform_api::ports::session_directory::{SessionDirectory, SessionOwner};
use datazen_runtime::directory::{InMemorySessionDirectory, SessionHandle};

const EPOCH: &str = "rte-integration-0001";

fn owner_with(db_session_id: &str, principal: &str, client_instance: &str) -> SessionOwner {
    SessionOwner {
        db_session_id: DbSessionId::new(db_session_id),
        organization_id: OrganizationId::new("org-1"),
        principal_id: PrincipalId::new(principal),
        connection_id: ConnectionId::new("conn-1"),
        owner: OwnerRef::Editor {
            client_instance_id: client(client_instance),
            editor_session_id: EditorSessionId::new(format!("{client_instance}-editor")),
        },
        worker_id: WorkerId::new("worker-1"),
        runtime_epoch: RuntimeEpoch::new(EPOCH),
        resource_epoch: 7,
        last_business_activity: Timestamp::new("2026-01-01T00:00:00.000Z"),
    }
}

fn assert_cas_conflict(err: &PortError, entity: &str, context: &str) {
    assert!(
        matches!(err, PortError::CasConflict { entity: e, .. } if &**e == entity),
        "{context}：期望 CasConflict({entity})，实际 {err:?}"
    );
}

/// 同一个 `dbSessionId` 被并发登记：只有一个成功，目录里只留一个 owner。
///
/// 非空点：若实现「后写覆盖前写」，`lookup` 拿到的 principal 会随机漂移且成功数 > 1；
/// 若实现忽略冲突直接 Ok，成功数会等于 N。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_registration_of_one_id_has_exactly_one_winner() {
    let (_clock, dir) = cycling_directory(64);
    let dir = Arc::new(dir);
    let contenders = 12;

    let mut tasks = Vec::with_capacity(contenders);
    for index in 0..contenders {
        let shared = Arc::clone(&dir);
        let candidate = owner_with("dbs-contended", &format!("user-{index}"), "client-1");
        tasks.push(tokio::spawn(async move {
            (index, shared.register(candidate).await)
        }));
    }

    let mut winner: Option<(usize, SessionHandle)> = None;
    let mut failures = 0usize;
    for task in tasks {
        let (index, result) = task.await.expect("登记任务不应 panic");
        match result {
            Ok(handle) => {
                assert!(
                    winner.is_none(),
                    "同一个 dbSessionId 并发登记只能有一个成功，第 {index} 次不该也返回成功"
                );
                winner = Some((index, handle));
            }
            Err(err) => {
                assert_cas_conflict(&err, "dbSessionId", "重复登记必须报冲突");
                failures += 1;
            }
        }
    }

    let (winner_index, handle) = winner.expect("至少有一方应该登记成功");
    assert_eq!(
        failures,
        contenders - 1,
        "其余登记必须逐条报错，不能悄悄成功"
    );
    assert_eq!(handle.db_session_id.as_str(), "dbs-contended");

    let stored = dir
        .lookup(DbSessionId::new("dbs-contended"))
        .await
        .expect("lookup 不应报错")
        .expect("胜出的条目必须可查");
    assert_eq!(
        stored.principal_id.as_str(),
        format!("user-{winner_index}"),
        "留存下的 owner 必须就是胜出的那一方（一个 ID 只能有一个登记 owner）"
    );
    assert_eq!(dir.len(), 1, "冲突不得产生第二个条目");
}

/// 随机源恒定 ⇒ 第一次成功，之后每次都撞同一个 id ⇒ 预算耗尽必须报错。
///
/// 非空点：若实现撞 id 后复用、或重试预算无限循环，用例会挂住；
/// 若实现把失败吞成 Ok，`Ok(_)` 分支直接 panic。
#[tokio::test]
async fn registration_failure_never_reports_success() {
    let clock = Arc::new(TestClock::new());
    let dir = directory(&clock, Arc::new(FixedEntropy::new(0x5a)));
    let epoch = RuntimeEpoch::new(EPOCH);

    let first = dir
        .open_session(
            editor_draft("org-1", "user-1", "conn-1", "client-1"),
            epoch.clone(),
        )
        .expect("首次登记应成功");
    assert_eq!(dir.len(), 1);

    for attempt in 0..3 {
        match dir.open_session(
            editor_draft("org-1", "user-2", "conn-1", "client-1"),
            epoch.clone(),
        ) {
            Ok(_) => panic!("第 {attempt} 次重复登记必须失败：熵源恒定，id 永远撞车，不得返回成功"),
            Err(PortError::BackendUnavailable(message)) => {
                assert!(
                    message.contains("colliding"),
                    "预算耗尽的错误要说明是碰撞没解开，实际 {message}"
                );
            }
            Err(other) => panic!("重试预算耗尽应报 BackendUnavailable，实际 {other:?}"),
        }
    }

    assert_eq!(
        dir.len(),
        1,
        "失败的登记不得留下半成品条目；既有会话仍应可用"
    );
    assert_eq!(
        dir.resolve(&first.0)
            .expect("既有会话仍可解析")
            .principal_id
            .as_str(),
        "user-1"
    );
}

/// 并发开会话：成功数 + 失败数恒等于请求数，且每个成功 ID 只有一个 owner。
///
/// 非空点：既防「两个任务拿到同一个 ID」，也防「拿不到 ID 却返回 Ok」。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_open_gives_each_registered_id_exactly_one_owner() {
    let (_clock, dir) = cycling_directory(97);
    let dir = Arc::new(dir);
    let epoch = RuntimeEpoch::new(EPOCH);
    let requesters = 24;

    let mut tasks = Vec::with_capacity(requesters);
    for index in 0..requesters {
        let shared = Arc::clone(&dir);
        let epoch = epoch.clone();
        tasks.push(tokio::spawn(async move {
            let draft = editor_draft("org-1", &format!("user-{index}"), "conn-1", "client-1");
            (index, shared.open_session(draft, epoch))
        }));
    }

    let mut success_count = 0usize;
    let mut failure_count = 0usize;
    let mut winners: BTreeMap<String, String> = BTreeMap::new();
    for task in tasks {
        let (index, result) = task.await.expect("开会话任务不应 panic");
        match result {
            Ok((handle, _token)) => {
                success_count += 1;
                let id = handle.db_session_id.as_str().to_string();
                assert!(
                    id.starts_with("dbs_") && id.len() == 4 + 32,
                    "dbSessionId 必须是随机 128 位以上的十六进制串，实际 {id}"
                );
                let previous = winners.insert(id.clone(), format!("user-{index}"));
                assert!(
                    previous.is_none(),
                    "并发开出的 dbSessionId {id} 重复了，一个 ID 只能有一个 owner"
                );
            }
            Err(err) => {
                failure_count += 1;
                assert!(
                    matches!(err, PortError::BackendUnavailable(_)),
                    "开会话失败只允许报 BackendUnavailable，实际 {err:?}"
                );
            }
        }
    }

    assert_eq!(
        success_count + failure_count,
        requesters,
        "每个请求必须要么成功要么失败，不许凭空消失"
    );
    assert!(success_count > 1, "熵源容量足够时应当开出多个会话");
    assert_eq!(
        dir.len(),
        success_count,
        "目录条目数必须等于成功数：失败的登记不得留下残留"
    );

    for (id, principal) in &winners {
        let stored = dir
            .lookup(DbSessionId::new(id.as_str()))
            .await
            .expect("lookup 不应报错")
            .unwrap_or_else(|| panic!("已登记的 {id} 必须能查到 owner"));
        assert_eq!(
            stored.principal_id.as_str(),
            principal,
            "{id} 的 owner 必须唯一且等于登记它的那一方"
        );
    }
}

/// 没有中心发号器：两个独立目录吃同一份确定性随机源，会得到**同一个** ID。
///
/// 非空点：任何进程内自增计数器 / 全局序号都会让两边错开，从而让这条断言红；
/// 反过来，「随机 128 位」的实现必然满足。
#[tokio::test]
async fn session_ids_come_from_entropy_not_from_a_central_allocator() {
    let dir_a = directory(
        &Arc::new(TestClock::new()),
        Arc::new(CyclicEntropy::new(1_000)),
    );
    let dir_b = directory(
        &Arc::new(TestClock::new()),
        Arc::new(CyclicEntropy::new(1_000)),
    );
    let epoch = RuntimeEpoch::new(EPOCH);

    let first = dir_a
        .open_session(
            editor_draft("org-1", "user-a", "conn-1", "client-1"),
            epoch.clone(),
        )
        .expect("A 目录首次开会话");
    let same_seed = dir_b
        .open_session(
            editor_draft("org-2", "user-b", "conn-2", "client-2"),
            epoch.clone(),
        )
        .expect("B 目录首次开会话");

    assert_eq!(
        first.0.db_session_id, same_seed.0.db_session_id,
        "同一份随机源在两个目录里产出同一 ID，说明发号不依赖任何跨目录的共享序号"
    );

    let mut ids = BTreeSet::new();
    for _ in 0..6 {
        let (handle, _token) = dir_a
            .open_session(
                editor_draft("org-1", "user-a", "conn-1", "client-1"),
                epoch.clone(),
            )
            .expect("熵源容量足够，后续登记都应成功");
        assert!(
            ids.insert(handle.db_session_id.as_str().to_string()),
            "同一目录内连续开出的 ID 必须互不相同"
        );
    }
    assert_eq!(ids.len(), 6, "不得出现重复 ID");
}

/// 陈旧句柄必须显式失败，且**不得**被换成另一个会话。
///
/// 非空点：若实现「找不到就换个能路由的会话」，`resolve` 会返回 Ok，这条断言立刻红。
#[tokio::test]
async fn stale_handle_fails_explicitly_without_borrowing_another_session() {
    let (_clock, dir) = cycling_directory(64);
    let live = dir
        .open_session(
            editor_draft("org-1", "user-live", "conn-1", "client-1"),
            RuntimeEpoch::new(EPOCH),
        )
        .expect("登记应成功");
    let other = dir
        .open_session(
            editor_draft("org-2", "user-other", "conn-2", "client-2"),
            RuntimeEpoch::new(EPOCH),
        )
        .expect("登记应成功");

    let stale = SessionHandle {
        db_session_id: live.0.db_session_id.clone(),
        runtime_epoch: RuntimeEpoch::new("rte-from-a-dead-worker"),
    };

    match dir.resolve(&stale) {
        Err(PortError::CasConflict { entity, id }) => {
            assert_eq!(
                entity, "session",
                "陈旧句柄的拒绝实体是会话本身，不是别的东西"
            );
            assert!(
                id.contains("staleRuntimeEpoch") && id.contains(EPOCH),
                "陈旧句柄的错误必须点名**当前** epoch，调用方据此区分「ID 从没注册过」\
                 与「ID 在，但持有它的是上一代 worker」：{id}"
            );
            assert!(
                !id.contains("rte-from-a-dead-worker"),
                "报错里回显的是当前 epoch，不是持有者已死的那个：{id}"
            );
        }
        Ok(owner) => panic!(
            "陈旧句柄不得解析成功，更不得改拿别的会话；实际拿到 {}",
            owner.db_session_id.as_str()
        ),
        Err(other) => panic!("陈旧句柄应报 CasConflict，实际 {other:?}"),
    }
    let unknown = SessionHandle {
        db_session_id: DbSessionId::new("dbs_never_registered_0000"),
        runtime_epoch: RuntimeEpoch::new(EPOCH),
    };
    assert!(
        matches!(dir.resolve(&unknown), Err(PortError::NotFound(_))),
        "真正没注册过的 ID 报 NotFound，与陈旧句柄的 CasConflict 分得开"
    );

    assert!(!dir.is_routable(&stale), "陈旧句柄不得可路由");
    assert!(
        dir.touch(&stale, Timestamp::new("2026-01-01T00:01:00.000Z"))
            .await
            .is_err(),
        "陈旧句柄不得刷新任何会话的业务时间"
    );
    assert_eq!(
        dir.resolve(&live.0)
            .expect("活句柄不受影响")
            .principal_id
            .as_str(),
        "user-live"
    );
    assert_eq!(
        dir.resolve(&other.0)
            .expect("另一个活会话也不受影响")
            .principal_id
            .as_str(),
        "user-other"
    );
    assert_eq!(dir.len(), 2, "陈旧句柄的失败不得增删条目");
}

/// worker 每次启动换一个 epoch；换了 epoch 之后旧句柄立刻失效。
#[tokio::test]
async fn worker_restart_rotates_the_epoch_and_kills_old_handles() {
    let (_clock, dir) = cycling_directory(64);
    let worker = WorkerId::new("worker-1");

    // 顺序照真实进程走：先起 worker-1，在它手里开会话；worker-1 挂掉后重启，同一个
    // WorkerId 领到**另一个** epoch。
    let first_epoch = dir.start_worker_epoch(&worker);
    let (old_handle, _token) = dir
        .open_session(
            editor_draft("org-1", "user-1", "conn-1", "client-1"),
            first_epoch.clone(),
        )
        .expect("旧 worker 会话应登记成功");
    assert!(dir.is_routable(&old_handle), "旧 worker 自己在时应当可路由");

    let second_epoch = dir.start_worker_epoch(&worker);
    assert_ne!(
        first_epoch, second_epoch,
        "每次启动都必须重新生成 runtimeEpoch，否则旧 worker 的句柄还能继续路由"
    );
    assert!(
        !dir.is_routable(&old_handle),
        "worker 重启之后，上一代签发的句柄必须立即不可路由"
    );
    assert!(
        matches!(
            dir.resolve(&old_handle),
            Err(PortError::CasConflict { id, .. }) if id.contains(second_epoch.as_str())
        ),
        "旧句柄要显式报错并点名当前 epoch，而不是悄悄路由到别的会话"
    );
    assert!(
        dir.touch(&old_handle, Timestamp::new("2026-01-01T00:00:30.000Z"))
            .await
            .is_err(),
        "旧句柄不得再刷新任何业务时间"
    );

    let (new_handle, _token) = dir
        .open_session(
            editor_draft("org-1", "user-1", "conn-1", "client-1"),
            second_epoch,
        )
        .expect("新 worker 会话应登记成功");
    assert!(dir.is_routable(&new_handle), "新 worker 的句柄照常可路由");
}

/// 目录默认走 OS 随机源：连开若干次都不撞车，且 ID 互不相同。
#[tokio::test]
async fn default_entropy_source_yields_distinct_sessions() {
    let dir = InMemorySessionDirectory::new();
    assert!(dir.is_empty(), "新目录必须为空");

    let mut ids = BTreeSet::new();
    for _ in 0..8 {
        let (handle, _token) = dir
            .open_session(
                editor_draft("org-1", "user-1", "conn-1", "client-1"),
                RuntimeEpoch::new(EPOCH),
            )
            .expect("OS 随机源下不应失败");
        assert!(
            ids.insert(handle.db_session_id.as_str().to_string()),
            "OS 随机源连续生成的 dbSessionId 不得重复"
        );
    }
    assert_eq!(ids.len(), 8, "8 次登记应给出 8 个不同 ID");
    assert_eq!(dir.len(), 8);
}
