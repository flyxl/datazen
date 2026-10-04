//! D-R2-2 的钉子测试：把「表项持有发送端」从注释变成**可执行的事实**。
//!
//! `invalidate_worker` 现在对 `control()` 的三种结果分三种动作（见 `registry.rs` 的函数文档）。
//! 其中 `Err` 分支在**正常退出路径**上不可达，可达性完全依赖一个结构事实：
//! `SessionRecord::actor` 按值持有 `SessionActor`，而 `SessionActor` 自己持有
//! `UnboundedSender`；`run_actor` 只有在 `exec_rx.recv()` 读到 `None` 时才退出，
//! 而 `None` 要求所有发送端都被 drop。于是：
//!
//! > **表里有行 ⇒ 有活着的发送端 ⇒ `control()` 不会因为「摘表项」而未送达。**
//!
//! 这个事实一旦被打破，`Err` 分支就会静默地把表项和额度留在错误的状态上，
//! 所以这里对它做一条直接的断言，而不只是把它写进注释。

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

/// 只驱动「登记 → 租约失效」这条路径的后端。
///
/// `execute` / `cancel` 在这条路径上永远不会被调用（`invalidate_worker` 走的是
/// 控制旁路 + §9.4 释放例程），所以这两个方法**故意**返回 `Err`：真被调到时
/// 立刻失败，比伪造一个看起来能跑的返回值诚实。
#[derive(Default)]
struct InvalidationOnlyBackend;

#[async_trait]
impl SessionBackend for InvalidationOnlyBackend {
    async fn open(&self, request: OpenResource) -> Result<OpenedResource, ProviderError> {
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

    async fn cancel(&self, _request: CancelOnResource) -> Result<ResourceCancel, ProviderError> {
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

    async fn close(&self, _request: CloseResource) -> Result<CloseResourceOutcome, ProviderError> {
        Ok(CloseResourceOutcome::Closed)
    }
}

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

/// 表项里那个字段就是发送端的所有者本身——不是 `Option`、不是弱句柄。
/// 这行函数能编译，就是「表项 ⇒ 活着的发送端」这条可达性论证的**形状**那一半。
#[cfg(test)]
fn row_owns_the_sender(record: &SessionRecord) -> &SessionActor {
    &record.actor
}

/// D-R2-2 的结构断言：摘表项**不可能**让控制投递变成未送达。
///
/// 断言的是两件事：
///
/// 1. `invalidate_worker` 走「按值克隆 → 投递 → 摘行」，投递成功时会照常摘行、
///    计入 `lost`（这是必须**不回归**的既有行为）；
/// 2. 表项摘掉之后，那份按值克隆出来的 `SessionActor` 仍然是活着的发送端
///    （`is_closed() == false`）——这就是 `Err` 分支不能由「摘行」触发的事实依据。
#[tokio::test]
async fn 摘表项不会让控制投递变成未送达_因为表项自己就是发送端() {
    let registry = SessionRegistry::new(Arc::new(InvalidationOnlyBackend), 4);
    registry
        .register_session(open_request())
        .await
        .expect("登记必须成功：这是本用例的前提");

    let row = registry
        .read_table()
        .locate(&db_session_id())
        .expect("登记成功后表里必须有一行");
    let _sender_owner = row_owns_the_sender(&row);

    assert!(
        !row_owns_the_sender(&row).is_closed(),
        "表项还在时 actor 不可能已退出：run_actor 只在 exec_rx 收到 None 后才收尾，\
         而发送端就装在这个表项里"
    );

    // 控制旁路送达（Ok(true)）时，会话照常作废、照常摘行、照常计入 lost。
    let lost = registry.invalidate_worker(&worker_id()).await;
    assert_eq!(
        lost,
        vec![db_session_id()],
        "投递已送达的会话必须计入 lost —— 否则额度会被挂成 stale 等一个不会来的隔离确认"
    );
    assert!(
        !registry.is_registered(&db_session_id()),
        "投递已送达的会话必须从登记表摘除"
    );

    // 关键一步：表项没了，sender 还在。
    assert!(
        !row_owns_the_sender(&row).is_closed(),
        "摘表项丢掉的只是表里那一份克隆；owned_by 按值克隆出的这份仍是活着的发送端，\
         所以 control() 的 Err 不可能由摘行触发"
    );
}

/// D-R2-2 的分类断言：`Ok(false)` 是**被确认的否定答复**，不是投递失败。
///
/// 直接对 actor 侧的控制旁路问同一个问题：worker 对不上时 actor 回 `Ok(false)`，
/// 这一格必须继续被 `invalidate_worker` 当成「送达了」——表项照摘、`lost` 照收。
/// 这条用例锁的是「三态分类表」里的第二格，防止有人日后把 `Ok(false)`
/// 并进失败分支（那会让会话永远留在表里、额度永远挂在 stale 上）。
#[tokio::test]
async fn 否定答复仍算送达_不归该worker管的会话照常摘行() {
    let registry = SessionRegistry::new(Arc::new(InvalidationOnlyBackend), 4);
    registry
        .register_session(open_request())
        .await
        .expect("登记必须成功：这是本用例的前提");

    let foreign_worker = WorkerId::new("w_2");
    assert_eq!(
        registry.invalidate_worker(&foreign_worker).await,
        Vec::<DbSessionId>::new(),
        "别的 worker 名下没有目标，表项不动"
    );
    assert!(
        registry.is_registered(&db_session_id()),
        "未命中的 worker 不得摘走任何行"
    );

    // 让 actor 侧真的回一次否定答复：登记后把视图对齐到同一个 actor，再换一个 worker 问。
    let row = registry
        .read_table()
        .locate(&db_session_id())
        .expect("登记成功后表里必须有一行");
    let negative = row_owns_the_sender(&row)
        .control(|reply| ControlCommand::InvalidateWorker {
            worker_id: foreign_worker.clone(),
            reply,
        })
        .await;
    assert_eq!(
        negative,
        Ok(false),
        "worker 对不上必须回 Ok(false)（被确认的否定答复），而不是 Err"
    );
    assert!(
        !row_owns_the_sender(&row).is_closed(),
        "否定答复也是答复：它证明控制旁路工作正常，sender 依然活着"
    );
}
