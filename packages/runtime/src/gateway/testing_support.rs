//! 网关单测共用的夹具。
//!
//! 只在 `--lib` 测试下编译：集成测试有自己的 `tests/gateway_fixtures/`，
//! 两份夹具刻意不共享代码——共享会让「集成测试验证了真实公开 API」这件事失真。
//!
//! 这里造出来的 `SessionView` 是**合成**数据，不含任何凭据或真实连接信息。

use crate::connection::{
    AttachmentState, ConfigRevision, ConnectionId, Counter, DbSessionId, ExecutionId,
    NamespaceTarget, OrganizationId, OwnerRef, PrincipalId, SessionContext, SessionHandle,
    SessionState, SessionView, Timestamp,
};
use datazen_platform_api::id::{ClientInstanceId, EditorSessionId};

/// 命名空间目标。
pub fn namespace() -> NamespaceTarget {
    NamespaceTarget {
        database: "dz_ns_a".to_owned(),
        catalog: "dz_catalog_a".to_owned(),
        schema: "dz_schema_a".to_owned(),
        path: String::new(),
    }
}

/// 编辑器归属。
pub fn editor_owner() -> OwnerRef {
    OwnerRef::Editor {
        organization_id: OrganizationId::new("org_a"),
        principal_id: PrincipalId::new("principal_a"),
        connection_id: ConnectionId::new("cnx_a"),
        client_instance_id: ClientInstanceId::new("cli_a"),
        editor_session_id: EditorSessionId::new("edt_a"),
    }
}

/// 任意会话视图。
pub fn view(
    db_session_id: &str,
    runtime_epoch: u64,
    context_revision: u64,
    state: SessionState,
) -> SessionView {
    let connection_id = ConnectionId::new("cnx_a");
    SessionView {
        handle: SessionHandle {
            db_session_id: DbSessionId::new(db_session_id),
            runtime_epoch: Counter::new(runtime_epoch),
        },
        connection_id: connection_id.clone(),
        config_revision: ConfigRevision::new(3),
        owner: editor_owner(),
        initial_target: crate::connection::ExecutionTarget {
            connection_id,
            namespace: namespace(),
            object: None,
        },
        observed_context: SessionContext::new(namespace(), "dz_identity_a"),
        context_revision: Counter::new(context_revision),
        state,
        attachment_state: AttachmentState::Attached,
        active_execution_id: None,
        expires_at: Timestamp::new("9999-01-01T00:00:00Z"),
    }
}

/// 处于 `Ready` 的会话视图：可以受理执行。
pub fn ready_view(
    db_session_id: DbSessionId,
    runtime_epoch: Counter,
    context_revision: Counter,
) -> SessionView {
    view(
        db_session_id.as_str(),
        runtime_epoch.get(),
        context_revision.get(),
        SessionState::Ready,
    )
}

/// 句柄。
pub fn handle(db_session_id: &str, runtime_epoch: u64) -> SessionHandle {
    SessionHandle {
        db_session_id: DbSessionId::new(db_session_id),
        runtime_epoch: Counter::new(runtime_epoch),
    }
}

/// 一个执行 id。
pub fn execution_id(value: &str) -> ExecutionId {
    ExecutionId::new(value)
}

/// 故意在**持有锁**的情况下 panic，把互斥量弄成中毒态。
///
/// 存在的唯一理由是让「锁中毒后某个闸门是否 fail-closed」可被测试。
/// `poison!(` 是生产路径禁用词（`tests/gateway_contract/invariants.rs`），
/// 所以这一行只许写在 `#[cfg(test)]` 的本模块里，不许搬回生产文件。
#[cfg(test)]
pub(crate) fn poison_lock<T>(lock: &std::sync::Mutex<T>) {
    let _held = match lock.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    panic!("intentional lock poisoning for the fail-closed test");
}
