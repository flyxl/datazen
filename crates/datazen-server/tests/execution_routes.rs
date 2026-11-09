//! execution 域路由集成钉子（契约 §6.10；执行域路由收口刀，oneshot 驱动、
//! 进程内装配）。
//!
//! 覆盖：
//!
//! - 会话内执行往返：令牌签发（用例面）→ `POST
//!   /api/v1/sessions/{id}/executions` 202 + `ExecutionReceipt` →
//!   `GET /api/v1/executions/{id}` 200 + `ExecutionView` →
//!   `POST /api/v1/executions/{id}/cancel` 200 + `CancelReceipt`。
//! - 闸序：体缺令牌 400、路径 `{id}` 与体 `handle.dbSessionId` 不一致
//!   400、越组织会话 404（合并口径）、未知执行 404。
//! - 目标执行：体缺令牌 400（受理路径的用例面契约由 use_case_contracts
//!   会话域规格覆盖，本文件不重复后端桩语义）。
//!
//! L2 以请求扩展直接注入 `AuthenticatedSession`（先例照
//! `tests/f1_security_aihost.rs`）；用例面 = 共享规格夹具装配的真实
//! `RuntimeConnectionUseCases`（`use_case_contracts.rs` 的
//! `sessions_contract_service` 同构：`ContractPolicy` +
//! `ContractSessionBackend` + `InMemorySessionDirectory`）。

use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::http::{header, Request as HttpRequest, StatusCode};
use axum::Router;
use tower::ServiceExt;

use datazen::execution::application::orchestration::{
    RuntimeConnectionUseCases, RuntimeResultSink, SystemRuntimeClock,
};
use datazen::execution::application::requests::OpenSessionRequest;
use datazen::execution::application::use_cases::ConnectionUseCases;
use datazen::execution::{IdempotentOperation, SessionDirectory};
use datazen::shared::context::{OwnerRef, RequestContext};
use datazen::shared::id::{ClientInstanceId, OrganizationId, PrincipalId, RequestId};
use datazen::use_case_contracts_test_support::connections::{
    MemoryProfileStore, CONTRACT_ORGANIZATION,
};
use datazen::use_case_contracts_test_support::sessions::{
    contract_profile_record, contract_target, ContractPolicy, ContractSessionBackend,
    CONTRACT_CONNECTION,
};
use datazen_server::http::domains::execution::{execution_router, ExecutionState};
use datazen_server::http::middleware::auth::AuthenticatedSession;
use datazen_server::storage::login_sessions::LoginSessionMode;

// ── 装配 ────────────────────────────────────────────────────────────────────

/// 会话目录与用例**同一实例**（生产装配纪律的镜像：可见性闸查的就是用例
/// open 时登记的那份归属事实）。
fn execution_service() -> (
    Arc<dyn ConnectionUseCases>,
    Arc<dyn SessionDirectory>,
    Arc<MemoryProfileStore>,
) {
    let store = Arc::new(MemoryProfileStore::default());
    let directory =
        Arc::new(datazen::execution::runtime::directory::InMemorySessionDirectory::new());
    let use_cases = Arc::new(RuntimeConnectionUseCases::new_with_parts(
        store.clone(),
        Arc::new(ContractPolicy),
        Arc::new(ContractSessionBackend::default()),
        Arc::new(SystemRuntimeClock(Instant::now())),
        RuntimeResultSink::default(),
        directory.clone(),
    ));
    (use_cases, directory, store)
}

fn execution_app(state: ExecutionState) -> Router {
    execution_router(state)
}

fn session_of(org: &str, principal: &str) -> AuthenticatedSession {
    AuthenticatedSession {
        organization_id: org.into(),
        principal_id: principal.into(),
        session_id: "sess-1".into(),
        permission_version: 1,
        token_epoch: 0,
        auth_mode: LoginSessionMode::Cookie,
    }
}

/// 契约身份的 HTTP 侧变体：client_instance 与 SessionIdentity 提取器的
/// 硬编码值（"http"）一致——令牌签发与请求核验的 client 维度必须同源。
fn contract_http_context() -> RequestContext {
    RequestContext::new(
        OrganizationId::new(CONTRACT_ORGANIZATION),
        PrincipalId::new("user-contract"),
        None,
        ClientInstanceId::new("http"),
        RequestId::new("req-http"),
        None,
    )
}

/// 契约身份的变体（跨组织闸测试用第二主体；client_instance 与契约身份
/// 一致——client 维度不是本文件的判据）。
fn stranger_context() -> RequestContext {
    RequestContext::new(
        OrganizationId::new("org-stranger"),
        PrincipalId::new("user-stranger"),
        None,
        ClientInstanceId::new("http"),
        RequestId::new("req-http"),
        None,
    )
}

/// 开一个契约目标的会话（open 的闸序已由 sessions 域钉子覆盖，此处直驱）。
async fn open_contract_session(
    cases: &Arc<dyn ConnectionUseCases>,
    ctx: &RequestContext,
) -> datazen::execution::SessionHandle {
    let token = cases
        .issue_submission_token(ctx, IdempotentOperation::OpenSession, None)
        .await
        .expect("open submission token");
    let open = cases
        .open_session(
            ctx,
            OpenSessionRequest::new(
                contract_target(),
                OwnerRef::ClientSession {
                    client_instance_id: ClientInstanceId::new("http"),
                    purpose: "execution-routes".to_owned(),
                },
                token.idempotency_key,
            ),
        )
        .await
        .expect("open session");
    open.session.handle
}

// ── oneshot 助手 ────────────────────────────────────────────────────────────

async fn call_json(
    app: &mut Router,
    method: &str,
    uri: &str,
    session: Option<&AuthenticatedSession>,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let builder = HttpRequest::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    let mut request = match body {
        Some(value) => builder
            .body(Body::from(value.to_string()))
            .expect("request body"),
        None => builder.body(Body::empty()).expect("request"),
    };
    if let Some(session) = session {
        request.extensions_mut().insert(session.clone());
    }
    let response = app.oneshot(request).await.expect("oneshot dispatch");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("body bytes");
    let value = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    };
    (status, value)
}

// ── 往返：会话内执行 → 视图 → 取消 ──────────────────────────────────────

#[tokio::test]
async fn session_execution_round_trip_answers_receipt_view_and_cancel() {
    let (cases, directory, store) = execution_service();
    store.seed(vec![contract_profile_record()]).await;
    let mut app = execution_app(ExecutionState {
        sessions: cases.clone(),
        directory,
        // 落账器默认关闭（本套件钉路由语义；历史落账在 query_assets_routes 套件端到端验证）。
        history: None,
    });
    let ctx = contract_http_context();
    let session = session_of(CONTRACT_ORGANIZATION, "user-contract");

    // ① 开会话（用例面直驱）+ 签发执行令牌（会话绑定操作，绑句柄）。
    let handle = open_contract_session(&cases, &ctx).await;
    let db_session_id = handle.db_session_id.as_str().to_owned();
    let token = cases
        .issue_submission_token(&ctx, IdempotentOperation::ExecuteInSession, Some(&handle))
        .await
        .expect("execute submission token");
    let idempotency_key = token.idempotency_key.as_str().to_owned();

    // ② 会话内执行 → 202 + ExecutionReceipt（已接受 ≠ SQL 成功）。
    let (status, receipt) = call_json(
        &mut app,
        "POST",
        &format!("/api/v1/sessions/{db_session_id}/executions"),
        Some(&session),
        Some(serde_json::json!({
            "handle": handle,
            "call": { "command": "contract-execute", "input": {} },
            "idempotencyKey": idempotency_key,
            "submissionToken": { "idempotencyKey": idempotency_key },
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "executeInSession accepts asynchronously: {receipt}"
    );
    let execution_id = receipt["executionId"]
        .as_str()
        .expect("executionId")
        .to_owned();
    assert!(
        receipt["streamId"].as_str().is_some(),
        "receipt carries the stable stream id: {receipt}"
    );

    // ③ 执行视图：GET /executions/{id} → 200 ExecutionView。
    let (status, view) = call_json(
        &mut app,
        "GET",
        &format!("/api/v1/executions/{execution_id}"),
        Some(&session),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "execution view: {view}");
    assert_eq!(view["executionId"], receipt["executionId"]);
    assert!(
        view["state"].as_str().is_some(),
        "view answers the execution state: {view}"
    );

    // ④ 取消登记：POST /executions/{id}/cancel，体只留幂等凭据 → 200
    //    CancelReceipt（终态执行按 alreadyFinished 收敛是合法取值）。
    let (status, cancel) = call_json(
        &mut app,
        "POST",
        &format!("/api/v1/executions/{execution_id}/cancel"),
        Some(&session),
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": idempotency_key },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "cancel registration: {cancel}");
    assert_eq!(cancel["executionId"], receipt["executionId"]);
    assert!(
        cancel["disposition"].as_str().is_some(),
        "cancel receipt answers a disposition: {cancel}"
    );
    assert_eq!(
        cancel["state"], "succeeded",
        "terminal state replays: {cancel}"
    );
}

// ── 闸序钉子 ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn execute_in_session_requires_a_submission_token() {
    let (cases, directory, store) = execution_service();
    store.seed(vec![contract_profile_record()]).await;
    let mut app = execution_app(ExecutionState {
        sessions: cases.clone(),
        directory,
        // 落账器默认关闭（本套件钉路由语义；历史落账在 query_assets_routes 套件端到端验证）。
        history: None,
    });
    let ctx = contract_http_context();
    let session = session_of(CONTRACT_ORGANIZATION, "user-contract");
    let handle = open_contract_session(&cases, &ctx).await;
    let db_session_id = handle.db_session_id.as_str().to_owned();

    let (status, wire) = call_json(
        &mut app,
        "POST",
        &format!("/api/v1/sessions/{db_session_id}/executions"),
        Some(&session),
        Some(serde_json::json!({
            "handle": handle,
            "call": { "command": "contract-execute", "input": {} },
            "idempotencyKey": "key-no-token",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "token gate: {wire}");
    assert_eq!(wire["code"], "invalidArgument");
}

#[tokio::test]
async fn execute_in_session_rejects_handle_path_mismatch() {
    let (cases, directory, store) = execution_service();
    store.seed(vec![contract_profile_record()]).await;
    let mut app = execution_app(ExecutionState {
        sessions: cases.clone(),
        directory,
        // 落账器默认关闭（本套件钉路由语义；历史落账在 query_assets_routes 套件端到端验证）。
        history: None,
    });
    let ctx = contract_http_context();
    let session = session_of(CONTRACT_ORGANIZATION, "user-contract");
    // 两个本组织会话：路径指向 A、体句柄指向 B——可见（不触发 404 合并）
    // 但句柄不一致，命中一致性闸 400。
    let handle_a = open_contract_session(&cases, &ctx).await;
    let handle_b = open_contract_session(&cases, &ctx).await;
    let db_session_id_a = handle_a.db_session_id.as_str().to_owned();
    let token = cases
        .issue_submission_token(&ctx, IdempotentOperation::ExecuteInSession, Some(&handle_b))
        .await
        .expect("execute submission token");
    let idempotency_key = token.idempotency_key.as_str().to_owned();

    let (status, wire) = call_json(
        &mut app,
        "POST",
        &format!("/api/v1/sessions/{db_session_id_a}/executions"),
        Some(&session),
        Some(serde_json::json!({
            "handle": handle_b,
            "call": { "command": "contract-execute", "input": {} },
            "idempotencyKey": idempotency_key,
            "submissionToken": { "idempotencyKey": idempotency_key },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "consistency gate: {wire}");
    assert_eq!(wire["code"], "invalidArgument");
}

#[tokio::test]
async fn cross_org_session_is_not_visible_to_executions() {
    let (cases, directory, store) = execution_service();
    store.seed(vec![contract_profile_record()]).await;
    let mut app = execution_app(ExecutionState {
        sessions: cases.clone(),
        directory,
        // 落账器默认关闭（本套件钉路由语义；历史落账在 query_assets_routes 套件端到端验证）。
        history: None,
    });
    let ctx = contract_http_context();
    let stranger = session_of("org-stranger", "user-stranger");
    let handle = open_contract_session(&cases, &ctx).await;
    let db_session_id = handle.db_session_id.as_str().to_owned();

    // 越组织：可见性闸 404 合并口径（防枚举，与 sessions 域同一条固定文案）。
    let (status, wire) = call_json(
        &mut app,
        "POST",
        &format!("/api/v1/sessions/{db_session_id}/executions"),
        Some(&stranger),
        Some(serde_json::json!({
            "handle": handle,
            "call": { "command": "contract-execute", "input": {} },
            "idempotencyKey": "key-cross-org",
            "submissionToken": { "idempotencyKey": "key-cross-org" },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "org gate: {wire}");
    assert_eq!(wire["code"], "notFound");
}

#[tokio::test]
async fn cross_org_execution_read_is_uniform_404() {
    let (cases, directory, store) = execution_service();
    store.seed(vec![contract_profile_record()]).await;
    let mut app = execution_app(ExecutionState {
        sessions: cases.clone(),
        directory,
        // 落账器默认关闭（本套件钉路由语义；历史落账在 query_assets_routes
        // 套件端到端验证）。
        history: None,
    });
    let ctx = contract_http_context();
    let session = session_of(CONTRACT_ORGANIZATION, "user-contract");
    let stranger = session_of("org-stranger", "user-stranger");

    // 本主体提交一笔会话内执行（受理即可，不追终态）。
    let handle = open_contract_session(&cases, &ctx).await;
    let db_session_id = handle.db_session_id.as_str().to_owned();
    let token = cases
        .issue_submission_token(&ctx, IdempotentOperation::ExecuteInSession, Some(&handle))
        .await
        .expect("execute submission token");
    let idempotency_key = token.idempotency_key.as_str().to_owned();
    let (status, receipt) = call_json(
        &mut app,
        "POST",
        &format!("/api/v1/sessions/{db_session_id}/executions"),
        Some(&session),
        Some(serde_json::json!({
            "handle": handle,
            "call": { "command": "contract-execute", "input": {} },
            "idempotencyKey": idempotency_key,
            "submissionToken": { "idempotencyKey": idempotency_key },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{receipt}");
    let execution_id = receipt["executionId"]
        .as_str()
        .expect("executionId")
        .to_owned();

    // 越组织读取：404 合并口径（与提交/取消同一条 notFound 文案；用例层
    // visible_to 归因——防枚举纪律的跨组织腿，与同组织第二主体钉互补）。
    let (status, wire) = call_json(
        &mut app,
        "GET",
        &format!("/api/v1/executions/{execution_id}"),
        Some(&stranger),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "cross-org read: {wire}");
    assert_eq!(wire["code"], "notFound");
}

#[tokio::test]
async fn unknown_execution_reads_as_not_found() {
    let (cases, directory, _store) = execution_service();
    let mut app = execution_app(ExecutionState {
        sessions: cases.clone(),
        directory,
        // 落账器默认关闭（本套件钉路由语义；历史落账在 query_assets_routes 套件端到端验证）。
        history: None,
    });
    let session = session_of(CONTRACT_ORGANIZATION, "user-contract");

    let (status, wire) = call_json(
        &mut app,
        "GET",
        "/api/v1/executions/no-such-execution",
        Some(&session),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "unknown execution: {wire}");
    assert_eq!(wire["code"], "notFound");

    let (status, wire) = call_json(
        &mut app,
        "POST",
        "/api/v1/executions/no-such-execution/cancel",
        Some(&session),
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": "key-cancel-unknown" },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "unknown cancel: {wire}");
    assert_eq!(wire["code"], "notFound");
}

#[tokio::test]
async fn execute_at_target_requires_a_submission_token() {
    let (cases, directory, store) = execution_service();
    store.seed(vec![contract_profile_record()]).await;
    let mut app = execution_app(ExecutionState {
        sessions: cases.clone(),
        directory,
        // 落账器默认关闭（本套件钉路由语义；历史落账在 query_assets_routes 套件端到端验证）。
        history: None,
    });
    let session = session_of(CONTRACT_ORGANIZATION, "user-contract");

    let (status, wire) = call_json(
        &mut app,
        "POST",
        "/api/v1/executions",
        Some(&session),
        Some(serde_json::json!({
            "target": {
                "connectionId": CONTRACT_CONNECTION,
                "namespace": { "database": "app", "catalog": null, "schema": null, "path": [] },
            },
            "expectedConfigRevision": 1,
            "call": { "command": "contract-execute", "input": {} },
            "idempotencyKey": "key-at-target",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "token gate: {wire}");
    assert_eq!(wire["code"], "invalidArgument");
}

// ══════════════════════════════════════════════════════════════════════════
// W8 刀1对账（PR #204 关闭后的增量移植）：全栈夹具钉子。
//
// 上面 6 颗是域路由直挂的 oneshot 钉（请求扩展注入 L2）；下面的钉子走
// `build_router_with_auth` 的**真认证链**（真登录会话 cookie + CSRF +
// rbac L6）+ MockDriver 全栈（真租约源 / 执行后端 / 发布钩子；cm63
// `http_fixture` 同构）——补 main 缺的行为面：
//
// - `executeAtTarget` 绿路径（identity 路由 runtime 轨签发 → 202 → 轮询
//   终态 succeeded）；
// - `executeInSession` 全 HTTP 绿路径（开会话 → 带 handle 签发 → 202 →
//   终态）；
// - 未知会话的 404 固定文案（`NOT_VISIBLE_MESSAGE`，防枚举不可区分）；
// - 同组织第二主体读他人执行 → 统一 404「execution unavailable」
//   （`visible_to` 五元组；与未知 id 同文案，无存在性预言机）；
// - 令牌两轨不混用（HMAC 轨令牌喂执行路由 → 409 `idempotencyExpired`）；
// - `cancelExecution` 签发缺 handle 显式拒；未知执行取消统一 404；
// - aiHost.executeTarget `independent` 全栈绿路径 + 未知连接 503。
// ══════════════════════════════════════════════════════════════════════════

use std::path::PathBuf;

use datazen::connection::domain::catalog::{ConnectionRepository as _, ConnectionScope};
use datazen::connection::domain::profile::ProfileRepository as _;
use datazen::connection::dto::ProfileDraft;
use datazen::shared::id::IdempotencyKey;

use datazen_driver_api::capabilities;
use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::resource_adapter::LegacyResourceAdapter;
use datazen_driver_api::{DatabaseDriver, DatabaseDriverFactory, SslMode};

use datazen_server::csrf::{self, CsrfSecret};
use datazen_server::execution::budget::ServerBudgetCoordinator;
use datazen_server::execution::budget_bridge::BudgetPortBridge;
use datazen_server::execution::driver_exec::ServerDriverExecBackend;
use datazen_server::execution::lease::ServerRuntimeLeaseSource;
use datazen_server::execution::lease_use_cases::LeaseConnectionUseCases;
use datazen_server::execution::policy_adapter::ServerExecutionPolicy;
use datazen_server::execution::publisher::execution_result_publisher;
use datazen_server::execution::resource_host::ServerResourceExecutionHost;
use datazen_server::execution::secret_provider::ServerSecretVault;
use datazen_server::execution::submission_token::ServerSubmissionTokenIssuer;
use datazen_server::http::domains::ai_host::{ai_host_router, AiHostState};
use datazen_server::http::domains::sessions::{sessions_router, SessionsState};
use datazen_server::http::middleware::authorize::NOT_VISIBLE_MESSAGE;
use datazen_server::http::router::{build_router_with_auth, AuthDeps, DomainState};
use datazen_server::policy::{PolicyService, Role};
use datazen_server::storage::login_sessions::NewLoginSession;
use datazen_server::storage::{
    AdminStore, LoginSessions, ServerAudit, ServerConnections, ServerJobs, ServerProfiles,
    ServerSettingsStore,
};

const FULLSTACK_SECRET: &[u8] = b"execution-routes-fullstack-secret";

// ── MockDriver 工厂（cm63 WireMockFactory 同款）──────────────────────────────

struct RouteMockFactory {
    driver_id: &'static str,
    driver: Arc<dyn DatabaseDriver>,
}

impl DatabaseDriverFactory for RouteMockFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::clone(&self.driver)
    }

    fn driver_id(&self) -> &'static str {
        self.driver_id
    }

    fn resource_provider(&self) -> Option<Arc<dyn datazen_driver_api::resource::ResourceProvider>> {
        let shape = datazen_driver_api::namespace::NamespaceShape {
            levels: vec![
                datazen_driver_api::namespace::NamespaceLevel::present(
                    datazen_driver_api::namespace::NamespaceLevelKind::Database,
                    false,
                ),
                datazen_driver_api::namespace::NamespaceLevel::absent(
                    datazen_driver_api::namespace::NamespaceLevelKind::Catalog,
                ),
                datazen_driver_api::namespace::NamespaceLevel::present(
                    datazen_driver_api::namespace::NamespaceLevelKind::Schema,
                    false,
                ),
            ],
            ..datazen_driver_api::namespace::NamespaceShape::default()
        };
        let mut caps = capabilities::CapabilitySet::default();
        caps.stateful_session = capabilities::Availability::Supported;
        caps.namespace_switch = capabilities::NamespaceSwitch::Unsupported;
        caps.context_observation = capabilities::ContextObservation::Full;
        caps.session_scoped_handles = capabilities::SessionScopedHandleSupport::Supported;
        caps.reset_for_reuse = capabilities::ResetForReuse::Unsupported;
        Some(Arc::new(LegacyResourceAdapter::new(
            Arc::clone(&self.driver),
            self.driver_id,
            "execution-routes",
            1,
            shape,
            caps,
        )))
    }
}

// ── 全栈夹具 ────────────────────────────────────────────────────────────────

struct FullstackFixture {
    _dir: PathBuf,
    _store: Arc<AdminStore>,
    app: axum::Router,
    connection_id: String,
    profile_revision: u64,
    /// org-1/user-1 的真 cookie（登录会话 + CSRF 双令牌）。
    cookie: String,
    csrf: String,
    /// org-1/user-2 的真 cookie（同组织第二主体，visible_to 钉子用）。
    cookie_user2: String,
    csrf_user2: String,
}

async fn fullstack_fixture() -> FullstackFixture {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("dz-exec-routes-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let store = Arc::new(AdminStore::open(&dir.join("admin.sqlite")).expect("file store"));
    store.migrate().expect("migrations apply");

    // 真 membership（org-1 两主体 + org-2 一主体）+ 真登录会话 + 真 CSRF
    // 令牌（g1 冻结项先例：真实时钟戳，硬编码日期会腐烂成 401）。
    let policy = PolicyService::new(store.clone(), FULLSTACK_SECRET.to_vec()).expect("policy");
    policy
        .create_membership("org-1", "user-1", Role::OrgMember, "idp:user-1")
        .expect("membership user-1");
    policy
        .create_membership("org-1", "user-2", Role::OrgMember, "idp:user-2")
        .expect("membership user-2");
    policy
        .create_membership("org-2", "user-2", Role::OrgMember, "idp:user-2-org2")
        .expect("membership org-2");

    let now = chrono::Utc::now();
    let rfc3339 = |value: chrono::DateTime<chrono::Utc>| {
        value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    };
    let login = |session_id: &str, org: &str, principal: &str| NewLoginSession {
        organization_id: org.to_owned(),
        session_id: session_id.to_owned(),
        principal_id: principal.to_owned(),
        auth_time: rfc3339(now),
        issued_at: rfc3339(now),
        absolute_expires_at: rfc3339(now + chrono::Duration::hours(24)),
        idle_expires_at: rfc3339(now + chrono::Duration::hours(8)),
        permission_version: 1,
        client_binding: "ua|execution-routes".to_owned(),
        mode: LoginSessionMode::Cookie,
        native_secret_hash: None,
    };
    LoginSessions::new(store.clone())
        .create(login("sess-exec-u1", "org-1", "user-1"))
        .expect("login session user-1");
    LoginSessions::new(store.clone())
        .create(login("sess-exec-u2", "org-1", "user-2"))
        .expect("login session user-2");
    let csrf_secret = CsrfSecret::new(FULLSTACK_SECRET.to_vec()).expect("csrf secret");
    let csrf = csrf::mint(&csrf_secret, "sess-exec-u1").expect("mint csrf");
    let csrf_user2 = csrf::mint(&csrf_secret, "sess-exec-u2").expect("mint csrf");
    let cookie = format!("__Host-dz_session=sess-exec-u1; __Host-dz_csrf={csrf}");
    let cookie_user2 = format!("__Host-dz_session=sess-exec-u2; __Host-dz_csrf={csrf_user2}");

    // HTTP 面 SessionIdentity 固定铸造 `http`（router.rs 提取器）——令牌
    // 签发与请求核验的 client 维度必须同源。
    let ctx = RequestContext::new(
        OrganizationId::new("org-1"),
        PrincipalId::new("user-1"),
        None,
        ClientInstanceId::new("http"),
        RequestId::new("req-exec-routes"),
        None,
    );

    // 驱动注册表：懒形 + 测试驱动 + 测试工厂（闸序可达驱动层）。
    let registry = Arc::new(datazen::execution::runtime::driver_registry::DriverRegistry::new());
    let mut opts = MockDriverOptions::default();
    opts.query_rows = vec![vec![Some(datazen_driver_api::Value::Integer(1))]];
    let driver = MockDriver::new("mockdb", opts);
    registry
        .register_test_driver("mockdb", driver.clone())
        .await;
    registry
        .register_test_factory(
            "mockdb",
            Box::leak(Box::new(RouteMockFactory {
                driver_id: "mockdb",
                driver,
            })),
        )
        .await;

    let vault = Arc::new(
        ServerSecretVault::new(store.clone(), FULLSTACK_SECRET.to_vec()).expect("secret vault"),
    );
    let connections = Arc::new(ServerConnections::new(store.clone(), vault));
    let profiles = Arc::new(ServerProfiles::new(store.clone()));
    let record = profiles
        .create(
            &ctx,
            ProfileDraft::new("exec-routes-conn", "mockdb"),
            &IdempotencyKey::new("idem-exec-routes-profile"),
        )
        .await
        .expect("profile create");
    let connection_id = record.connection_id.as_str().to_owned();
    let profile_revision = record.config_revision.counter().get();
    let scope = ConnectionScope::new("org-1");
    connections
        .save(
            &scope,
            datazen_driver_api::ConnectionConfig {
                id: connection_id.clone(),
                name: "exec routes conn".into(),
                database_type: "mockdb".into(),
                host: Some("localhost".into()),
                port: Some(1),
                database: Some("app".into()),
                schema: None,
                username: None,
                password: None,
                ssl_mode: SslMode::Disable,
                connection_timeout: 30,
                max_pool_size: 10,
                ssh_tunnel: None,
                tunnel_kind: None,
                tunnel_id: None,
                http_proxy_tunnel: None,
                websocket_tunnel: None,
                color_tag: None,
                group: None,
                last_connected_at: None,
                server_version: None,
                options: None,
                read_only: false,
                pinned: false,
            },
        )
        .await
        .expect("connection config saved");

    // 执行后端 + 发布钩子 + 真租约源（sessions/execution/aiHost/host 同源）。
    let bridge = Arc::new(BudgetPortBridge::new(
        Arc::new(ServerBudgetCoordinator::new(
            8,
            10_000,
            OrganizationId::new("org-1"),
        )),
        OrganizationId::new("org-1"),
    ));
    let backend = Arc::new(ServerDriverExecBackend::new(
        registry.clone(),
        connections.clone(),
        profiles.clone(),
        scope,
        bridge,
        None,
    ));
    let sink = RuntimeResultSink::default();
    backend
        .set_publisher(execution_result_publisher(sink.clone()))
        .expect("publisher installed");
    let execution_policy = Arc::new(ServerExecutionPolicy::new(
        Arc::new(PolicyService::new(store.clone(), FULLSTACK_SECRET.to_vec()).expect("policy")),
        store.clone(),
        Arc::new(datazen::shared::clock::SystemClock),
    ));
    let lease_source = Arc::new(ServerRuntimeLeaseSource::new_with_shared_parts(
        profiles.clone(),
        execution_policy,
        backend,
        sink,
        Arc::new(datazen::execution::runtime::directory::InMemorySessionDirectory::new()),
    ));
    let directory_face: Arc<dyn datazen::execution::SessionDirectory> =
        lease_source.session_directory();
    let cases = Arc::new(LeaseConnectionUseCases::new(lease_source.clone()));
    let host = Arc::new(ServerResourceExecutionHost::new(
        connections.clone(),
        profiles.clone(),
        registry.clone(),
        Arc::new(ServerSettingsStore::scoped("org-1", store.clone())),
        lease_source.clone(),
        "org-1",
    ));

    // transfer 面（DomainState 的 HMAC 轨 + tasks；token 路由挂载必需）。
    let clock: Arc<dyn datazen::shared::clock::Clock> =
        Arc::new(datazen::shared::clock::SystemClock);
    let hmac_issuer = Arc::new(
        ServerSubmissionTokenIssuer::new(FULLSTACK_SECRET.to_vec(), clock.clone())
            .expect("hmac issuer"),
    );
    let audit = Arc::new(ServerAudit::new(store.clone()));
    let jobs = Arc::new(ServerJobs::new(
        store.clone(),
        Arc::new(datazen::jobs::domain::time::PortJobClock::new(
            datazen::shared::clock::SystemClock,
        )),
        900,
    ));
    let submission = Arc::new(datazen_server::transfer::SubmissionService::new(
        hmac_issuer.clone(),
        jobs.clone(),
        Arc::new(datazen_server::transfer::AllowAllGate),
        audit.clone(),
        clock.clone(),
    ));
    let artifacts = Arc::new(
        datazen_server::storage::ServerArtifacts::open(store.clone(), &dir).expect("artifacts"),
    );

    let app = build_router_with_auth(AuthDeps {
        store: store.clone(),
        policy: Arc::new(
            PolicyService::new(store.clone(), FULLSTACK_SECRET.to_vec()).expect("policy"),
        ),
        server_secret: FULLSTACK_SECRET.to_vec(),
        idle_ttl_seconds: 3600,
        rate_limit: None,
        sse: None,
        oidc: None,
        domains: Some(DomainState {
            issuer: hmac_issuer,
            submission,
            uploads: Arc::new(datazen_server::transfer::UploadService::new(
                artifacts.clone(),
                audit.clone(),
                clock.clone(),
            )),
            downloads: Arc::new(datazen_server::transfer::DownloadService::new(
                artifacts, audit, clock,
            )),
            tasks: Arc::new(datazen_server::operations::TaskCenter::new(jobs, 50)),
            sessions: cases.clone(),
        }),
        mcp: None,
        wapp_assets: None,
        domain_routers: Some(vec![
            sessions_router(SessionsState {
                sessions: cases.clone(),
                directory: directory_face.clone(),
            }),
            execution_router(ExecutionState {
                sessions: cases.clone(),
                directory: directory_face.clone(),
                // 落账器默认关闭（本套件钉路由语义；历史落账在 query_assets_routes
                // 套件端到端验证）。
                history: None,
            }),
            ai_host_router(AiHostState {
                settings: Arc::new(ServerSettingsStore::scoped("org-1", store.clone())),
                organization_id: "org-1".to_owned(),
                host,
            }),
        ]),
    })
    .expect("router");

    FullstackFixture {
        _dir: dir,
        _store: store,
        app,
        connection_id,
        profile_revision,
        cookie,
        csrf,
        cookie_user2,
        csrf_user2,
    }
}

/// oneshot 驱动认证链 Router（cm63 `call` 同款）。
async fn fullstack_call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    cookie: &str,
    csrf_token: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = HttpRequest::builder()
        .method(method)
        .uri(uri)
        .header(header::COOKIE, cookie)
        .header(datazen_server::csrf::CSRF_HEADER, csrf_token);
    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    let request = builder
        .body(Body::from(
            body.map(|value| value.to_string()).unwrap_or_default(),
        ))
        .expect("request builds");
    let response = app.clone().oneshot(request).await.expect("oneshot");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
        .await
        .expect("body bytes");
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json body")
    };
    (status, json)
}

/// identity 路由的 runtime 轨签发（执行域令牌的正路）。
async fn fullstack_issue_token(
    app: &axum::Router,
    cookie: &str,
    csrf: &str,
    operation: &str,
    handle: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut body = serde_json::json!({ "operation": operation });
    if let Some(handle) = handle {
        body["handle"] = handle;
    }
    fullstack_call(
        app,
        "POST",
        "/api/v1/submission-tokens",
        cookie,
        csrf,
        Some(body),
    )
    .await
}

/// 轮询一条执行到终态（测试侧上限 200 × 25ms；runtime 自负生命周期）。
async fn fullstack_poll_terminal(
    app: &axum::Router,
    cookie: &str,
    csrf: &str,
    execution_id: &str,
) -> serde_json::Value {
    for _ in 0..200 {
        let (status, view) = fullstack_call(
            app,
            "GET",
            &format!("/api/v1/executions/{execution_id}"),
            cookie,
            csrf,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "poll: {view}");
        let state = view["state"].as_str().unwrap_or_default().to_owned();
        if matches!(state.as_str(), "succeeded" | "failed" | "cancelled") {
            return view;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("execution did not reach a terminal state in time: {execution_id}");
}

// ── 全栈绿路径 ──────────────────────────────────────────────────────────────

/// `executeAtTarget` 全链：identity 路由 runtime 轨签发 →
/// POST /api/v1/executions → 202 `ExecutionReceipt`（稳定 id + 受理态）→
/// 轮询终态 `succeeded`。
#[tokio::test]
async fn execute_at_target_green_path_polls_to_terminal() {
    let fx = fullstack_fixture().await;

    let (status, token) =
        fullstack_issue_token(&fx.app, &fx.cookie, &fx.csrf, "executeAtTarget", None).await;
    assert_eq!(status, StatusCode::CREATED, "{token}");
    let key = token["idempotencyKey"].as_str().expect("key").to_owned();

    let (status, receipt) = fullstack_call(
        &fx.app,
        "POST",
        "/api/v1/executions",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key,
            "target": {
                "connectionId": fx.connection_id,
                "namespace": { "database": "app", "catalog": null, "schema": null, "path": [] },
            },
            "expectedConfigRevision": fx.profile_revision,
            "call": { "command": "query", "input": { "sql": "SELECT 1", "database": "app" } },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{receipt}");
    let execution_id = receipt["executionId"]
        .as_str()
        .expect("executionId")
        .to_owned();
    assert!(
        execution_id.starts_with("exe_"),
        "stable execution id mints from the session namespace: {execution_id}"
    );
    assert!(
        receipt["streamId"].as_str().is_some(),
        "streamId present: {receipt}"
    );
    // 受理态：非终态承诺（已接受 ≠ SQL 成功）。
    assert_ne!(receipt["state"], "succeeded");

    let view = fullstack_poll_terminal(&fx.app, &fx.cookie, &fx.csrf, &execution_id).await;
    assert_eq!(view["state"], "succeeded", "{view}");
}

/// `executeInSession` 全 HTTP 链：开会话（令牌不带 handle——首开）→
/// 带 handle 签发（runtime 轨的绑定事实）→ POST
/// /api/v1/sessions/{id}/executions → 202 → 终态。
#[tokio::test]
async fn execute_in_session_green_path_over_http_polls_to_terminal() {
    let fx = fullstack_fixture().await;

    let (status, token) =
        fullstack_issue_token(&fx.app, &fx.cookie, &fx.csrf, "openSession", None).await;
    assert_eq!(status, StatusCode::CREATED, "{token}");
    let open_key = token["idempotencyKey"].as_str().expect("key").to_owned();
    let (status, opened) = fullstack_call(
        &fx.app,
        "POST",
        "/api/v1/sessions",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": open_key },
            "idempotencyKey": open_key,
            "initialTarget": {
                "connectionId": fx.connection_id,
                "namespace": { "database": "app", "catalog": null, "schema": null, "path": [] },
            },
            "owner": {
                "kind": "editor",
                "clientInstanceId": "http",
                "editorSessionId": "exec-routes-editor",
            },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{opened}");
    let handle = opened["session"]["handle"].clone();
    let db_session_id = handle["dbSessionId"]
        .as_str()
        .expect("dbSessionId")
        .to_owned();

    // 会话内执行：令牌带 handle（runtime 轨的绑定事实）。
    let (status, token) = fullstack_issue_token(
        &fx.app,
        &fx.cookie,
        &fx.csrf,
        "executeInSession",
        Some(handle.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{token}");
    let key = token["idempotencyKey"].as_str().expect("key").to_owned();

    let (status, receipt) = fullstack_call(
        &fx.app,
        "POST",
        &format!("/api/v1/sessions/{db_session_id}/executions"),
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key,
            "handle": handle,
            "call": { "command": "query", "input": { "sql": "SELECT 1" } },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{receipt}");
    let execution_id = receipt["executionId"]
        .as_str()
        .expect("executionId")
        .to_owned();

    let view = fullstack_poll_terminal(&fx.app, &fx.cookie, &fx.csrf, &execution_id).await;
    assert_eq!(view["state"], "succeeded", "{view}");
}

// ── 全栈可见性钉子 ──────────────────────────────────────────────────────────

/// 未知会话（令牌在场、形状完整）→ 404 固定文案（`NOT_VISIBLE_MESSAGE`，
/// 与 sessions 域/L6 同一条——防枚举不可区分）。闸序按 main 契约 §6.10：
/// 令牌在场 → 可见性（目录闸）→ 一致性 → DTO 校验。
#[tokio::test]
async fn unknown_session_uses_the_fixed_not_visible_copy() {
    let fx = fullstack_fixture().await;

    let (status, wire) = fullstack_call(
        &fx.app,
        "POST",
        "/api/v1/sessions/dbs_unknown/executions",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": "key-unknown-session" },
            "idempotencyKey": "key-unknown-session",
            "handle": { "dbSessionId": "dbs_unknown", "runtimeEpoch": "0" },
            "call": { "command": "query", "input": { "sql": "SELECT 1" } },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{wire}");
    assert_eq!(wire["message"], NOT_VISIBLE_MESSAGE);

    // 越组织主体对同一未知会话：同一 404 固定文案（不可区分性）。
    let (status, wire) = fullstack_call(
        &fx.app,
        "POST",
        "/api/v1/sessions/dbs_unknown/executions",
        &fx.cookie_user2,
        &fx.csrf_user2,
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": "key-unknown-session" },
            "idempotencyKey": "key-unknown-session",
            "handle": { "dbSessionId": "dbs_unknown", "runtimeEpoch": "0" },
            "call": { "command": "query", "input": { "sql": "SELECT 1" } },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{wire}");
    assert_eq!(wire["message"], NOT_VISIBLE_MESSAGE);
}

/// 同组织第二主体读他人执行 → 统一 404「execution unavailable」
/// （L6 基线 Read 放行 OrgMember，用例层 `visible_to` 五元组拦下）；
/// 完全未知 id：同一 404 同文案（不泄露存在性）。
#[tokio::test]
async fn cross_principal_execution_read_is_uniform_404() {
    let fx = fullstack_fixture().await;

    let (status, token) =
        fullstack_issue_token(&fx.app, &fx.cookie, &fx.csrf, "executeAtTarget", None).await;
    assert_eq!(status, StatusCode::CREATED);
    let key = token["idempotencyKey"].as_str().expect("key").to_owned();
    let (status, receipt) = fullstack_call(
        &fx.app,
        "POST",
        "/api/v1/executions",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key,
            "target": {
                "connectionId": fx.connection_id,
                "namespace": { "database": "app", "catalog": null, "schema": null, "path": [] },
            },
            "expectedConfigRevision": fx.profile_revision,
            "call": { "command": "query", "input": { "sql": "SELECT 1", "database": "app" } },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{receipt}");
    let execution_id = receipt["executionId"]
        .as_str()
        .expect("executionId")
        .to_owned();

    // 同组织第二主体：404 与未知 id 同文案（user-2 的 L6 基线 Read 放行，
    // 用例层 visible_to 拦下）。
    let (status, wire) = fullstack_call(
        &fx.app,
        "GET",
        &format!("/api/v1/executions/{execution_id}"),
        &fx.cookie_user2,
        &fx.csrf_user2,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{wire}");
    assert_eq!(wire["message"], "execution unavailable");

    // 完全未知 id：同一 404 同文案（不泄露存在性）。
    let (status, wire) = fullstack_call(
        &fx.app,
        "GET",
        "/api/v1/executions/exe_nope_1",
        &fx.cookie,
        &fx.csrf,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{wire}");
    assert_eq!(wire["message"], "execution unavailable");
}

// ── 令牌纪律 ────────────────────────────────────────────────────────────────

/// 两轨不混用：HMAC 轨令牌（createProfile 形，`ServerSubmissionTokenIssuer`
/// 签名）喂给执行路由 → runtime 核验拒绝（`IdempotencyExpired`，409）。
#[tokio::test]
async fn hmac_track_token_is_rejected_by_the_runtime_track() {
    let fx = fullstack_fixture().await;

    // HMAC 轨令牌（createProfile 形——ServerSubmissionTokenIssuer 签名）
    // 不是 runtime 注册表的成员 → runtime 核验拒绝。
    let (status, hmac_token) = fullstack_call(
        &fx.app,
        "POST",
        "/api/v1/submission-tokens",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({ "operation": "createProfile" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{hmac_token}");
    let hmac_key = hmac_token["idempotencyKey"]
        .as_str()
        .expect("key")
        .to_owned();
    let (status, wire) = fullstack_call(
        &fx.app,
        "POST",
        "/api/v1/executions",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": hmac_key },
            "idempotencyKey": hmac_key,
            "target": {
                "connectionId": fx.connection_id,
                "namespace": { "database": "app", "catalog": null, "schema": null, "path": [] },
            },
            "expectedConfigRevision": fx.profile_revision,
            "call": { "command": "query", "input": { "sql": "SELECT 1", "database": "app" } },
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "runtime rejects foreign tokens (§9.2 的 409 行): {wire}"
    );
    assert_eq!(wire["code"], "idempotencyExpired");
}

/// `cancelExecution` 的令牌签发是会话绑定操作：缺 handle 在用例层显式拒
/// （400）；持会话签发后取消未知执行 → 404「execution unavailable」
/// （`visible_to` 先于取消语义）。
#[tokio::test]
async fn cancel_execution_token_requires_handle_and_unknown_cancel_is_uniform_404() {
    let fx = fullstack_fixture().await;

    let (status, token) =
        fullstack_issue_token(&fx.app, &fx.cookie, &fx.csrf, "cancelExecution", None).await;
    // cancelExecution 的 runtime 轨签发缺 handle → 显式拒（400）。
    assert_eq!(status, StatusCode::BAD_REQUEST, "{token}");

    // 持会话签发 cancel 令牌（前端 EditorSessionController 的形状：
    // token('cancelExecution') 带 this.session.handle）。
    let (status, open_token) =
        fullstack_issue_token(&fx.app, &fx.cookie, &fx.csrf, "openSession", None).await;
    assert_eq!(status, StatusCode::CREATED);
    let open_key = open_token["idempotencyKey"]
        .as_str()
        .expect("key")
        .to_owned();
    let (status, opened) = fullstack_call(
        &fx.app,
        "POST",
        "/api/v1/sessions",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": open_key },
            "idempotencyKey": open_key,
            "initialTarget": {
                "connectionId": fx.connection_id,
                "namespace": { "database": "app", "catalog": null, "schema": null, "path": [] },
            },
            "owner": { "kind": "editor", "clientInstanceId": "http", "editorSessionId": "e" },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{opened}");
    let handle = opened["session"]["handle"].clone();
    let (status, cancel_token) = fullstack_issue_token(
        &fx.app,
        &fx.cookie,
        &fx.csrf,
        "cancelExecution",
        Some(handle),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{cancel_token}");
    let key = cancel_token["idempotencyKey"]
        .as_str()
        .expect("key")
        .to_owned();

    // 未知执行：404 统一文案（visible_to 先于取消语义）。
    let (status, wire) = fullstack_call(
        &fx.app,
        "POST",
        "/api/v1/executions/exe_nope_1/cancel",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({ "submissionToken": { "idempotencyKey": key } })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{wire}");
    assert_eq!(wire["message"], "execution unavailable");
}

// ── aiHost.executeTarget（W8 刀1对账：claim 纪律 + 全栈闸序）─────────────────

/// `independent` 声明真执行绿路径（MockDriver 全栈，200 `CommandResult`）。
#[tokio::test]
async fn ai_host_execute_target_independent_green_path() {
    let fx = fullstack_fixture().await;

    let (status, result) = fullstack_call(
        &fx.app,
        "POST",
        "/api/v1/ai-host/execute-target",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "id": fx.connection_id,
            "command": "query",
            "input": { "sql": "SELECT 1", "database": "app" },
            "claim": { "kind": "independent" },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    // CommandResult 的 camelCase wire 形状：{ data }。
    assert!(
        result.get("data").is_some(),
        "CommandResult.data present: {result}"
    );
}

/// 闸序与 fail-closed 分臂的全栈半边（分臂形状由 f1 文件钉住，此处钉
/// 全栈闸序：未知连接 → 503「profile unavailable」）。
#[tokio::test]
async fn ai_host_execute_target_unknown_connection_is_503() {
    let fx = fullstack_fixture().await;

    let (status, wire) = fullstack_call(
        &fx.app,
        "POST",
        "/api/v1/ai-host/execute-target",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "id": "conn-unknown",
            "command": "query",
            "input": { "sql": "SELECT 1" },
            "claim": { "kind": "independent" },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{wire}");
    assert_eq!(wire["code"], "serviceUnavailable");
}
