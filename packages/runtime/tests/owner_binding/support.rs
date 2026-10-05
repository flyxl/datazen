//! 本测试私有的夹具：按句柄脚本化的 `SessionPort` + 记录读写次数的幂等存储。
//!
//! **不是共享夹具。** `tests/gateway_fixtures/mod.rs` 与
//! `src/gateway/testing_support.rs` 都不复用，理由见 `tests/owner_binding.rs`
//! 的文件头——共用夹具会让「集成测试验证了真实公开 API」这件事失真。这里只是
//! 把同一个测试按单文件 800 行惯例拆成两个文件，语义上仍是「一个自足的测试」。
//!
//! 这里的会话与 ID 全部为合成数据，不含凭据或真实连接信息。
//!
//! 模块内的东西允许未使用：不是每个测试都要用到夹具里的每一件。

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use datazen_platform_api::error::ApiErrorCode;
use datazen_platform_api::id::{ClientInstanceId, EditorSessionId, JobId};
use datazen_runtime::connection::capability::PreciseCancel;
use datazen_runtime::connection::{
    AttachmentState, CloseMode, CommandCall, ConfigRevision, ConnectionId, Counter, DbSessionId,
    ExecuteInSessionRequest, ExecutionId, ExecutionReceipt, ExecutionState, ExecutionTarget,
    NamespaceTarget, OrganizationId, OwnerRef, PrincipalId, RuntimeError, SessionContext,
    SessionHandle, SessionState, SessionView, Timestamp,
};
use datazen_runtime::gateway::idempotency::IdempotencyStoreError;
use datazen_runtime::gateway::owner_binding::OwnerMatchAuthorizer;
use datazen_runtime::gateway::{
    CancelBinding, CancelRequest, ExecutionGateway, ExecutionRequest, ExecutionSource, FixedClock,
    GatewayError, IdempotencyRecord, IdempotencyScope, IdempotencyStore, InMemoryIdempotencyStore,
    RequestPrincipal, SourceKind,
};
use datazen_runtime::registry::SessionPort;

/// 归属组织 O1：U1 与 U2 同属一个组织，用来看清「同组织 ≠ 同一主体」。
pub const ORG_O1: &str = "org-ob-1";
/// 归属组织 O2。
pub const ORG_O2: &str = "org-ob-2";
pub const PRINCIPAL_U1: &str = "principal-ob-u1";
pub const PRINCIPAL_U2: &str = "principal-ob-u2";

/// U1 自己名下的编辑器会话。
pub const U1_SESSION: &str = "db_session-ob-u1";
/// U2 自己名下的编辑器会话。
pub const U2_EDITOR_SESSION: &str = "db_session-ob-u2-editor";
/// U2 自己名下的作业会话。
pub const U2_JOB_SESSION: &str = "db_session-ob-u2-job";
/// O2 名下的编辑器会话：组织维度也要测。
pub const O2_EDITOR_SESSION: &str = "db_session-ob-o2-editor";
/// 一个从来不存在过的会话 id。
pub const ABSENT_SESSION: &str = "db_session-ob-absent";
/// 一个长得像 `connectionId` 的配置 id —— CM-04 拿它冒充会话句柄。
pub const PROFILE_ID: &str = "cnx-ob-profile-0000";

/// 夹具里的 `contextRevision`。
pub const REVISION: u64 = 11;
/// 夹具里的幂等键。
pub const IDEMPOTENCY_KEY: &str = "idem-ob";

// ---------------------------------------------------------------------------
// 夹具
// ---------------------------------------------------------------------------

pub fn namespace() -> NamespaceTarget {
    NamespaceTarget::unknown_placeholder()
}

pub fn editor_owner(organization_id: &str, principal_id: &str) -> OwnerRef {
    OwnerRef::Editor {
        organization_id: OrganizationId::new(organization_id),
        principal_id: PrincipalId::new(principal_id),
        connection_id: ConnectionId::new("cnx-ob"),
        client_instance_id: ClientInstanceId::new("cli-ob"),
        editor_session_id: EditorSessionId::new("edt-ob"),
    }
}

pub fn job_owner(organization_id: &str) -> OwnerRef {
    OwnerRef::Job {
        organization_id: OrganizationId::new(organization_id),
        job_id: JobId::new("job-ob"),
        stage_id: "stage-ob".to_string(),
    }
}

pub fn handle(db_session_id: &str) -> SessionHandle {
    SessionHandle {
        db_session_id: DbSessionId::new(db_session_id),
        runtime_epoch: Counter::new(1),
    }
}

pub fn view(db_session_id: &str, owner: OwnerRef, state: SessionState) -> SessionView {
    let connection_id = ConnectionId::new("cnx-ob");
    SessionView {
        handle: handle(db_session_id),
        connection_id: connection_id.clone(),
        config_revision: ConfigRevision::new(3),
        owner,
        initial_target: ExecutionTarget {
            connection_id,
            namespace: namespace(),
            object: None,
        },
        observed_context: SessionContext::new(namespace(), "dz_identity_ob"),
        context_revision: Counter::new(REVISION),
        state,
        attachment_state: AttachmentState::Attached,
        active_execution_id: None,
        expires_at: Timestamp::new("9999-01-01T00:00:00Z"),
    }
}

pub fn ready_view(db_session_id: &str, owner: OwnerRef) -> SessionView {
    view(db_session_id, owner, SessionState::Ready)
}

pub fn principal(principal_id: &str, organization_id: &str) -> RequestPrincipal {
    RequestPrincipal::new(
        PrincipalId::new(principal_id),
        OrganizationId::new(organization_id),
        DbSessionId::new("org_session-ob"),
    )
}

pub fn u1() -> RequestPrincipal {
    principal(PRINCIPAL_U1, ORG_O1)
}

pub fn u2() -> RequestPrincipal {
    principal(PRINCIPAL_U2, ORG_O1)
}

pub fn u1_of_o2() -> RequestPrincipal {
    principal(PRINCIPAL_U1, ORG_O2)
}

pub fn u2_of_o2() -> RequestPrincipal {
    principal(PRINCIPAL_U2, ORG_O2)
}

pub fn editor_source(principal_id: &str, organization_id: &str) -> ExecutionSource {
    ExecutionSource::new(
        SourceKind::Editor,
        "edt-ob",
        Some(OrganizationId::new(organization_id)),
        Some(PrincipalId::new(principal_id)),
    )
}

pub fn call() -> CommandCall {
    CommandCall {
        command: "select 1".to_string(),
        input: serde_json::json!({"marker": "ob"}),
    }
}

pub fn request_for(db_session_id: &str, source: ExecutionSource) -> ExecutionRequest {
    ExecutionRequest::new(
        handle(db_session_id),
        Counter::new(REVISION),
        call(),
        IDEMPOTENCY_KEY,
        source,
    )
}

pub fn request_with_revision(db_session_id: &str, expected: u64) -> ExecutionRequest {
    let mut request = request_for(db_session_id, editor_source(PRINCIPAL_U1, ORG_O1));
    request.expected_context_revision = Counter::new(expected);
    request
}

/// 按句柄脚本化的端口：三个世界（U1 的、U2 的、不存在的）同时在线。
pub struct ScriptedPort {
    pub views: Mutex<HashMap<String, SessionView>>,
    pub view_calls: AtomicU64,
    pub execute_calls: AtomicU64,
    pub cancel_calls: AtomicU64,
    pub close_calls: AtomicU64,
}

impl ScriptedPort {
    pub fn new(views: Vec<SessionView>) -> Arc<Self> {
        Arc::new(Self {
            views: Mutex::new(
                views
                    .into_iter()
                    .map(|v| (v.handle.db_session_id.as_str().to_string(), v))
                    .collect(),
            ),
            view_calls: AtomicU64::new(0),
            execute_calls: AtomicU64::new(0),
            cancel_calls: AtomicU64::new(0),
            close_calls: AtomicU64::new(0),
        })
    }

    pub fn execute_calls(&self) -> u64 {
        self.execute_calls.load(Ordering::SeqCst)
    }

    pub fn cancel_calls(&self) -> u64 {
        self.cancel_calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl SessionPort for ScriptedPort {
    async fn session_view(&self, handle: &SessionHandle) -> Result<SessionView, RuntimeError> {
        self.view_calls.fetch_add(1, Ordering::SeqCst);
        // 对齐 `SessionRegistry::session_view` → `locate()`：查不到 ⇒
        // `UnknownSession(句柄字符串)`；查得到 ⇒ `Ok`，**终态也是 `Ok`**。
        self.views
            .lock()
            .expect("scripted views")
            .get(handle.db_session_id.as_str())
            .cloned()
            .ok_or_else(|| RuntimeError::UnknownSession(handle.db_session_id.to_string()))
    }

    async fn execute_in_session(
        &self,
        _request: ExecuteInSessionRequest,
    ) -> Result<ExecutionReceipt, RuntimeError> {
        self.execute_calls.fetch_add(1, Ordering::SeqCst);
        // 本文件所有断言都是「驱动执行次数必须是 0」。真的走到这里就是缺陷，
        // 用一个内部错把失败现场顶出来，而不是伪造一个成功回执掩盖它。
        Err(RuntimeError::InvariantBroken(
            "scripted port must never execute under the owner-binding gate",
        ))
    }

    async fn cancel_execution(
        &self,
        _handle: &SessionHandle,
        _execution_id: &ExecutionId,
    ) -> Result<ExecutionState, RuntimeError> {
        self.cancel_calls.fetch_add(1, Ordering::SeqCst);
        Err(RuntimeError::InvariantBroken(
            "scripted port must never cancel under the owner-binding gate",
        ))
    }

    async fn close_session(
        &self,
        _handle: &SessionHandle,
        _mode: CloseMode,
    ) -> Result<(), RuntimeError> {
        self.close_calls.fetch_add(1, Ordering::SeqCst);
        Err(RuntimeError::InvariantBroken(
            "scripted port must never close under the owner-binding gate",
        ))
    }
}

/// 会数账本读写次数的幂等存储：CM-06 要证明「不创建会话」时，不能只看内存里
/// 少了一条记录——还得证明连账本都没写过一行。
pub struct CountingStore {
    pub inner: Arc<InMemoryIdempotencyStore>,
    pub reads: AtomicU64,
    pub writes: AtomicU64,
}

impl CountingStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: InMemoryIdempotencyStore::shared(),
            reads: AtomicU64::new(0),
            writes: AtomicU64::new(0),
        })
    }

    pub fn writes(&self) -> u64 {
        self.writes.load(Ordering::SeqCst)
    }

    pub fn reads(&self) -> u64 {
        self.reads.load(Ordering::SeqCst)
    }
}

impl IdempotencyStore for CountingStore {
    fn read(
        &self,
        scope: &IdempotencyScope,
    ) -> Result<Option<IdempotencyRecord>, IdempotencyStoreError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.inner.read(scope)
    }

    fn write(
        &self,
        scope: &IdempotencyScope,
        record: IdempotencyRecord,
    ) -> Result<(), IdempotencyStoreError> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.inner.write(scope, record)
    }
}

/// 构造被测世界：U1 的编辑器会话、U2 的编辑器会话、U2 的作业会话、O2 的编辑器会话。
pub fn world() -> (Arc<ScriptedPort>, Arc<CountingStore>) {
    let port = ScriptedPort::new(vec![
        ready_view(U1_SESSION, editor_owner(ORG_O1, PRINCIPAL_U1)),
        ready_view(U2_EDITOR_SESSION, editor_owner(ORG_O1, PRINCIPAL_U2)),
        ready_view(U2_JOB_SESSION, job_owner(ORG_O1)),
        ready_view(O2_EDITOR_SESSION, editor_owner(ORG_O2, PRINCIPAL_U2)),
    ]);
    (port, CountingStore::new())
}

pub fn gateway(port: Arc<ScriptedPort>, store: Arc<CountingStore>) -> ExecutionGateway {
    ExecutionGateway::new(
        port,
        Arc::new(OwnerMatchAuthorizer),
        store,
        FixedClock::shared(),
    )
}

/// 把网关错误投影成审计落盘形态。CM-05 的「无法观察存在性」断言就是在比这两个值。
pub fn projection(error: &GatewayError) -> serde_json::Value {
    error.to_persistable_json()
}

/// 对外错误码。CM-04 断言的是 §13 的 `sessionNotFound`。
pub fn api_code(error: &GatewayError) -> Option<ApiErrorCode> {
    match error {
        GatewayError::Runtime(err) => err.api_code(),
        // 失败文本刻意不含错误值：错误里带句柄，回显它等于把不该出现的东西写进断言输出。
        _ => panic!("期望 GatewayError::Runtime，实际是别的变体"),
    }
}

pub fn cancel_request(execution_id: &ExecutionId, db_session_id: &str) -> CancelRequest {
    CancelRequest::new(
        CancelBinding::new(execution_id.clone(), handle(db_session_id)),
        PreciseCancel::Supported,
    )
}
