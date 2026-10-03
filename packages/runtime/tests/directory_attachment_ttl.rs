//! 挂载与 TTL 的语义（CM-63 + A3.6 / A3.7）。
//!
//! 覆盖点：
//!
//! - 重复 detach **不**续命；
//! - 无凭据挂载一律拒绝（连同伪造令牌、错 principal、错 client、未发过令牌的会话）；
//! - 合法挂载**不**刷新事务空闲期限；
//! - 多个期限同时存在时，最早的那个说了算，摘掉它才轮到下一个；
//! - 心跳**不**改期限；
//! - `Closing` / 已过期 / 已关闭的会话都不可再挂载；
//! - 判定到期只认服务端单调时钟，调用方给的墙钟不能提前收会话；
//! - 事务 TTL 与挂载 / 关闭并发推进时，裁定只发生一次，结果唯一。
//!
//! **所有时间都来自 [`common::FakeClock`]，全程没有一次 `sleep`。**

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{client, principal, CyclicEntropy, FakeClock};
use datazen_platform_api::context::OwnerRef;
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{
    AttachmentToken, ConnectionId, DbSessionId, EditorSessionId, ExecutionId, OrganizationId,
    PrincipalId, RuntimeEpoch, Timestamp, WorkerId,
};
use datazen_platform_api::ports::session_directory::{
    CloseDisposition, SessionDirectory, SessionOwner,
};
use datazen_runtime::directory::{
    Adjudication, AttachmentOutcome, AttachmentRejection, AttachmentRequest, DeadlineKind,
    InMemorySessionDirectory, MonoInstant, RouteRejection, SessionHandle,
};

const EPOCH: &str = "rte-ttl-0001";
const TTL: Duration = Duration::from_secs(60);

struct Opened {
    dir: InMemorySessionDirectory,
    handle: SessionHandle,
    token: AttachmentToken,
}

fn open(clock: &Arc<FakeClock>) -> Opened {
    let dir =
        InMemorySessionDirectory::with_sources(clock.shared(), Arc::new(CyclicEntropy::new(64)));
    let (handle, token) = dir
        .open_session(
            common::editor_draft("org-1", "user-1", "conn-1", "client-1"),
            RuntimeEpoch::new(EPOCH),
        )
        .expect("登记应成功");
    Opened { dir, handle, token }
}

fn secs(nanos_of_seconds: u128) -> MonoInstant {
    MonoInstant::from_nanos(nanos_of_seconds * 1_000_000_000)
}

fn transaction_idle_at(opened: &Opened) -> MonoInstant {
    opened
        .dir
        .deadlines(&opened.handle)
        .expect("条目应存在")
        .get(DeadlineKind::TransactionIdle)
        .expect("事务空闲期限应当存在")
        .at()
}

fn unauthenticated_owner(db_session_id: &str) -> SessionOwner {
    SessionOwner {
        db_session_id: DbSessionId::new(db_session_id),
        organization_id: OrganizationId::new("org-1"),
        principal_id: PrincipalId::new("user-1"),
        connection_id: ConnectionId::new("conn-1"),
        owner: OwnerRef::Editor {
            client_instance_id: client("client-1"),
            editor_session_id: EditorSessionId::new("client-1-editor"),
        },
        worker_id: WorkerId::new("worker-1"),
        runtime_epoch: RuntimeEpoch::new(EPOCH),
        resource_epoch: 1,
        last_business_activity: Timestamp::new("2026-01-01T00:00:00.000Z"),
    }
}

/// 重复 detach 不改期限：不 detach 一次、detach 四次，到期点必须完全一样。
#[test]
fn duplicate_detach_does_not_extend_the_deadline() {
    let clock = Arc::new(FakeClock::new());
    let opened = open(&clock);
    opened
        .dir
        .arm_transaction_idle(&opened.handle, TTL, MonoInstant::ZERO)
        .expect("装期限应成功");

    let armed_at = transaction_idle_at(&opened);
    assert_eq!(armed_at, secs(60), "期限应当落在 t0 + 60s");

    opened
        .dir
        .attach_from_client(
            &opened.handle,
            principal("user-1"),
            client("client-1"),
            opened.token.clone(),
        )
        .expect("首次挂载应成功");

    let projection = opened
        .dir
        .expiration_projection(&opened.handle)
        .expect("投影应可读");
    for _ in 0..4 {
        opened
            .dir
            .detach(&opened.handle)
            .expect("detach 应幂等成功");
        clock.advance_secs(30);
    }

    assert_eq!(
        transaction_idle_at(&opened),
        armed_at,
        "重复 detach 绝不能续命：期限必须停在 t0 + 60s"
    );
    assert_eq!(
        opened
            .dir
            .expiration_projection(&opened.handle)
            .expect("投影应可读"),
        projection,
        "expiresAt 是期限的投影，期限不动投影也不动"
    );
    assert_eq!(
        opened
            .dir
            .expiration_projection(&opened.handle)
            .expect("投影应可读"),
        Some(clock.utc_at(secs(60))),
        "投影必须等于最早期限的 UTC 投影，而不是「现在 + 60s」"
    );
}

/// 无凭据挂载必须被拒：只有句柄没有令牌 = 任何人都能抢别人的会话。
#[test]
fn credential_less_attach_is_rejected() {
    let clock = Arc::new(FakeClock::new());
    let opened = open(&clock);

    let rejection = opened
        .dir
        .attach(&AttachmentRequest::credential_less(
            opened.handle.clone(),
            principal("user-1"),
        ))
        .expect_err("无凭据挂载必须失败");

    assert_eq!(
        rejection,
        AttachmentRejection::NoToken,
        "没出示令牌就必须是无凭据拒绝，不得放行"
    );
    assert_eq!(
        rejection.to_port_error(&opened.handle.db_session_id),
        PortError::TokenInvalid,
        "对外只暴露既有的 TokenInvalid 变体，不新增错误类型"
    );
    assert!(opened.dir.is_routable(&opened.handle));
}

/// 令牌对了还得身份对得上；四类错配逐条可区分。
#[test]
fn attachment_requires_token_principal_and_owner_to_match() {
    let clock = Arc::new(FakeClock::new());
    let opened = open(&clock);

    let first = opened
        .dir
        .attach_from_client(
            &opened.handle,
            principal("user-1"),
            client("client-1"),
            opened.token.clone(),
        )
        .expect("凭据与身份都对得上，应挂载成功");
    assert_eq!(
        first,
        AttachmentOutcome::Attached { expires_at: None },
        "首次挂载是 Attached；没有期限时 expires_at 为 null"
    );

    let again = opened
        .dir
        .attach_from_client(
            &opened.handle,
            principal("user-1"),
            client("client-1"),
            opened.token.clone(),
        )
        .expect("重复挂载不应报错");
    assert!(
        matches!(again, AttachmentOutcome::AlreadyAttached { .. }),
        "已经挂着时是 AlreadyAttached，实际 {again:?}"
    );

    let forged = opened
        .dir
        .attach_from_client(
            &opened.handle,
            principal("user-1"),
            client("client-1"),
            AttachmentToken::new("att_ffffffffffffffffffffffffffffffff"),
        )
        .expect_err("伪造令牌必须被拒");
    assert_eq!(
        forged,
        AttachmentRejection::TokenMismatch,
        "令牌内容对不上就是 TokenMismatch"
    );

    let wrong_principal = opened
        .dir
        .attach_from_client(
            &opened.handle,
            principal("intruder"),
            client("client-1"),
            opened.token.clone(),
        )
        .expect_err("换一个 principal 必须被拒");
    assert_eq!(
        wrong_principal,
        AttachmentRejection::PrincipalMismatch,
        "令牌有效但主体不对，不得放行"
    );

    let wrong_client = opened
        .dir
        .attach_from_client(
            &opened.handle,
            principal("user-1"),
            client("client-2"),
            opened.token.clone(),
        )
        .expect_err("换一个 client 实例必须被拒");
    assert_eq!(
        wrong_client,
        AttachmentRejection::OwnerMismatch,
        "归属身份必须与 owner 对得上"
    );

    assert_eq!(
        forged.to_port_error(&opened.handle.db_session_id),
        PortError::TokenInvalid,
        "令牌本身不对，落到既有的 TokenInvalid 变体"
    );
    for identity_rejection in [&wrong_principal, &wrong_client] {
        assert!(
            matches!(
                identity_rejection.to_port_error(&opened.handle.db_session_id),
                PortError::NotFound(_)
            ),
            "主体 / 归属对不上时不承认这条会话存在（只回 NotFound），既不新增变体，\
             也不替 principal 泄漏会话的归属关系，实际 {:?}",
            identity_rejection
        );
    }
    assert_ne!(
        wrong_principal.code(),
        wrong_client.code(),
        "两类身份不符在模块内必须分别可辨，不能合并成一个笼统的拒绝"
    );
}

/// 目录里没发过令牌的会话（走 `register` 直接登记），任何令牌都换不来挂载权。
#[tokio::test]
async fn attach_requires_a_token_the_session_actually_issued() {
    let clock = Arc::new(FakeClock::new());
    let dir =
        InMemorySessionDirectory::with_sources(clock.shared(), Arc::new(CyclicEntropy::new(64)));
    let handle = dir
        .register(unauthenticated_owner("dbs-no-token"))
        .await
        .expect("登记应成功");

    let rejection = dir
        .attach_from_client(
            &handle,
            principal("user-1"),
            client("client-1"),
            AttachmentToken::new("att_whatever"),
        )
        .expect_err("未发过令牌的会话不得被挂载");
    assert_eq!(
        rejection,
        AttachmentRejection::NoTokenIssued,
        "目录里没有该会话的令牌摘要，只能报 NoTokenIssued，不能放行"
    );
}

/// 合法挂载不刷新事务空闲期限：挂载时点是 t0+30s，期限仍停在 t0+60s。
#[test]
fn attach_does_not_refresh_transaction_idle() {
    let clock = Arc::new(FakeClock::new());
    let opened = open(&clock);
    opened
        .dir
        .arm_transaction_idle(&opened.handle, TTL, MonoInstant::ZERO)
        .expect("装期限应成功");

    clock.advance_secs(30);
    opened
        .dir
        .attach_from_client(
            &opened.handle,
            principal("user-1"),
            client("client-1"),
            opened.token.clone(),
        )
        .expect("挂载应成功");
    opened
        .dir
        .heartbeat(&opened.handle, clock.utc_now())
        .expect("心跳应成功");

    assert_eq!(
        transaction_idle_at(&opened),
        secs(60),
        "挂载与心跳都不得把事务空闲期限推到 t0+90s"
    );
    assert_eq!(
        opened
            .dir
            .expiration_projection(&opened.handle)
            .expect("投影应可读"),
        Some(clock.utc_at(secs(60))),
        "expiresAt 应当是剩余期限的投影，不是从现在起重新计"
    );
}

/// 多个期限并存时最早者说了算；摘掉最早的才轮到下一个。
#[test]
fn earliest_deadline_wins_and_next_one_promotes() {
    let clock = Arc::new(FakeClock::new());
    let opened = open(&clock);

    opened
        .dir
        .arm_transaction_idle(&opened.handle, Duration::from_secs(100), MonoInstant::ZERO)
        .expect("装事务空闲");
    opened
        .dir
        .arm_session_idle(&opened.handle, Duration::from_secs(80), MonoInstant::ZERO)
        .expect("装会话空闲");
    opened
        .dir
        .arm_disconnect_grace(&opened.handle, Duration::from_secs(50), MonoInstant::ZERO)
        .expect("装断连宽限");

    assert_eq!(
        transaction_idle_at(&opened),
        secs(100),
        "并存期限各自记着自己的点，不许被最早者改写"
    );
    assert_eq!(
        opened
            .dir
            .expiration_projection(&opened.handle)
            .expect("投影应可读"),
        Some(clock.utc_at(secs(50))),
        "expiresAt 取最早期限（断连宽限 50s），不是任意一个"
    );

    opened
        .dir
        .clear_disconnect_grace(&opened.handle)
        .expect("摘断连宽限");
    assert_eq!(
        opened
            .dir
            .expiration_projection(&opened.handle)
            .expect("投影应可读"),
        Some(clock.utc_at(secs(80))),
        "摘掉最早者后，会话空闲 80s 应当接棒"
    );

    opened
        .dir
        .clear_transaction_idle(&opened.handle)
        .expect("摘事务空闲");
    assert_eq!(
        opened
            .dir
            .expiration_projection(&opened.handle)
            .expect("投影应可读"),
        Some(clock.utc_at(secs(80))),
        "只剩会话空闲时投影不动"
    );

    // 另一条会话：只装一个期限，摘干净之后 expiresAt 必须回到 null。
    let lonely = open(&clock);
    lonely
        .dir
        .arm_disconnect_grace(&lonely.handle, Duration::from_secs(30), MonoInstant::ZERO)
        .expect("装断连宽限");
    assert_eq!(
        lonely
            .dir
            .expiration_projection(&lonely.handle)
            .expect("投影应可读"),
        Some(clock.utc_at(secs(30)))
    );
    lonely
        .dir
        .clear_disconnect_grace(&lonely.handle)
        .expect("摘断连宽限");
    assert_eq!(
        lonely
            .dir
            .expiration_projection(&lonely.handle)
            .expect("投影应可读"),
        None,
        "没有适用期限时 expiresAt 就是 null，不编一个远期时间"
    );
    assert!(
        lonely
            .dir
            .deadlines(&lonely.handle)
            .expect("条目应存在")
            .is_empty(),
        "期限确实被摘干净了，不是投影蒙对了"
    );
}

/// 心跳只记业务活动时间，一个字的期限都不动。
#[test]
fn heartbeat_does_not_move_the_deadline() {
    let clock = Arc::new(FakeClock::new());
    let opened = open(&clock);
    opened
        .dir
        .arm_transaction_idle(&opened.handle, TTL, MonoInstant::ZERO)
        .expect("装期限应成功");

    clock.advance_secs(1_200);
    let beat = clock.utc_now();
    opened
        .dir
        .heartbeat(&opened.handle, beat.clone())
        .expect("心跳应成功");

    assert_eq!(
        transaction_idle_at(&opened),
        secs(60),
        "20 分钟过去又发了一次心跳，期限仍必须是 t0+60s"
    );
    let snapshot = opened.dir.observe(&opened.handle).expect("观测应可读");
    assert_eq!(
        snapshot.last_business_activity.as_str(),
        beat.as_str(),
        "心跳确实落地了（业务时间被更新），只是不构成 TTL 刷新"
    );
    assert!(!snapshot.attached, "心跳不代表有人挂着");
}

/// 到期、关闭中、已关闭：三种状态都不得再挂载。
#[tokio::test]
async fn closing_closed_and_expired_sessions_are_not_attachable() {
    let clock = Arc::new(FakeClock::new());
    let opened = open(&clock);
    opened
        .dir
        .arm_transaction_idle(&opened.handle, TTL, MonoInstant::ZERO)
        .expect("装期限应成功");

    clock.advance_secs(61);
    let adjudication = opened.dir.adjudicate(&opened.handle).expect("裁定应可读");
    assert!(
        matches!(adjudication, Adjudication::Expired { kind } if kind == DeadlineKind::TransactionIdle),
        "单调时钟越过期限后应裁定过期，实际 {adjudication:?}"
    );

    let after_expiry = opened
        .dir
        .attach_from_client(
            &opened.handle,
            principal("user-1"),
            client("client-1"),
            opened.token.clone(),
        )
        .expect_err("已过期不得再挂载");
    assert_eq!(
        after_expiry,
        AttachmentRejection::NotRoutable(RouteRejection::Expired {
            kind: DeadlineKind::TransactionIdle
        }),
        "过期的会话挂载必须被拒，并报明是哪个期限到点"
    );
    assert!(
        matches!(
            after_expiry.to_port_error(&opened.handle.db_session_id),
            PortError::CasConflict { entity, .. } if entity == "session"
        ),
        "对外落到既有的 CasConflict 变体"
    );
    assert!(!opened.dir.is_routable(&opened.handle));

    let swept = opened
        .dir
        .sweep_expired(clock.utc_now())
        .await
        .expect("清扫不应报错");
    assert_eq!(swept.len(), 1, "过期的会话应被清扫出去");
    let after_sweep = opened
        .dir
        .attach_from_client(
            &opened.handle,
            principal("user-1"),
            client("client-1"),
            opened.token.clone(),
        )
        .expect_err("已清扫的会话更不得挂载");
    assert!(
        matches!(
            after_sweep.to_port_error(&opened.handle.db_session_id),
            PortError::NotFound(_)
        ),
        "清扫之后目录里没有条目，只能按「查无此会话」处理，实际 {:?}",
        after_sweep
    );
    assert!(opened.dir.is_empty());

    let closed = open(&clock);
    closed
        .dir
        .release(&closed.handle, CloseDisposition::Closed)
        .await
        .expect("释放应成功");
    let after_release = closed
        .dir
        .attach_from_client(
            &closed.handle,
            principal("user-1"),
            client("client-1"),
            closed.token.clone(),
        )
        .expect_err("已释放的会话不得再挂载");
    assert!(
        matches!(
            after_release,
            AttachmentRejection::NotRoutable(RouteRejection::Closed { .. })
        ),
        "关闭后的会话必须报 Closed，实际 {after_release:?}"
    );
    assert_eq!(
        closed
            .dir
            .lookup(closed.handle.db_session_id.clone())
            .await
            .expect("lookup 不应报错"),
        None,
        "关闭中的会话对 lookup 不可见"
    );
}

/// 判定到期只看服务端单调时钟；调用方递过来的墙钟再晚也不能提前收会话。
#[tokio::test]
async fn sweeper_trusts_the_server_clock_not_the_caller_argument() {
    let clock = Arc::new(FakeClock::new());
    let opened = open(&clock);
    opened
        .dir
        .arm_transaction_idle(&opened.handle, TTL, MonoInstant::ZERO)
        .expect("装期限应成功");

    let liar = clock.utc_at(secs(86_400));
    let swept = opened.dir.sweep_expired(liar).await.expect("清扫不应报错");
    assert!(
        swept.is_empty(),
        "调用方说已经过去一天，但服务端单调时钟才走了 0 秒，不得提前收会话"
    );
    assert!(opened.dir.is_routable(&opened.handle));

    clock.advance_secs(61);
    let swept = opened
        .dir
        .sweep_expired(clock.utc_now())
        .await
        .expect("清扫不应报错");
    assert_eq!(swept.len(), 1, "单调时钟真的越线后才清");
    assert!(opened.dir.is_empty());
}

/// 执行中、又没有适用期限时，`expiresAt` 就是 null。
#[test]
fn expiration_projection_is_null_while_executing_without_a_deadline() {
    let clock = Arc::new(FakeClock::new());
    let opened = open(&clock);
    assert_eq!(
        opened
            .dir
            .expiration_projection(&opened.handle)
            .expect("投影应可读"),
        None,
        "刚开出来、什么期限都没装时不得编一个到期时间"
    );

    let execution = ExecutionId::new("exec-1");
    opened
        .dir
        .begin_execution(&opened.handle, execution.clone())
        .expect("执行应能开始");
    assert_eq!(
        opened
            .dir
            .expiration_projection(&opened.handle)
            .expect("投影应可读"),
        None,
        "执行中且无适用期限，expiresAt 仍然是 null"
    );

    let busy = opened
        .dir
        .begin_execution(&opened.handle, ExecutionId::new("exec-2"))
        .expect_err("会话忙时不得顶掉上一个执行");
    assert!(
        matches!(busy, PortError::CasConflict { entity, .. } if entity == "session"),
        "执行冲突应落到 CasConflict(session)，实际 {busy:?}"
    );

    opened
        .dir
        .arm_transaction_idle(&opened.handle, TTL, MonoInstant::ZERO)
        .expect("执行中也照样可以装事务空闲期限");
    assert_eq!(
        opened
            .dir
            .expiration_projection(&opened.handle)
            .expect("投影应可读"),
        Some(clock.utc_at(secs(60))),
        "一旦有适用期限就必须投影出来，即使正在执行"
    );
    assert_eq!(
        opened.dir.execution_occupancy(&opened.handle).ok(),
        Some(datazen_runtime::directory::ExecutionOccupancy::Busy(
            execution
        )),
        "占用状态应保持在第一个执行上"
    );
}

/// 事务 TTL 推着走的同时，挂载 / 关闭 / 清扫并发发生：终态唯一，且清扫幂等。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn transaction_ttl_racing_attach_and_close_adjudicates_once() {
    let clock = Arc::new(FakeClock::new());
    let opened = open(&clock);
    opened
        .dir
        .arm_transaction_idle(&opened.handle, Duration::from_secs(10), MonoInstant::ZERO)
        .expect("装期限应成功");

    let dir = Arc::new(opened.dir);
    let handle = opened.handle.clone();
    let token = opened.token.clone();
    let tick = Arc::clone(&clock);

    let mut tasks = Vec::new();
    for index in 0..8u32 {
        let dir = Arc::clone(&dir);
        let handle = handle.clone();
        let token = token.clone();
        let tick = Arc::clone(&tick);
        tasks.push(tokio::spawn(async move {
            // 每个任务都先把单调时钟往前推一点，再做自己的动作：期限推进与
            // 挂载 / 摘挂 / 释放 / 清扫真正交错，全程没有真实等待。
            tick.advance(Duration::from_millis(1_500));
            match index % 4 {
                0 => {
                    let outcome = dir.attach_from_client(
                        &handle,
                        principal("user-1"),
                        client("client-1"),
                        token,
                    );
                    format!("attach:{:?}", outcome.is_ok())
                }
                1 => {
                    let outcome = dir.detach(&handle);
                    format!("detach:{:?}", outcome.is_ok())
                }
                2 => {
                    let outcome = dir.release(&handle, CloseDisposition::ForceClosed).await;
                    format!("release:{:?}", outcome.is_ok())
                }
                _ => {
                    let swept = dir.sweep_expired(tick.utc_now()).await;
                    format!("sweep:{:?}", swept.map(|v| v.len()))
                }
            }
        }));
    }

    let mut outcomes = Vec::new();
    for task in tasks {
        outcomes.push(task.await.expect("并发任务不应 panic"));
    }
    assert_eq!(outcomes.len(), 8, "8 个动作都要有结论，不许无声消失");
    assert!(
        outcomes.iter().any(|line| line.starts_with("sweep:")),
        "并发队列里应当真有清扫动作跑过：{outcomes:?}"
    );

    tick.advance(Duration::from_secs(60));
    let before_final = dir.revision();
    let final_sweep = dir
        .sweep_expired(tick.utc_now())
        .await
        .expect("清扫不应报错");
    assert!(
        final_sweep.len() <= 1,
        "一条会话至多被收走一次，收了 {} 条",
        final_sweep.len()
    );
    assert_eq!(
        dir.revision(),
        before_final,
        "没有新会话可收时，重复清扫不得推高版本号"
    );
    assert!(
        dir.is_empty(),
        "无论并发交错成什么样，会话终将被清扫出目录，实际还剩 {} 条",
        dir.len()
    );
    assert_eq!(
        dir.lookup(handle.db_session_id.clone())
            .await
            .expect("lookup 不应报错"),
        None,
        "终态之后目录里查不到这条会话"
    );
    let repeat = dir
        .sweep_expired(tick.utc_now())
        .await
        .expect("清扫不应报错");
    assert!(
        repeat.is_empty(),
        "清扫必须幂等：第二次没有东西可收，说明这条会话没有被重复收走"
    );
}
