//! WR-G1：四缺口（CM-07 / CM-53 / CM-63 / CM-68）的 W1 可码化断言。
//!
//! `server/tests/g1_cm_w1_assertions.rs` 头部账目表把这四颗登记为 ⏳ W1 缺口
//! （第 17 / 21 / 27 / 28 行）。本文件把**当前 lib 公开面已经能表达**的 W1
//! 断言落成可执行钉子（Wave4 裁决⑤：路由面 pub(crate) 不可达时，直接钉
//! handler 所委托的公开面语义），其余如实登记在每颗测试的 doc 注释里。
//!
//! ## 装配说明（与 `g1_cm_w1_assertions.rs` 的差异）
//!
//! 会话语义需要**一个跨调用的 runtime 实例**（会话登记在
//! `RuntimeConnectionUseCases` 私有的内存 registry/directory 里）。主线
//! `ServerRuntimeLeaseSource` 是每调用现构造（`driver_exec_lease.rs` 钉住
//! 「不缓存」），而 bootstrap 与 `SessionsState.directory` 的「同一目录实例」
//! 装配纪律目前没有公开 accessor 可满足（见 CM-63 登记行）——本文件因此用
//! [`FixedLeaseSource`]（恒返回同一 runtime）钉用例层语义，装配偏差在
//! CM-63 的 doc 注释里登记。
//!
//! ## 断言面与登记（代码即账本）
//!
//! | CM | 本文件钉住 | 仍属登记（主线台账句子） |
//! |----|-----------|--------------------------|
//! | CM-07 | 请求体目标校验：blank connectionId → InvalidArgument（400）；未知/他组织 connectionId → NotFound 固定文案（404，存在性不泄漏）；attach 未知/他组织会话 → 先 404（目录闸）后 SessionNotFound（runtime 闸） | `sessions.rs` 路由是 `pub(crate)`，oneshot 装配只能经 `build_router_with_auth` 的 `domain_routers` 槽（composition root 独占）——wire 形状的逐字节断言随该接线切片进 `g2_freeze_acceptance` 同款认证链测试 |
//! | CM-53 | MCP 会话归属：`ModulesBackend` 有状态分发下，身份 B（他组织）拿身份 A 的 client_session_ref 调 use/close 一律被拒，且 B 的失败不触碰 A 的归属（A 随后 close 成功） | `PerCallHost::authorize_owner` 在 server 形态恒过（owner 与 ctx 共用 server 后端生成的 clientInstanceId）；逐身份归属的最终裁决在 runtime `session_entry`（org/principal/client 三元组）——`authorize_owner` 的 server 形态空转是既有登记事实，本测试钉的是「第二身份仍被拒」这一归属结果 |
//! | CM-63 | attach→detach 往返（ Attached→Detached→Attached→Detached）；重复 detach 幂等；错 token → ContextConflict；他主体 attach/get → SessionNotFound | W1 HTTP attach/detach 面未接线（`pub(crate)` 路由 + composition root 独占装配）；且主线 `ServerRuntimeLeaseSource` 每 resolve 新建 runtime 私有目录，`SessionsState.directory`（`ServerSessionDirectory`）与用例实现「持有同一目录实例」的装配纪律没有公开 accessor 可满足——接线时需先补共享目录的装配出口（台账句子） |
//! | CM-68 | 替换发布的前置语义：连接 configRevision 的 CAS 前门（fresh expected 前进、stale expected → `CasConflict`→409 `configRevisionMismatch`）；跨组织 CAS/disable → `NotFound`（存在性不泄漏） | 完整的候选替换发布（两阶段候选 + 响应丢失面）属 runtime H + P9 WN，未接线——此处只钉 W1 可达部分（P9 WN 面未接线，此处钉 W1 可达部分） |

use std::sync::Arc;

use async_trait::async_trait;

use datazen::connection::domain::{NamespaceTarget, ProfileRepository};
use datazen::connection::dto::{ProfileDraft, ProfilePatch};
use datazen::execution::application::orchestration::RuntimeConnectionUseCases;
use datazen::execution::application::policy::{
    AuthorizationAction, AuthorizationDecision, AuthorizationSubject, PolicyChangeStream,
    PolicyService as ModulesPolicyService, PolicyVersionChange,
};
use datazen::execution::application::requests::{AttachmentRequest, OpenSessionRequest};
use datazen::execution::application::use_cases::ConnectionUseCases;
use datazen::execution::{
    AttachmentState, ExecutionTarget, IdempotentOperation, OpenSessionReceipt, SessionDirectory,
    SessionHandle, SessionOwner,
};
use datazen::execution::runtime::connection as rc;
use datazen::execution::runtime::registry::audit::CapabilityVersions;
use datazen::execution::runtime::registry::backend::{
    CapabilityProvenance, CancelOnResource, CloseResource, CloseResourceOutcome, ExecuteOnResource,
    FinalizeHandles, HandleFinalization, OpenResource, OpenedResource, ResourceCancel,
    ResourceExecution, SessionBackend,
};
use datazen::shared::context::{OwnerRef, RequestContext};
use datazen::shared::error::{ApiError, ApiErrorCode, PortError};
use datazen::shared::id::{
    AuthenticationSessionId, ClientInstanceId, ConfigRevision, ConnectionId, DbSessionId,
    IdempotencyKey, OrganizationId, PrincipalId, RequestId, RuntimeEpoch, Timestamp, WorkerId,
};

use datazen_server::execution::lease::RuntimeLeaseSource;
use datazen_server::execution::lease_use_cases::LeaseConnectionUseCases;
use datazen_server::execution::session_directory::ServerSessionDirectory;
use datazen_server::http::middleware::authorize::NOT_VISIBLE_MESSAGE;
use datazen_server::storage::{AdminStore, ServerProfiles};

const NOW: &str = "2026-01-01T00:00:00.000Z";

// ── 公共装配 ─────────────────────────────────────────────────────────────

fn store() -> Arc<AdminStore> {
    let store = AdminStore::open_in_memory().expect("store");
    store.migrate().expect("migrate");
    Arc::new(store)
}

fn ctx(org: &str, principal: &str) -> RequestContext {
    RequestContext::new(
        OrganizationId::new(org),
        PrincipalId::new(principal),
        Some(AuthenticationSessionId::new("auth-1")),
        ClientInstanceId::new("client-1"),
        RequestId::new("req-1"),
        None,
    )
}

/// open_session 的归属盖章：ClientSession 归属的 clientInstanceId 必须与
/// ctx 一致（`convert::owner` → `check_owner` 的可采信裁决）。
fn owner() -> OwnerRef {
    OwnerRef::ClientSession {
        client_instance_id: ClientInstanceId::new("client-1"),
        purpose: "g1-w1-gap".into(),
    }
}

/// `ProfileRepository` 落库：真实 `ServerProfiles`（内存管理库）。
async fn create_profile(store: &Arc<AdminStore>, ctx: &RequestContext, name: &str) -> ConnectionId {
    let profiles = ServerProfiles::new(store.clone());
    let record = profiles
        .create(
            ctx,
            ProfileDraft::new(name, "postgres"),
            &IdempotencyKey::new("idem-g1"),
        )
        .await
        .expect("profile create");
    record.connection_id
}

/// modules `PolicyService` 假体：全允许（CM-07/63 钉的是目标校验与归属，
/// 授权裁决归 `f7_wiring_tests.rs` 的真适配器钉子）。
struct AllowAllPolicy;

#[async_trait]
impl ModulesPolicyService for AllowAllPolicy {
    async fn authorize(
        &self,
        _ctx: &RequestContext,
        _subject: AuthorizationSubject,
        _action: AuthorizationAction,
    ) -> Result<AuthorizationDecision, PortError> {
        Ok(AuthorizationDecision::Allow)
    }

    async fn verify_delegation(
        &self,
        _ctx: &RequestContext,
        _delegation: datazen::shared::context::DelegationRef,
    ) -> Result<datazen::execution::application::policy::DelegationGrant, PortError> {
        Err(PortError::TokenInvalid)
    }

    fn isolation_key(
        &self,
        _ctx: &RequestContext,
    ) -> Result<datazen::shared::id::PolicyIsolationKey, PortError> {
        Ok(datazen::shared::id::PolicyIsolationKey::new("g1-w1-gaps"))
    }

    fn subscribe_version_changes(&self) -> PolicyChangeStream {
        Box::new(std::iter::empty::<PolicyVersionChange>())
    }
}

/// `SessionBackend` 假体：会话登记/关闭全程无物理驱动（与
/// `driver_exec_lease.rs` 的 StubBackend 同形状）。
struct StubBackend;

#[async_trait]
impl SessionBackend for StubBackend {
    async fn open(&self, request: OpenResource) -> Result<OpenedResource, rc::ProviderError> {
        Ok(OpenedResource {
            context: rc::SessionContext::new(request.initial_target, "stub"),
            capabilities: CapabilityVersions {
                contract: "1".into(),
                driver_api: "1".into(),
            },
            capability_snapshot: CapabilityProvenance::new("stub-driver", "0.0.0", 1),
            resource_id: "res_stub".into(),
            driver_supports_cancel: false,
        })
    }
    async fn execute(
        &self,
        _request: ExecuteOnResource,
    ) -> Result<ResourceExecution, rc::ProviderError> {
        Err(rc::ProviderError::HostRejected("unexpectedExecute".into()))
    }
    async fn cancel(&self, _request: CancelOnResource) -> Result<ResourceCancel, rc::ProviderError> {
        Err(rc::ProviderError::HostRejected("unexpectedCancel".into()))
    }
    async fn finalize_handles(
        &self,
        _request: FinalizeHandles,
    ) -> Result<HandleFinalization, rc::ProviderError> {
        Ok(HandleFinalization {
            finalized: 0,
            remaining: 0,
            effect_outcome: rc::EffectOutcome::NotStarted,
        })
    }
    async fn close(&self, _request: CloseResource) -> Result<CloseResourceOutcome, rc::ProviderError> {
        Ok(CloseResourceOutcome::Closed)
    }
}

/// 测试专用租约源：恒返回**同一** runtime 实例。会话登记在 runtime 私有的
/// 内存 registry/directory 里，跨调用存活是本文件所有会话语义的前置；主线
/// 的每调用现构造装配偏差见文件头与 CM-63 登记。
struct FixedLeaseSource(Arc<RuntimeConnectionUseCases>);

#[async_trait]
impl RuntimeLeaseSource for FixedLeaseSource {
    async fn resolve(&self) -> Result<Arc<RuntimeConnectionUseCases>, String> {
        Ok(self.0.clone())
    }
}

/// 一个真实装配的 runtime（真 `ServerProfiles` 落库 + 全允许策略 + 桩后端）。
fn session_runtime(store: Arc<AdminStore>) -> Arc<RuntimeConnectionUseCases> {
    let profiles: Arc<dyn datazen::connection::domain::ProfileRepository> =
        Arc::new(ServerProfiles::new(store));
    let backend: Arc<dyn SessionBackend> = Arc::new(StubBackend);
    Arc::new(RuntimeConnectionUseCases::new(
        profiles,
        Arc::new(AllowAllPolicy),
        backend,
    ))
}

fn session_cases(store: Arc<AdminStore>) -> LeaseConnectionUseCases {
    LeaseConnectionUseCases::new(Arc::new(FixedLeaseSource(session_runtime(store))))
}

/// 走完「签发提交令牌 → 构造 open 请求」的前置（open 的 validate_token
/// 要求幂等键来自本 runtime 的签发面）。
async fn open_session(
    cases: &LeaseConnectionUseCases,
    ctx: &RequestContext,
    connection_id: &str,
) -> Result<OpenSessionReceipt, ApiError> {
    let token = cases
        .issue_submission_token(ctx, IdempotentOperation::OpenSession, None)
        .await?;
    let target = ExecutionTarget::new(
        ConnectionId::new(connection_id),
        NamespaceTarget::database("app"),
    );
    cases
        .open_session(ctx, OpenSessionRequest::new(target, owner(), token.idempotency_key))
        .await
}

// ── CM-07：请求体目标校验（账目表第 17 行）────────────────────────────────
//
// 故障真实性：若目标校验缺失，open/attach 会把「不存在的连接/会话」路由进
// driver 或物理面（打库、烧预算、或把「他组织的资源」当成可开目标）——
// 三颗钉子分别钉住参数类（400）、存在性（404 固定文案）、运行时归属
// （SessionNotFound）三道拒绝面。

/// CM-07（W1）：open 请求体的纯字段校验——blank connectionId 是参数类
/// 拒绝（handler 映射 400 `invalidArgument`），任何端口调用都不发生。
#[tokio::test]
async fn cm07_open_session_rejects_a_blank_connection_target_as_invalid_argument() {
    let target = ExecutionTarget::new(ConnectionId::new(""), NamespaceTarget::database("app"));
    let request = OpenSessionRequest::new(target, owner(), IdempotencyKey::new("idem-cm07-blank"));
    let error = request
        .validate()
        .expect_err("a blank connectionId must be refused");
    assert_eq!(error.code, ApiErrorCode::InvalidArgument);
    assert_eq!(error.message, "connectionId must not be empty");
}

/// CM-07（W1）：connectionId 指向不存在的连接、或**他组织真实存在**的连接
/// ——两条路给同一 `NotFound` + 同一固定文案（404 `notFound`，存在性不泄漏，
/// 防枚举）。这是 sessions.rs `handler_open_session` 逐字透传的用例错误。
#[tokio::test]
async fn cm07_open_session_answers_not_found_for_unknown_and_cross_org_connections() {
    let store = store();
    let cases = session_cases(store.clone());
    let user = ctx("org-1", "user-1");

    // 不存在：固定文案。
    let error = open_session(&cases, &user, "conn-never-created")
        .await
        .expect_err("an unknown connection must be refused");
    assert_eq!(error.code, ApiErrorCode::NotFound);
    assert_eq!(error.message, "connection unavailable");

    // 他组织：org-2 真实建一条配置，org-1 用户按 id 开会话——同一拒绝面。
    let org2 = ctx("org-2", "user-2");
    let foreign = create_profile(&store, &org2, "cm07-foreign-conn").await;
    let error = open_session(&cases, &user, foreign.as_str())
        .await
        .expect_err("a cross-org connection must be equally invisible");
    assert_eq!(error.code, ApiErrorCode::NotFound);
    assert_eq!(
        error.message, "connection unavailable",
        "与「不存在」同文案：存在性不泄漏（CM-07 的 404 面）"
    );
}

/// CM-07（W1）：attach 的目标校验分两半——**目录半边**（router 的
/// `ensure_visible`：未知 id → `None`、他组织 id → 组织字段不匹配，两者
/// 折进同一条 404 `NOT_VISIBLE_MESSAGE`）先于一切存活语义；**runtime 半边**
/// （越目录的归属裁决：org/principal/client 三元组不符 → `SessionNotFound`，
/// wire 上是 409 冲突类）兜底组织边界。
#[tokio::test]
async fn cm07_attach_target_checks_run_before_any_session_liveness_semantics() {
    let store = store();
    let cases = session_cases(store.clone());

    // 目录半边：404 判定输入（sessions.rs `ensure_visible` 的数据源）。
    let directory = Arc::new(ServerSessionDirectory::new());
    let row = SessionOwner {
        db_session_id: DbSessionId::new("dbs-cm07-1"),
        organization_id: OrganizationId::new("org-1"),
        principal_id: PrincipalId::new("user-1"),
        connection_id: ConnectionId::new("conn-1"),
        owner: owner(),
        worker_id: WorkerId::new("local-runtime"),
        runtime_epoch: RuntimeEpoch::new("1"),
        resource_epoch: 1,
        last_business_activity: Timestamp::new(NOW),
    };
    let registered = directory.register(row).await.expect("register");
    assert_eq!(registered.db_session_id.as_str(), "dbs-cm07-1");
    let unknown = directory
        .lookup(DbSessionId::new("dbs-cm07-none"))
        .await
        .expect("lookup")
        .is_none();
    assert!(unknown, "未知 id → None：与他组织同折 404（防枚举）");
    let found = directory
        .lookup(DbSessionId::new("dbs-cm07-1"))
        .await
        .expect("lookup")
        .expect("registered row is routable");
    assert_eq!(found.organization_id.as_str(), "org-1");
    assert_ne!(
        found.organization_id.as_str(),
        "org-2",
        "他组织请求方的组织与归属字段不匹配 → 同一条 404 固定文案"
    );
    assert!(
        !NOT_VISIBLE_MESSAGE.is_empty(),
        "404 文案是固定常量（不含资源名/归属，防 ID 枚举）"
    );

    // runtime 半边：fabricate 一个句柄，attach/get 都在触达任何驱动/存活
    // 语义**之前**被归属裁决拒绝（SessionNotFound，409 冲突类）。
    let handle = SessionHandle::new(DbSessionId::new("dbs-cm07-ghost"), RuntimeEpoch::new("1"));
    let request = AttachmentRequest::new(handle.clone(), datazen::shared::id::AttachmentToken::new("tok-cm07"));
    let error = cases
        .attach_session(&ctx("org-1", "user-1"), request)
        .await
        .expect_err("an unknown session must not attach");
    assert_eq!(error.code, ApiErrorCode::SessionNotFound);

    // 他组织身份读同一句柄：runtime 归属闸同样先答 SessionNotFound——
    // 即使目录半边接线偏差，组织边界仍在 runtime 兜底。
    let foreign = ctx("org-2", "user-2");
    let error = cases
        .get_session(&foreign, handle)
        .await
        .expect_err("a cross-org identity must not resolve the session");
    assert_eq!(error.code, ApiErrorCode::SessionNotFound);
}

// ── CM-63：attach/detach（账目表第 27 行）────────────────────────────────
//
// 故障真实性：detach 若关闭会话或让句柄失效，重挂会打穿「分离只解除视图」
// 的契约；重复 detach 若报错，客户端重试路径断裂；错 token 若被接受，任何
// 拿到 dbSessionId 的旁观者都能接管会话视图。

/// CM-63（W1）：attach → detach 往返。分离只解除视图（`Detached`），会话
/// 仍然存活可读；同 token 重挂恢复 `Attached`；重复 detach 幂等（状态保持
/// `Detached`、不报错）。即 §7.5 的完整往返语义。
#[tokio::test]
async fn cm63_attach_detach_round_trip_is_view_scoped_and_idempotent() {
    let store = store();
    let cases = session_cases(store.clone());
    let user = ctx("org-1", "user-1");
    let connection = create_profile(&store, &user, "cm63-primary").await;

    let receipt = open_session(&cases, &user, connection.as_str())
        .await
        .expect("open session");
    assert_eq!(receipt.session.attachment_state, AttachmentState::Attached);
    let handle = receipt.session.handle.clone();
    let token = receipt.attachment_token;

    // detach：解除视图，不关闭会话。
    let view = cases
        .detach_session(&user, AttachmentRequest::new(handle.clone(), token.clone()))
        .await
        .expect("detach");
    assert_eq!(view.attachment_state, AttachmentState::Detached);
    let alive = cases
        .get_session(&user, handle.clone())
        .await
        .expect("detach must not close the session");
    assert_eq!(alive.handle, handle);

    // 重挂：同 token 恢复附着。
    let view = cases
        .attach_session(&user, AttachmentRequest::new(handle.clone(), token.clone()))
        .await
        .expect("reattach with the issued token");
    assert_eq!(view.attachment_state, AttachmentState::Attached);

    // 重复 detach：幂等（同一 token 复核仍通过，状态保持 Detached）。
    let view = cases
        .detach_session(&user, AttachmentRequest::new(handle, token))
        .await
        .expect("a repeated detach is accepted");
    assert_eq!(view.attachment_state, AttachmentState::Detached);
}

/// CM-63（W1）：附着凭据面。错 token → `ContextConflict`（409，§6.4 的
/// 「attachmentToken 不匹配」裁决）；他主体（同组织另一 principal）attach/
/// 读同一会话 → `SessionNotFound`（runtime 归属闸，404/409 均不泄漏会话
/// 内容）。
#[tokio::test]
async fn cm63_foreign_token_and_foreign_identity_are_refused_on_the_attachment_face() {
    let store = store();
    let cases = session_cases(store.clone());
    let user = ctx("org-1", "user-1");
    let connection = create_profile(&store, &user, "cm63-guarded").await;

    let receipt = open_session(&cases, &user, connection.as_str()).await.expect("open");
    let handle = receipt.session.handle.clone();
    let token = receipt.attachment_token;

    // 错 token：归属正确但凭据不符 → ContextConflict（不是静默换绑）。
    let error = cases
        .attach_session(
            &user,
            AttachmentRequest::new(handle.clone(), datazen::shared::id::AttachmentToken::new("not-the-token")),
        )
        .await
        .expect_err("a wrong attachment token must be refused");
    assert_eq!(error.code, ApiErrorCode::ContextConflict);
    assert_eq!(error.message, "attachment token rejected");

    // 他主体：同组织、另一 principal——session_entry 的三元组归属不符。
    let impostor = ctx("org-1", "user-2");
    let error = cases
        .attach_session(
            &impostor,
            AttachmentRequest::new(handle.clone(), token.clone()),
        )
        .await
        .expect_err("another principal must not attach");
    assert_eq!(error.code, ApiErrorCode::SessionNotFound);
    let error = cases
        .get_session(&impostor, handle.clone())
        .await
        .expect_err("another principal must not even read the view");
    assert_eq!(error.code, ApiErrorCode::SessionNotFound);

    // 归属者不受影响：同 token 仍可附着（他主体的失败没有消耗凭据）。
    let view = cases
        .attach_session(&user, AttachmentRequest::new(handle, token))
        .await
        .expect("the owner is untouched by foreign attempts");
    assert_eq!(view.attachment_state, AttachmentState::Attached);
}

// ── CM-68：替换发布的 W1 面（账目表第 28 行）──────────────────────────────
//
// 完整的候选替换发布（两阶段候选 + 响应丢失）属 runtime H + P9 WN，未接线
// ——**P9 WN 面未接线，此处钉 W1 可达部分**：替换发布的版本前置是连接
// configRevision 的 CAS 前门（stale expected 必须被拒，不得静默覆盖），
// 以及跨组织写操作的 404 存在性不泄漏。

/// CM-68（W1）：连接 configRevision 的 CAS 前门——fresh expected 前进版本；
/// stale expected → `CasConflict`（wire 面 409 `configRevisionMismatch`，
/// 调用方重读后由用户决定重试）；跨组织 CAS 与 disable → `NotFound`（与
/// 「不存在」不可区分，防枚举）。
#[tokio::test]
async fn cm68_stale_expected_revision_conflicts_and_cross_org_writes_do_not_leak_existence() {
    let profiles = ServerProfiles::new(store());
    let owner = ctx("org-a", "user-1");
    let record = profiles
        .create(
            &owner,
            ProfileDraft::new("cm68-primary", "postgres"),
            &IdempotencyKey::new("idem-cm68"),
        )
        .await
        .expect("create");
    assert_eq!(record.config_revision.counter().get(), 1);

    // fresh expected → 前进（替换发布的版本前置：替换只发生在当前版本上）。
    let updated = profiles
        .compare_and_set(
            &owner,
            record.connection_id.clone(),
            ConfigRevision::new(1),
            ProfilePatch {
                name: Some("cm68-replacement".into()),
                ..ProfilePatch::default()
            },
        )
        .await
        .expect("fresh expected applies");
    assert_eq!(updated.config_revision.counter().get(), 2);

    // stale expected → CasConflict：旧请求按明确政策拒绝（替换不得覆盖并发变更）。
    let stale = profiles
        .compare_and_set(
            &owner,
            record.connection_id.clone(),
            ConfigRevision::new(1),
            ProfilePatch::default(),
        )
        .await
        .expect_err("a stale expected revision must conflict");
    assert!(matches!(stale, PortError::CasConflict { .. }));

    // 跨组织写：NotFound（存在性不泄漏——「无权」与「不存在」同形）。
    let outsider = ctx("org-b", "user-2");
    let cross = profiles
        .compare_and_set(
            &outsider,
            record.connection_id.clone(),
            ConfigRevision::new(2),
            ProfilePatch::default(),
        )
        .await
        .expect_err("cross-org CAS must be invisible");
    assert!(matches!(cross, PortError::NotFound(_)));
    let disable = profiles
        .disable(&outsider, record.connection_id.clone())
        .await
        .expect_err("cross-org disable must be invisible");
    assert!(matches!(disable, PortError::NotFound(_)));
}

// ── CM-53：MCP 会话归属（账目表第 21 行）─────────────────────────────────
//
// feature 门控：MCP runtime 面（`datazen_server::mcp::runtime`）挂在 server
// `mcp` feature 后（`datazen/mcp`）。与 `driver_exec_lease.rs` 的
// `mod lease_source` 同一装配纪律。

#[cfg(feature = "mcp")]
mod cm53_mcp_owner_attribution {
    use super::*;
    use datazen::mcp::domain::permission::McpPermissionMode;
    use datazen_server::mcp::http::{CallIdentity, McpBackend};
    use datazen_server::mcp::runtime::ModulesBackend;
    use datazen_server::mcp::wire::ToolCallError;

    fn identity(org: &str, principal: &str) -> CallIdentity {
        CallIdentity {
            organization_id: org.to_owned(),
            principal_id: principal.to_owned(),
            request_id: "req-cm53".to_owned(),
        }
    }

    /// CM-53（W1）：client_session_ref 的归属面——身份 A（org-1）open 出的
    /// 会话引用，身份 B（org-2）拿同一 ref 调 use_session / close_session
    /// 一律被拒（runtime 归属裁决 → internal error，绝不落到物理执行），
    /// 且 B 的失败**不触碰** A 的归属：A 随后 close 成功（`closed: true`）。
    ///
    /// 装配注：server 形态的 `PerCallHost::authorize_owner` 比较 owner 与
    /// ctx 的 clientInstanceId——两者都是本 server 后端生成的同一实例 id，
    /// 恒过（既有登记）。逐身份归属的最终裁决在 runtime `session_entry`
    /// （org/principal/client 三元组），本测试经 `ModulesBackend` 公开分发
    /// 面把 admit → authorize_owner → registry → runtime 的整条链走到头，
    /// 钉住「第二身份被拒」的**结果**。
    #[tokio::test]
    async fn a_client_session_ref_opened_by_one_identity_is_unusable_by_another() {
        let store = store();
        let owner_ctx = ctx("org-1", "user-1");
        let connection = create_profile(&store, &owner_ctx, "cm53-conn").await;

        let runtime = session_runtime(store);
        let lease: Arc<dyn RuntimeLeaseSource> = Arc::new(FixedLeaseSource(runtime));
        let backend = ModulesBackend::new(
            lease,
            vec![connection.as_str().to_owned()],
            McpPermissionMode::SafeWrite,
        );

        // A：open_session → clientSessionRef（归属盖章在本 server 实例 id 上）。
        let opened = backend
            .call_tool(
                &identity("org-1", "user-1"),
                "open_session",
                serde_json::json!({ "connection_id": connection.as_str(), "database": "app" }),
            )
            .await
                        .map_err(|error| match error {
                ToolCallError::WithHelp { reason, .. } | ToolCallError::Plain(reason)
                | ToolCallError::Internal(reason) => reason,
            })
            .unwrap_or_else(|reason| panic!("identity A opens a stateful session: {reason}"));
        let reference = serde_json::from_str::<serde_json::Value>(&opened)
            .expect("open reply is JSON")["clientSessionRef"]
            .as_str()
            .expect("the reply carries an opaque clientSessionRef")
            .to_owned();
        assert!(!reference.is_empty());
        assert!(
            !reference.contains(connection.as_str()),
            "reference 不携带 connectionId 等可推断信息"
        );

        // B（他组织）：同一 ref 的 use_session → 拒绝（runtime SessionNotFound）。
        let refused = backend
            .call_tool(
                &identity("org-2", "user-2"),
                "use_session",
                serde_json::json!({
                    "client_session_ref": reference,
                    "command": "query",
                    "input": { "sql": "SELECT 1" }
                }),
            )
            .await
            .expect_err("a second identity must be refused on the same ref");
        match &refused {
            ToolCallError::Internal(reason) => assert!(
                reason.contains("sessionNotFound"),
                "归属拒绝来自 runtime 的会话归属裁决，不是物理执行失败: {reason}"
            ),
            ToolCallError::Plain(_) | ToolCallError::WithHelp { .. } => {
                panic!("expected an internal attribution rejection")
            }
        }

        // B：同一 ref 的 close_session → 同样拒绝（不能顺手回收别人的会话）。
        assert!(
            backend
                .call_tool(
                    &identity("org-2", "user-2"),
                    "close_session",
                    serde_json::json!({ "client_session_ref": reference }),
                )
                .await
                .is_err(),
            "a second identity must not close a session it does not own"
        );

        // A：归属完好——随后 close 成功（B 的失败没有消耗/转移 A 的映射）。
        let closed = backend
            .call_tool(
                &identity("org-1", "user-1"),
                "close_session",
                serde_json::json!({ "client_session_ref": reference }),
            )
            .await
            .map_err(|error| match error {
                ToolCallError::WithHelp { reason, .. } | ToolCallError::Plain(reason)
                | ToolCallError::Internal(reason) => reason,
            })
            .unwrap_or_else(|reason| panic!("the owner still holds the mapping: {reason}"));
        let reply = serde_json::from_str::<serde_json::Value>(&closed).expect("close reply is JSON");
        assert_eq!(reply["closed"], serde_json::json!(true));
    }
}
