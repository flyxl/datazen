//! D-R2-2 的钉子测试：三态分类表的**每一格**都要有走完整 `invalidate_worker`
//! 的用例，而且每格的后果是**三件事一起**断言：表项在不在、`lost` 收没收、
//! 额度挂没挂 stale。逐栏去测（甚至绕过 `invalidate_worker` 直接问 actor）
//! 是上一版用例没能保护住这块代码的原因——把三态并成两态的改法照样能过。
//!
//! | `control()` | 用例 | 造法 |
//! | --- | --- | --- |
//! | `Ok(true)` | `送达即作废_摘行并把额度挂成stale` | 登记后直接租约失效 |
//! | `Ok(false)` | `否定答复仍算送达_已无物理资源的会话照常摘行` | 候选走完 §9.4 销毁之后仍被发布进可见表（见该用例注释） |
//! | `Err(_)` | `投递失败保留表项_额度不挂stale等重投` | 驱动回调在 `close()` 里 panic，把 actor 任务带走 |
//!
//! 后两格都不是「造出来的假象」：第三格是驱动侧崩溃打穿 actor 任务的**生产事故形状**，
//! 两格都不需要改生产代码。
//!
//! ★ 第二格的造法**变过**，理由必须写在这里而不是台账里：此前它靠的是先跑一次
//! `evict_idle_at` 并配一个关不掉的驱动（`Undecidable`）——§9.4 在物理层关不掉时
//! **先**清空 `state.physical` 才写出 `SessionLost`，而当时的 `evict_idle_at`
//! 用 `let Ok(Some(..)) = .. else { continue }` 把这条答复当成「什么都没发生」，
//! 于是留下「资源已清空、行还在表里」的残留格。那**就是缺陷本身**：物理资源已经
//! 发出过关闭、行却留在表里、额度还扣着、`session_view` 仍能向一个不存在的资源发执行。
//! 驱逐改为逐向判别（`SessionLost` ⇒ 摘表还额度）之后这条路不再产生该状态，所以本组
//! 改用**候选原语**造它：销毁过的候选被发布进可见表，同样是「在册但已无物理资源」。
//! **判定的三态一个字没改，改的只是造法**；驱逐那一格现在的正确行为由
//! `不可判定的驱逐摘表还额度_不留僵尸行` 一例钉住——它钉的正是修好之后的形状，
//! 与 `tests/registry_release.rs` 里同一形状的端到端断言互为正反。
//!
//! 另有一条**结构事实**决定第三格不是「摘表项」造成的：`SessionRecord` 按值持有
//! `SessionActor`，`owned_by` 又是按值克隆，于是 `.await` 期间一定有一份活着的发送端；
//! 摘行丢的只是表里那一份。这条事实由 `摘行丢不掉活着的发送端` 一例钉住。

use async_trait::async_trait;

use super::*;
use crate::connection::error::ProviderError;
use crate::connection::types::{ClientInstanceId, EditorSessionId};
use crate::connection::{
    ConfigRevision, ConnectionId, EffectOutcome, ExecutionTarget, NamespaceTarget, OrganizationId,
    OwnerRef, PrincipalId, SessionContext, Timestamp,
};
use crate::registry::audit::CapabilityVersions;
use crate::registry::backend::{
    CancelOnResource, CloseResource, CloseResourceOutcome, ExecuteOnResource, FinalizeHandles,
    HandleFinalization, OpenResource, OpenedResource, ResourceCancel, ResourceExecution,
};

/// 空闲驱逐期限：固定值，它不是被测对象。
const IDLE_DEADLINE_MS: u64 = 60_000;

/// 三档后端只在 `close()` 上分岔——它就是三态分类表的那个旋钮。
///
/// `execute` / `cancel` 在本文件的三条路径上永远不会被调用（租约失效走的是控制旁路
/// + §9.4 释放例程），所以这两个方法**故意**返回 `Err`：真被调到时立刻失败，
/// 比伪造一个看起来能跑的返回值诚实。
macro_rules! invalidation_only_backend {
    ($(#[$meta:meta])* $name:ident => $close:block) => {
        $(#[$meta])*
        struct $name;

        #[async_trait]
        impl SessionBackend for $name {
            async fn open(
                &self,
                request: OpenResource,
            ) -> Result<OpenedResource, ProviderError> {
                Ok(OpenedResource {
                    context: SessionContext::new(request.initial_target, "tester"),
                    capabilities: CapabilityVersions {
                        contract: "1.0.0".to_owned(),
                        driver_api: "2.3.1".to_owned(),
                    },
                    resource_id: format!("res_{}", request.runtime_epoch),
                    driver_supports_cancel: true,
                })
            }

            async fn execute(
                &self,
                _request: ExecuteOnResource,
            ) -> Result<ResourceExecution, ProviderError> {
                Err(ProviderError::HostRejected("unexpectedExecute".to_owned()))
            }

            async fn cancel(
                &self,
                _request: CancelOnResource,
            ) -> Result<ResourceCancel, ProviderError> {
                Err(ProviderError::HostRejected("unexpectedCancel".to_owned()))
            }

            async fn finalize_handles(
                &self,
                _request: FinalizeHandles,
            ) -> Result<HandleFinalization, ProviderError> {
                Ok(HandleFinalization {
                    finalized: 0,
                    remaining: 0,
                    effect_outcome: EffectOutcome::RolledBack,
                })
            }

            async fn close(
                &self,
                _request: CloseResource,
            ) -> Result<CloseResourceOutcome, ProviderError> {
                $close
            }
        }
    };
}

invalidation_only_backend!(
    /// 释放例程跑得完 ⇒ actor 回 `Ok(true)`：送达，且确属这个 worker。
    ConfirmedCloseBackend => {
        Ok(CloseResourceOutcome::Closed)
    }
);

invalidation_only_backend!(
    /// 物理层关不掉 ⇒ §9.4 走 `SessionLost` 出口：物理资源**已经**被清空
    /// （`state.physical = None`），但登记表这一侧收不到成功视图。
    UndecidableCloseBackend => {
        Ok(CloseResourceOutcome::Undecidable {
            reason: "closeOutcomeUndecidable",
        })
    }
);

invalidation_only_backend!(
    /// 驱动回调把 actor 任务带走 ⇒ 回执还没发出去任务就没了 ⇒ `control()` 拿到 `Err`。
    CrashingCloseBackend => {
        // 这行 panic 是**被测事件本身**，不是断言失败：§9.4 在 actor 任务内部直接
        // `await backend.close(...)`，回调一崩，展开就一路穿过 `release` →
        // `handle_control` → `run_actor`，在 `reply.send(..)` 之前丢掉回执。
        panic!("driverCloseCallbackCrashedTheActorTask")
    }
);

fn db_session_id() -> DbSessionId {
    DbSessionId::new("db_r2_2_1")
}

fn worker_id() -> WorkerId {
    WorkerId::new("w_1")
}

fn open_request() -> OpenRequest {
    OpenRequest {
        db_session_id: db_session_id(),
        worker_id: worker_id(),
        connection_id: ConnectionId::new("conn_1"),
        config_revision: ConfigRevision::new(1),
        owner: OwnerRef::Editor {
            organization_id: OrganizationId::new("org_1"),
            principal_id: PrincipalId::new("pr_1"),
            connection_id: ConnectionId::new("conn_1"),
            client_instance_id: ClientInstanceId::new("cli-1"),
            editor_session_id: EditorSessionId::new("ed-1"),
        },
        initial_target: ExecutionTarget {
            connection_id: ConnectionId::new("conn_1"),
            namespace: NamespaceTarget {
                database: "app".to_owned(),
                catalog: "main".to_owned(),
                schema: "public".to_owned(),
                path: "ns/app".to_owned(),
            },
            object: None,
        },
        expires_at: Timestamp::new("2026-01-01T00:30:00Z"),
        idle_deadline_ms: Some(IDLE_DEADLINE_MS),
    }
}

/// 表项里有一个**按值**持有的 `actor` 字段：不是 `Option`、不是弱句柄、不是裸指针。
///
/// 这行函数能编译，能钉住的**只有这一条**：`SessionRecord` 有一个名为 `actor`、
/// 类型为 `SessionActor`、按值存放的字段。至于 `SessionActor` 内部的两个发送端——
/// 它们的字段对 `registry::actor` 私有，本模块按 Rust 可见性够不着，
/// 所以「表项 ⇒ 活着的发送端」只能靠下面的**行为**断言去钉，不能靠这里的类型。
fn row_owns_the_sender(record: &SessionRecord) -> &SessionActor {
    &record.actor
}

/// 一格分类对登记表造成的后果。四个字段缺一不可：只看「行摘了没」的话，
/// 把三态并成两态的改法照样能过。
#[derive(Debug, PartialEq, Eq)]
struct Classification {
    /// 表项还留在登记表里。
    row_kept: bool,
    /// `invalidate_worker` 的返回值里收了它。
    counted_in_lost: bool,
    /// 额度被记成了该 worker 的陈旧占用。
    quota_held_stale: bool,
    /// 审计里留下了 `quotaHeldStale` 痕迹。
    stale_audit_recorded: bool,
}

/// 观测一次 `invalidate_worker` 的全部后果。
///
/// 取的是累积的 [`SessionRegistry::audit_log`] 而不是 `drain_audit`：要断言的是
/// 「从头到尾没有这条痕迹」，不是「这一次没收到」。
fn observe(registry: &SessionRegistry, lost: &[DbSessionId]) -> Classification {
    Classification {
        row_kept: registry.is_registered(&db_session_id()),
        counted_in_lost: lost.contains(&db_session_id()),
        quota_held_stale: registry.stale_quota_for(&worker_id()) == 1,
        stale_audit_recorded: registry
            .audit_log()
            .iter()
            .any(|entry| entry.kind == AuditKind::QuotaHeldStale),
    }
}

/// 三态表第一格：`Ok(true)`——送达，且确属这个 worker，会话已在 actor 里回滚关闭。
#[tokio::test]
async fn 送达即作废_摘行并把额度挂成stale() {
    let registry = SessionRegistry::new(Arc::new(ConfirmedCloseBackend), 4);
    registry
        .register_session(open_request())
        .await
        .expect("登记必须成功：这是本用例的前提");

    let row = registry
        .read_table()
        .locate(&db_session_id())
        .expect("登记成功后表里必须有一行");
    assert!(
        !row_owns_the_sender(&row).is_closed(),
        "actor 还活着，才谈得上「送达」"
    );

    let lost = registry.invalidate_worker(&worker_id()).await;
    assert_eq!(
        observe(&registry, &lost),
        Classification {
            row_kept: false,
            counted_in_lost: true,
            quota_held_stale: true,
            stale_audit_recorded: true,
        },
        "送达即作废：摘行、计入 lost、额度挂 stale 等隔离确认。\
        少任何一样都是把「已作废」记漏了或多记了"
    );
}

/// 摘行丢不掉活着的发送端——`Err` 分支不能由「摘表项」触发。
///
/// `owned_by` 是按值克隆出 `SessionRecord` 的，克隆里的 `actor` 在 `.await`
/// 期间就是一份活着的发送端；摘行丢掉的只是表里那一份。这是第三格
/// （`Err` 保留表项）与「第二格照常摘行」能同时成立的结构前提。
#[tokio::test]
async fn 摘行丢不掉活着的发送端() {
    let registry = SessionRegistry::new(Arc::new(ConfirmedCloseBackend), 4);
    registry
        .register_session(open_request())
        .await
        .expect("登记必须成功：这是本用例的前提");

    let row = registry
        .read_table()
        .locate(&db_session_id())
        .expect("登记成功后表里必须有一行");
    assert_eq!(
        registry.invalidate_worker(&worker_id()).await,
        vec![db_session_id()],
        "投递送达时会话照常作废，否则额度会被挂成 stale 等一个不会来的隔离确认"
    );
    assert!(
        !registry.is_registered(&db_session_id()),
        "投递送达时必须摘行"
    );
    assert!(
        !row_owns_the_sender(&row).is_closed(),
        "摘表项只丢掉表里那一份克隆；这份按值克隆仍是活着的发送端，\
         所以 control() 的 Err 不可能由摘行触发"
    );
}

/// 三态表第二格：`Ok(false)`——**送达了的否定答复**：actor 说「已无物理资源」。
///
/// 造法：候选原语走完 §9.4 销毁（`destroy_candidate` → 唯一的释放例程把
/// `state.physical` 清空）**之后**才被发布进可见表。这一行于是同时满足
/// 「在册」「actor 活着」「物理资源已没了」，正是 `owned_by` 放行而 actor 回
/// `Ok(false)` 的形状。
///
/// 此前这一格靠 `evict_idle_at` + 关不掉的驱动（`CloseResourceOutcome::Undecidable`）
/// 造成——那是驱逐侧的一处缺陷（收不到成功视图就跳过摘行）留下的残留格。缺陷已闭合，
/// 所以造法改成候选原语；**判定的三态一个字没改**，见本文件头 ★ 段。
#[tokio::test]
async fn 否定答复仍算送达_已无物理资源的会话照常摘行() {
    let registry = SessionRegistry::new(Arc::new(ConfirmedCloseBackend), 4);
    let (actor, view, runtime_epoch) = registry
        .open_candidate(open_request())
        .await
        .expect("候选登记必须成功：这是本用例的前提");
    let candidate_handle = view.handle.clone();
    assert!(
        !registry.is_registered(&db_session_id()),
        "候选在 §12 切换之前对宿主不可见——否则本用例测的就不是销毁过的候选"
    );
    // 唯一的 §9.4 释放例程跑完：物理资源清空、绑定作废，actor 仍活着（邮箱没关）。
    actor
        .destroy_candidate(&candidate_handle)
        .await
        .expect("候选销毁必须走 §9.4 并交出终态");
    registry
        .publish_candidate(worker_id(), runtime_epoch, view, actor)
        .expect("发布销毁过的候选：造出「在册但已无物理资源」这一格");

    assert!(
        registry.is_registered(&db_session_id()),
        "本用例的前提：这一行必须在册，否则租约失效根本不会遍历到它"
    );

    let lost = registry.invalidate_worker(&worker_id()).await;
    assert_eq!(
        observe(&registry, &lost),
        Classification {
            row_kept: false,
            counted_in_lost: true,
            quota_held_stale: true,
            stale_audit_recorded: true,
        },
        "否定答复也是答复：照常摘行、照常计入 lost、照常挂 stale。\
        把 Ok(false) 并进失败分支，这条会话就会永远留在表里，额度永远挂在 stale 上"
    );
}

/// §6.4 / R-01：不可判定的驱逐（§9.4 四步跑完却写出 `SessionLost`）**必须**摘表还额度。
///
/// 这一例钉的是 `evict_idle_at` 逐向判别里「已丢失」那一格**修好之后**的形状。
/// 判据与 [`SessionRegistry::close_registered`] 的 R-01 完全一致：不可判定的是某个
/// 句柄的命运，不是「有没有关掉」——四步已经跑完，关闭请求已经发出，此时留着行
/// 就等于留一个「物理资源已死、行还在表里、额度还扣着」的僵尸：`session_view`
/// 仍查得到它、仍能向一个不存在的资源发执行，而那格额度永久卡死没人会想起。
///
/// 反向也钉：`Err` **不改写**给调用方的语义——这里驱逐如实回「没有干净视图」，
/// 空手而归。把不可判定的会话混进 `evicted` 返回值，等于替它报「已驱逐」。
#[tokio::test]
async fn 不可判定的驱逐摘表还额度_不留僵尸行() {
    let registry = SessionRegistry::new(Arc::new(UndecidableCloseBackend), 4);
    registry
        .register_session(open_request())
        .await
        .expect("登记必须成功：这是本用例的前提");
    let before = registry.remaining_quota();
    assert_eq!(before, 3, "登记占掉一格，剩 3 格");

    let evicted = registry.evict_idle_at(IDLE_DEADLINE_MS).await;
    assert!(
        evicted.is_empty(),
        "物理层关不掉就没有干净视图可交回，驱逐必须空手而归"
    );
    assert!(
        !registry.is_registered(&db_session_id()),
        "§9.4 已跑完 ⇒ 行必须摘除：留着就是一行查得到、能用、却指向已关资源的僵尸登记"
    );
    assert_eq!(
        registry.remaining_quota(),
        4,
        "关闭已发出 ⇒ 额度必须归还，否则每驱逐失败一次就永久卡死一格（R-01 同一种病）"
    );
    assert!(
        registry
            .registered_ids()
            .iter()
            .all(|id| id != &db_session_id()),
        "在册快照必须同步失效，否则重投会撞上死行"
    );
}

/// §6.4：驱逐的请求**没送达**（驱动回调把 actor 任务带走）⇒ 保留表项与额度。
///
/// 这一格与三态表第三格同源，判据也必须一致：回执丢失只证明「没送达或送达不明」，
/// 不证明「资源已关」。此时摘行还额度，等于把一格额度发给一个物理资源可能还活着的
/// 连接，而那条会话从此既不在册也没人负责——比僵尸行更糟，因为它连重投的机会都没了。
///
/// 与 `不可判定的驱逐摘表还额度_不留僵尸行` 成对：两条合起来才说明分类不是
/// 「凡是 Err 都摘」或「凡是 Err 都留」这种偷懒的两态。
#[tokio::test]
async fn 驱逐请求没送达时保留表项_不归还额度() {
    let registry = SessionRegistry::new(Arc::new(CrashingCloseBackend), 4);
    registry
        .register_session(open_request())
        .await
        .expect("登记必须成功：这是本用例的前提");

    let evicted = registry.evict_idle_at(IDLE_DEADLINE_MS).await;
    assert!(
        evicted.is_empty(),
        "actor 任务被驱动回调带走 ⇒ 没有干净视图可交回；stderr 上那行 panic 是预期的"
    );
    assert!(
        registry.is_registered(&db_session_id()),
        "没送达就不能记成已关：行留着，让租约失效/显式关闭那条路去处置它"
    );
    assert_eq!(
        registry.remaining_quota(),
        3,
        "物理资源可能仍活着 ⇒ 这格额度不许发给别人"
    );
}

/// 三态表第三格：`Err(_)`——**没送达**：保留表项，且绝不把额度挂给一个不会到来的
/// 隔离确认。
///
/// 造法是驱动回调崩溃（见 [`CrashingCloseBackend`]）：回执还没发出去，actor 任务
/// 就不在了。tokio 捕获任务里的 panic，`JoinHandle` 又被 `spawn_actor` 丢弃，
/// 所以测试进程不受影响——stderr 上那一行 panic 消息是**预期的**，不是失败。
#[tokio::test]
async fn 投递失败保留表项_额度不挂stale等重投() {
    let registry = SessionRegistry::new(Arc::new(CrashingCloseBackend), 4);
    registry
        .register_session(open_request())
        .await
        .expect("登记必须成功：这是本用例的前提");

    let lost = registry.invalidate_worker(&worker_id()).await;
    assert_eq!(
        observe(&registry, &lost),
        Classification {
            row_kept: true,
            counted_in_lost: false,
            quota_held_stale: false,
            stale_audit_recorded: false,
        },
        "没送达就不能记成已作废：行留着等重投，额度不挂 stale——\
         挂上去的额度记在一个物理资源可能仍活着的会话上，会等一个永远不来的隔离确认"
    );

    // 重投得到同样的答复：控制通道已经断了，行还得留着，等一个换来的新 actor。
    let retried = registry.invalidate_worker(&worker_id()).await;
    assert!(
        retried.is_empty() && registry.is_registered(&db_session_id()),
        "重投必须得到同样的答复：控制通道已断，行不许被顺手摘掉"
    );
    let row = registry
        .read_table()
        .locate(&db_session_id())
        .expect("Err 时行必须留着——这正是本用例的主张");
    assert!(
        row_owns_the_sender(&row).is_closed(),
        "ctrl_rx 已经丢了，同一个 future 里的 exec_rx 必然一起丢：actor 任务确实没了"
    );
}
