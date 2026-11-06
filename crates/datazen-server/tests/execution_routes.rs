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
async fn unknown_execution_reads_as_not_found() {
    let (cases, directory, _store) = execution_service();
    let mut app = execution_app(ExecutionState {
        sessions: cases.clone(),
        directory,
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
