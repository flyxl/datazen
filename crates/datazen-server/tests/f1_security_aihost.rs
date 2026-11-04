//! WR-F1 security / ai-host 域路由的集成钉子（oneshot 驱动，进程内装配）。
//!
//! 覆盖：
//!
//! - security（WR-F1#2，`packages/backend-client/src/securityClient.ts` 的
//!   冻结契约）：存后取回一致、未存 → 404 防枚举、put → 204 且二次 put
//!   覆盖、组织隔离、登录要求、审计行落库（坐标在账、明文永不进审计）。
//! - aiHost（WR-F1#3 的服务端半边）：session-config 回答
//!   `ExecutionSettings` 形状（缺省文档 + 存储切片回读）、executeTarget
//!   501 `capabilityUnsupported` fail-closed、组织边界、登录要求。
//!
//! L2 在真实装配里注入 `AuthenticatedSession`；本文件以请求扩展直接注入
//! （同一扩展类型，先例照 `tests/sse_events.rs`；装配语义由主线集成腿复核）。
//!
//! 前置（主线接线的一部分）：`crates/datazen-server/src/http/mod.rs` 的
//! `pub(crate) mod domains;` 需放开为 `pub mod domains;`——域路由片段对
//! 集成测试可见（与并行的 `tests/domains_read_side.rs` 同一前提）。

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request as HttpRequest, StatusCode};
use axum::Router;
use tower::ServiceExt;

use datazen::settings::SettingsRepository;
use datazen_server::execution::secret_provider::ServerSecretVault;
use datazen_server::http::domains::ai_host::{ai_host_router, AiHostState};
use datazen_server::http::domains::security::{security_router, SecurityState};
use datazen_server::http::middleware::auth::AuthenticatedSession;
use datazen_server::http::middleware::authorize::NOT_VISIBLE_MESSAGE;
use datazen_server::storage::{
    default_shared_settings, AdminStore, ServerAudit, ServerSettingsStore,
};

/// 审计红线的探针明文：审计测试用它作为 put 的 value，断言任何审计行都
/// 不携带它（与请求体里的字面量同源，杜绝「探针值与真实值不一致」的假绿）。
const PLAINTEXT: &str = "sk-live-9";

// ── 装配 ────────────────────────────────────────────────────────────────────

fn session_of(org: &str, principal: &str) -> AuthenticatedSession {
    AuthenticatedSession {
        organization_id: org.into(),
        principal_id: principal.into(),
        session_id: "sess-1".into(),
        permission_version: 1,
        token_epoch: 0,
        auth_mode: datazen_server::storage::login_sessions::LoginSessionMode::Cookie,
    }
}

fn memory_store() -> Arc<AdminStore> {
    let store = AdminStore::open_in_memory().expect("in-memory store");
    store.migrate().expect("migrations apply");
    Arc::new(store)
}

/// 文件库（审计断言要另开连接读 `audit_events`——`AdminStore` 对测试只给
/// 端口面；进程内唯一文件名，测试末尾尽力清理）。
fn file_store(tag: &str) -> (Arc<AdminStore>, PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "f1-security-{tag}-{}-{}.db",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let store = AdminStore::open(&path).expect("file store opens");
    store.migrate().expect("migrations apply");
    (Arc::new(store), path)
}

fn security_app(store: Arc<AdminStore>) -> Router {
    let vault = ServerSecretVault::new(store.clone(), b"f1-test-server-secret".to_vec())
        .expect("vault derives its subkeys");
    security_router(SecurityState {
        vault: Arc::new(vault),
        audit: Arc::new(ServerAudit::new(store)),
    })
}

fn ai_host_app(store: Arc<AdminStore>, organization_id: &str) -> Router {
    ai_host_router(AiHostState {
        settings: Arc::new(ServerSettingsStore::scoped(organization_id, store)),
        organization_id: organization_id.to_string(),
    })
}

// ── oneshot 助手 ────────────────────────────────────────────────────────────

async fn post_json(
    app: &mut Router,
    uri: &str,
    body: &str,
    session: Option<&AuthenticatedSession>,
) -> axum::http::Response<Body> {
    let mut request = HttpRequest::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_owned()))
        .expect("request");
    if let Some(session) = session {
        request.extensions_mut().insert(session.clone());
    }
    app.oneshot(request).await.expect("oneshot dispatch")
}

async fn put_json(
    app: &mut Router,
    uri: &str,
    body: &str,
    session: Option<&AuthenticatedSession>,
) -> axum::http::Response<Body> {
    let mut request = HttpRequest::builder()
        .method("PUT")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_owned()))
        .expect("request");
    if let Some(session) = session {
        request.extensions_mut().insert(session.clone());
    }
    app.oneshot(request).await.expect("oneshot dispatch")
}

async fn json_of(response: axum::http::Response<Body>) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body bytes");
    serde_json::from_slice(&bytes).expect("json body")
}

// ── security：存取语义 ──────────────────────────────────────────────────────

#[tokio::test]
async fn put_then_resolve_roundtrips_the_stored_value() {
    let mut app = security_app(memory_store());
    let session = session_of("org-a", "user-1");

    let put = put_json(
        &mut app,
        "/api/v1/secrets/stripe/apiToken",
        r#"{"value":"sk-live-9"}"#,
        Some(&session),
    )
    .await;
    assert_eq!(put.status(), StatusCode::NO_CONTENT);
    let bytes = axum::body::to_bytes(put.into_body(), 1024)
        .await
        .expect("body");
    assert!(
        bytes.is_empty(),
        "204 carries no body (the value never echoes)"
    );

    let resolve = post_json(
        &mut app,
        "/api/v1/secrets/resolve",
        r#"{"domain":"stripe","key":"apiToken"}"#,
        Some(&session),
    )
    .await;
    assert_eq!(resolve.status(), StatusCode::OK);
    let body = json_of(resolve).await;
    assert_eq!(body["value"], "sk-live-9");
    // 响应只带 `value` 一个键（无回显面：securityClient 的红线）。
    assert_eq!(body.as_object().expect("object").len(), 1);
}

#[tokio::test]
async fn second_put_overwrites_and_resolve_answers_the_current_revision() {
    let mut app = security_app(memory_store());
    let session = session_of("org-a", "user-1");

    let put = put_json(
        &mut app,
        "/api/v1/secrets/github/deployKey",
        r#"{"value":"first-material"}"#,
        Some(&session),
    )
    .await;
    assert_eq!(put.status(), StatusCode::NO_CONTENT);
    let put = put_json(
        &mut app,
        "/api/v1/secrets/github/deployKey",
        r#"{"value":"second-material"}"#,
        Some(&session),
    )
    .await;
    assert_eq!(put.status(), StatusCode::NO_CONTENT);

    let resolve = post_json(
        &mut app,
        "/api/v1/secrets/resolve",
        r#"{"domain":"github","key":"deployKey"}"#,
        Some(&session),
    )
    .await;
    assert_eq!(resolve.status(), StatusCode::OK);
    assert_eq!(json_of(resolve).await["value"], "second-material");
}

#[tokio::test]
async fn resolve_of_unknown_key_is_404_anti_enumeration() {
    let mut app = security_app(memory_store());
    let session = session_of("org-a", "user-1");

    let resolve = post_json(
        &mut app,
        "/api/v1/secrets/resolve",
        r#"{"domain":"stripe","key":"ghost"}"#,
        Some(&session),
    )
    .await;
    assert_eq!(resolve.status(), StatusCode::NOT_FOUND);
    let body = json_of(resolve).await;
    assert_eq!(body["code"], "notFound");
    assert_eq!(body["message"], NOT_VISIBLE_MESSAGE);
    // 固定文案不携带坐标（防枚举：不存在与不可见不可区分）。
    assert!(!body["message"].as_str().expect("message").contains("ghost"));
}

#[tokio::test]
async fn security_reads_and_writes_require_a_session() {
    let mut app = security_app(memory_store());

    let resolve = post_json(
        &mut app,
        "/api/v1/secrets/resolve",
        r#"{"domain":"stripe","key":"apiToken"}"#,
        None,
    )
    .await;
    assert_eq!(resolve.status(), StatusCode::UNAUTHORIZED);

    let put = put_json(
        &mut app,
        "/api/v1/secrets/stripe/apiToken",
        r#"{"value":"sk-live-9"}"#,
        None,
    )
    .await;
    assert_eq!(put.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn secret_references_are_scoped_to_the_session_organization() {
    let mut app = security_app(memory_store());
    let owner = session_of("org-a", "user-1");
    let stranger = session_of("org-b", "user-2");

    let put = put_json(
        &mut app,
        "/api/v1/secrets/stripe/apiToken",
        r#"{"value":"org-a-only"}"#,
        Some(&owner),
    )
    .await;
    assert_eq!(put.status(), StatusCode::NO_CONTENT);

    // 越组织 resolve：引用按会话组织铸造 → 解析不到 → 404（CM-05 不可区分）。
    let resolve = post_json(
        &mut app,
        "/api/v1/secrets/resolve",
        r#"{"domain":"stripe","key":"apiToken"}"#,
        Some(&stranger),
    )
    .await;
    assert_eq!(resolve.status(), StatusCode::NOT_FOUND);

    // 同组织照常取回。
    let resolve = post_json(
        &mut app,
        "/api/v1/secrets/resolve",
        r#"{"domain":"stripe","key":"apiToken"}"#,
        Some(&owner),
    )
    .await;
    assert_eq!(resolve.status(), StatusCode::OK);
    assert_eq!(json_of(resolve).await["value"], "org-a-only");
}

// ── security：审计行落库（坐标在账，明文永不进审计）─────────────────────────

#[tokio::test]
async fn audit_rows_carry_reference_and_action_but_never_the_value() {
    let (store, path) = file_store("audit");
    let mut app = security_app(store);
    let session = session_of("org-a", "user-1");

    let put = put_json(
        &mut app,
        "/api/v1/secrets/stripe/apiToken",
        &format!(r#"{{"value":"{PLAINTEXT}"}}"#),
        Some(&session),
    )
    .await;
    assert_eq!(put.status(), StatusCode::NO_CONTENT);
    let resolve = post_json(
        &mut app,
        "/api/v1/secrets/resolve",
        r#"{"domain":"stripe","key":"apiToken"}"#,
        Some(&session),
    )
    .await;
    assert_eq!(resolve.status(), StatusCode::OK);
    drop(app);

    let conn = rusqlite::Connection::open(&path).expect("reopen the admin db");
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .expect("busy timeout");
    let mut stmt = conn
        .prepare(
            "SELECT action, outcome, principal_id, payload_json FROM audit_events
             WHERE organization_id = 'org-a' ORDER BY event_id",
        )
        .expect("prepare");
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .expect("query")
        .collect::<Result<Vec<_>, _>>()
        .expect("rows");
    drop(stmt);
    drop(conn);
    let _ = std::fs::remove_file(&path);

    assert_eq!(
        rows.len(),
        2,
        "one audit row per vault touch (read and write)"
    );
    assert_eq!(rows[0].0, "secret_store");
    assert_eq!(rows[0].1, "succeeded");
    assert_eq!(rows[0].2.as_deref(), Some("user-1"));
    assert_eq!(rows[1].0, "secret_resolve");
    assert_eq!(rows[1].1, "succeeded");
    for (_, _, _, payload) in &rows {
        // 坐标在账：规范资源路径形式携带 domain/key（put 与 resolve 同形）。
        assert!(
            payload.contains("/api/v1/secrets/stripe/apiToken"),
            "audit payload must carry the reference: {payload}"
        );
        // 红线：value 明文永不进审计。
        assert!(
            !payload.contains(PLAINTEXT),
            "plaintext must never enter the audit payload: {payload}"
        );
    }
}

// ── aiHost：session-config ──────────────────────────────────────────────────

#[tokio::test]
async fn session_config_answers_execution_settings_defaults() {
    let mut app = ai_host_app(memory_store(), "org-a");
    let session = session_of("org-a", "user-1");

    let response = post_json(
        &mut app,
        "/api/v1/ai-host/session-config",
        r#"{"sessionId":"sess-1"}"#,
        Some(&session),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_of(response).await;
    // `ExecutionSettings` 的 serde camelCase 形状（storage::settings 的缺省
    // 文档，fresh install 语义）。
    assert_eq!(body["limitSelectResults"], false);
    assert_eq!(body["queryResultLimit"], 5000);
    assert_eq!(body["safeMode"], true);
    assert_eq!(body["connectionPoolSize"], 10);
    assert_eq!(body["mcpDisabledTools"], serde_json::json!([]));
    assert_eq!(body["mcpPermissionMode"], "safe_write");
    assert_eq!(body["mcpAllowedConnectionIds"], serde_json::json!([]));
    assert_eq!(body["driverSettings"], serde_json::json!({}));
    assert_eq!(body["aiStrictEgress"], true);
}

#[tokio::test]
async fn session_config_reflects_the_stored_execution_section() {
    let store = memory_store();
    {
        let scoped = ServerSettingsStore::scoped("org-a", store.clone());
        let mut document = default_shared_settings();
        document.execution.safe_mode = false;
        document.execution.query_result_limit = 1234;
        scoped
            .write_settings(document)
            .await
            .expect("settings write");
    }
    let mut app = ai_host_app(store, "org-a");
    let session = session_of("org-a", "user-1");

    let response = post_json(
        &mut app,
        "/api/v1/ai-host/session-config",
        r#"{"sessionId":"sess-1"}"#,
        Some(&session),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_of(response).await;
    assert_eq!(body["safeMode"], false);
    assert_eq!(body["queryResultLimit"], 1234);
}

// ── aiHost：executeTarget fail-closed ───────────────────────────────────────

#[tokio::test]
async fn execute_target_is_fail_closed_501_capability_unsupported() {
    let mut app = ai_host_app(memory_store(), "org-a");
    let session = session_of("org-a", "user-1");

    // 体 = payload 原样（AiTargetClaim 进体，TS 半边 aiPorts.ts 的形状）。
    let response = post_json(
        &mut app,
        "/api/v1/ai-host/execute-target",
        r#"{"id":"conn-1","command":"listTables","input":{"database":"main"},"claim":{"kind":"independent"}}"#,
        Some(&session),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    let wire = json_of(response).await;
    assert_eq!(wire["code"], "capabilityUnsupported");
    let message = wire["message"].as_str().expect("message");
    assert!(
        message.contains("target execution machine"),
        "precise copy names the missing capability: {message}"
    );
    assert!(
        message.contains("No execution was started"),
        "must never read as acceptance: {message}"
    );
}

#[tokio::test]
async fn ai_host_state_is_bound_to_its_organization() {
    let mut app = ai_host_app(memory_store(), "org-a");
    let stranger = session_of("org-b", "user-2");

    let config = post_json(
        &mut app,
        "/api/v1/ai-host/session-config",
        r#"{"sessionId":"sess-1"}"#,
        Some(&stranger),
    )
    .await;
    assert_eq!(config.status(), StatusCode::NOT_FOUND);
    assert_eq!(json_of(config).await["code"], "notFound");

    // 组织边界核对先于 fail-closed：异组织会话连 501 都拿不到（不暴露装配形态）。
    let target = post_json(
        &mut app,
        "/api/v1/ai-host/execute-target",
        r#"{"id":"conn-1","command":"listTables","input":null,"claim":{"kind":"independent"}}"#,
        Some(&stranger),
    )
    .await;
    assert_eq!(
        target.status(),
        StatusCode::NOT_FOUND,
        "org guard precedes the 501 fail-closed answer"
    );
}

#[tokio::test]
async fn ai_host_requires_an_authenticated_session() {
    let mut app = ai_host_app(memory_store(), "org-a");

    let config = post_json(
        &mut app,
        "/api/v1/ai-host/session-config",
        r#"{"sessionId":"sess-1"}"#,
        None,
    )
    .await;
    assert_eq!(config.status(), StatusCode::UNAUTHORIZED);
}
