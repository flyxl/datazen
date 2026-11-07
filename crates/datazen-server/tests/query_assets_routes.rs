//! W8 刀3a：查询资产域路由集成钉子（契约 §6.12；收藏与查询历史从
//! legacy IPC 命令轨收敛进 MethodMap 的服务端半边，oneshot 驱动、进程内
//! 装配）。
//!
//! 覆盖：
//!
//! - **收藏 CRUD**：201 铸造 wire 形状（id 服务端铸造、camelCase）、列表
//!   连接过滤、删除 204（未知 id 幂等 no-op 同码）、体缺令牌 400。
//! - **隔离**：同组织第二主体与越组织主体的读取天然落空、删除无操作
//!   （CM-05 读越界空；INV-01 归属只来自认证链）。
//! - **历史读写**：空页形状、存储侧落账后的分页/过滤/排序、删除行数、
//!   clear/purge、跨主体删除 `deleted: 0`。
//! - **落账钩子**（`execution::history_recorder`）：SQL 命令终态落一行
//!   （假用例面即时终态）、非 SQL 命令不落、跨主体隔离落账。
//!
//! L2 以请求扩展直接注入 `AuthenticatedSession`（f1 先例）。

use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{header, Request as HttpRequest, StatusCode};
use axum::Router;
use tower::ServiceExt;

use datazen::connection::ProfileView;
use datazen::execution::application::requests::{
    AttachmentRequest, CloseSessionRequest, ExecuteAtTargetRequest, ExecuteInSessionRequest,
    OpenSessionRequest, SetSessionContextRequest,
};
use datazen::execution::application::use_cases::ConnectionUseCases;
use datazen::execution::{
    CancelReceipt, CloseReceipt, ContextChangeReceipt, EventSequence, EventSubscription,
    ExecutionReceipt, ExecutionState as ExecutionStateEnum, ExecutionView, IdempotentOperation,
    OpenSessionReceipt, SessionDirectory, SessionHandle, SessionView, SubmissionToken,
};
use datazen::history::{FavoriteDraft, QueryHistoryEntry};
use datazen::shared::context::RequestContext;
use datazen::shared::id::{ConnectionId, DbSessionId, ExecutionId, StreamId};

use datazen_server::execution::history_recorder::HistoryRecorder;
use datazen_server::http::domains::query_assets::{query_assets_router, QueryAssetsState};
use datazen_server::http::middleware::auth::AuthenticatedSession;
use datazen_server::storage::query_assets::{
    HistoryPageQuery, ServerQueryFavoritesStore, ServerQueryHistoryStore,
};
use datazen_server::storage::AdminStore;

// ── 装配 ────────────────────────────────────────────────────────────────────

fn memory_store() -> Arc<AdminStore> {
    let store = AdminStore::open_in_memory().expect("in-memory store");
    store.migrate().expect("migrations apply v1..=v10");
    Arc::new(store)
}

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

fn assets_app(store: Arc<AdminStore>) -> Router {
    query_assets_router(QueryAssetsState { store })
}

/// 写面请求的令牌词面（本域 handler 只核在场性与绑定；HMAC 轨签发面
/// 的签名核验属配置类操作，本套件不重复）。
fn token_body(mut extra: serde_json::Value) -> serde_json::Value {
    let mut body = serde_json::json!({
        "submissionToken": { "idempotencyKey": "idem-1" },
        "idempotencyKey": "idem-1",
    });
    if let (Some(target), Some(source)) = (body.as_object_mut(), extra.as_object_mut()) {
        for (key, value) in source.iter() {
            target.insert(key.clone(), value.clone());
        }
    }
    body
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    session: Option<&AuthenticatedSession>,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = HttpRequest::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    let mut request = builder
        .body(Body::from(
            body.map(|value| value.to_string()).unwrap_or_default(),
        ))
        .expect("request");
    if let Some(session) = session {
        request.extensions_mut().insert(session.clone());
    }
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

// ── 收藏 ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn t1_favorite_create_round_trips_the_wire_shape() {
    let app = assets_app(memory_store());
    let session = session_of("org-a", "user-1");

    let (status, wire) = call(
        &app,
        "POST",
        "/api/v1/query-favorites",
        Some(&session),
        Some(token_body(serde_json::json!({
            "connectionId": "conn-1",
            "title": "Users by email",
            "sql": "SELECT 1",
            "database": "app",
            "keyword": "users",
        }))),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{wire}");
    // id 服务端铸造；camelCase wire 词汇（共享 FavoriteQuery 形状）。
    assert!(!wire["id"].as_str().expect("id").is_empty());
    assert_eq!(wire["connectionId"], "conn-1");
    assert_eq!(wire["title"], "Users by email");
    assert_eq!(wire["database"], "app");
    assert_eq!(wire["keyword"], "users");
    assert!(wire["createdAt"].as_str().is_some(), "createdAt minted");
}

#[tokio::test]
async fn t2_favorite_list_filters_by_connection() {
    let store = memory_store();
    let app = assets_app(store.clone());
    let session = session_of("org-a", "user-1");
    // 同一边界直写两连接各一条（创建语义由 t1 钉），路由面验证读形状。
    let scoped = ServerQueryFavoritesStore::for_identity(store, "org-a", "user-1");
    for (connection, title) in [("conn-1", "first"), ("conn-2", "second")] {
        scoped
            .add(FavoriteDraft {
                connection_id: connection.to_owned(),
                title: title.to_owned(),
                sql: "SELECT 1".into(),
                database: None,
                keyword: None,
                folder: None,
            })
            .await
            .expect("favorite seeded");
    }

    let (status, all) = call(&app, "GET", "/api/v1/query-favorites", Some(&session), None).await;
    assert_eq!(status, StatusCode::OK, "{all}");
    assert_eq!(all.as_array().expect("array").len(), 2);

    let (status, filtered) = call(
        &app,
        "GET",
        "/api/v1/query-favorites?connectionId=conn-1",
        Some(&session),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let list = filtered.as_array().expect("array");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["connectionId"], "conn-1");
}

#[tokio::test]
async fn t3_favorite_delete_is_idempotent_204() {
    let app = assets_app(memory_store());
    let session = session_of("org-a", "user-1");
    let id = seed_favorite_via_router(&app, &session).await;

    let (status, body) = call(
        &app,
        "DELETE",
        &format!("/api/v1/query-favorites/{id}"),
        Some(&session),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");

    // 未知 id：同一 204（幂等 no-op，不区分未知与越界——防枚举）。
    let (status, body) = call(
        &app,
        "DELETE",
        "/api/v1/query-favorites/nope-1",
        Some(&session),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
}

#[tokio::test]
async fn t4_favorite_isolation_is_per_principal_and_org() {
    let app = assets_app(memory_store());
    let owner = session_of("org-a", "user-1");
    let id = seed_favorite_via_router(&app, &owner).await;

    // 同组织第二主体：读取落空 + 删除无操作。
    let peer = session_of("org-a", "user-2");
    let (status, list) = call(&app, "GET", "/api/v1/query-favorites", Some(&peer), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        list.as_array().expect("array").is_empty(),
        "peer sees nothing"
    );
    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/api/v1/query-favorites/{id}"),
        Some(&peer),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    // 主人的收藏仍在（peer 的删除没碰到它）。
    let (status, list) = call(&app, "GET", "/api/v1/query-favorites", Some(&owner), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().expect("array").len(), 1);

    // 越组织主体：同样落空。
    let stranger = session_of("org-b", "user-3");
    let (status, list) = call(
        &app,
        "GET",
        "/api/v1/query-favorites",
        Some(&stranger),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(list.as_array().expect("array").is_empty());
}

#[tokio::test]
async fn t5_favorite_create_requires_a_submission_token() {
    let app = assets_app(memory_store());
    let session = session_of("org-a", "user-1");
    let (status, wire) = call(
        &app,
        "POST",
        "/api/v1/query-favorites",
        Some(&session),
        Some(serde_json::json!({ "connectionId": "conn-1", "title": "t", "sql": "SELECT 1" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{wire}");
    assert_eq!(wire["code"], "invalidArgument");
}

// ── 查询历史 ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn t6_history_page_reads_recorded_rows() {
    let store = memory_store();
    let app = assets_app(store.clone());
    let session = session_of("org-a", "user-1");

    // 空页形状。
    let (status, page) = call(&app, "GET", "/api/v1/query-history", Some(&session), None).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["entries"].as_array().expect("entries").len(), 0);
    assert_eq!(page["total"], 0);

    // 存储侧落账两行（不同连接）后分页读取。
    seed_history(&store, "org-a", "user-1", "conn-1", "SELECT 1", true).await;
    seed_history(
        &store,
        "org-a",
        "user-1",
        "conn-2",
        "UPDATE t SET x = 1",
        true,
    )
    .await;

    let (status, page) = call(&app, "GET", "/api/v1/query-history", Some(&session), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["total"], 2);
    assert_eq!(page["entries"].as_array().expect("entries").len(), 2);

    // 连接过滤 + 搜索（search 在 limit 前施加）。
    let (status, page) = call(
        &app,
        "GET",
        "/api/v1/query-history?connectionId=conn-1",
        Some(&session),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["total"], 1);

    let (status, page) = call(
        &app,
        "GET",
        "/api/v1/query-history?search=UPDATE",
        Some(&session),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["total"], 1);

    // 未知 order → 400。
    let (status, wire) = call(
        &app,
        "GET",
        "/api/v1/query-history?order=sideways",
        Some(&session),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{wire}");
}

#[tokio::test]
async fn t7_history_manage_surface_and_isolation() {
    let store = memory_store();
    let app = assets_app(store.clone());
    let owner = session_of("org-a", "user-1");
    seed_history(&store, "org-a", "user-1", "conn-1", "SELECT 1", true).await;
    let entries: Vec<QueryHistoryEntry> =
        ServerQueryHistoryStore::for_identity(store.clone(), "org-a", "user-1")
            .page(HistoryPageQuery {
                limit: 10,
                ..HistoryPageQuery::default()
            })
            .await
            .expect("page")
            .entries;
    let id = entries[0].id.clone();

    // 同组织第二主体删除：deleted 0（越界无操作），主人的行还在。
    let peer = session_of("org-a", "user-2");
    let (status, wire) = call(
        &app,
        "DELETE",
        &format!("/api/v1/query-history/{id}"),
        Some(&peer),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{wire}");
    assert_eq!(wire["deleted"], 0);

    // 主人删除：deleted 1。
    let (status, wire) = call(
        &app,
        "DELETE",
        &format!("/api/v1/query-history/{id}"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{wire}");
    assert_eq!(wire["deleted"], 1);

    // clear 与 purge 的 wire 形状。
    seed_history(&store, "org-a", "user-1", "conn-1", "SELECT 2", true).await;
    let (status, wire) = call(&app, "DELETE", "/api/v1/query-history", Some(&owner), None).await;
    assert_eq!(status, StatusCode::OK, "{wire}");
    assert_eq!(wire["purged"], 1);

    seed_history(&store, "org-a", "user-1", "conn-1", "SELECT 3", true).await;
    let (status, wire) = call(
        &app,
        "POST",
        "/api/v1/query-history/purge",
        Some(&owner),
        Some(serde_json::json!({ "scope": "query", "retainDays": 30 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{wire}");
    assert_eq!(wire["purged"], 0, "保留窗口内的行不动");

    // 未知 scope → 400。
    let (status, wire) = call(
        &app,
        "POST",
        "/api/v1/query-history/purge",
        Some(&owner),
        Some(serde_json::json!({ "scope": "galaxy" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{wire}");
}

// ── 落账钩子 ────────────────────────────────────────────────────────────────

/// `ConnectionUseCases` 的即时终态假体：`get_execution` 恒答 Succeeded
/// （轮询任务首跳即终态），其余方法按端口形状显式不可用（本套件只消费
/// 读面）。
struct TerminalNowUseCases;

#[async_trait]
impl ConnectionUseCases for TerminalNowUseCases {
    async fn list_connections(
        &self,
        _ctx: &RequestContext,
    ) -> Result<Vec<ProfileView>, datazen::shared::error::ApiError> {
        Err(datazen::shared::error::ApiError::invalid_argument("unused"))
    }
    async fn issue_submission_token(
        &self,
        _ctx: &RequestContext,
        _operation: IdempotentOperation,
        _handle: Option<&SessionHandle>,
    ) -> Result<SubmissionToken, datazen::shared::error::ApiError> {
        Err(datazen::shared::error::ApiError::invalid_argument("unused"))
    }
    async fn open_session(
        &self,
        _ctx: &RequestContext,
        _request: OpenSessionRequest,
    ) -> Result<OpenSessionReceipt, datazen::shared::error::ApiError> {
        Err(datazen::shared::error::ApiError::invalid_argument("unused"))
    }
    async fn attach_session(
        &self,
        _ctx: &RequestContext,
        _request: AttachmentRequest,
    ) -> Result<SessionView, datazen::shared::error::ApiError> {
        Err(datazen::shared::error::ApiError::invalid_argument("unused"))
    }
    async fn detach_session(
        &self,
        _ctx: &RequestContext,
        _request: AttachmentRequest,
    ) -> Result<SessionView, datazen::shared::error::ApiError> {
        Err(datazen::shared::error::ApiError::invalid_argument("unused"))
    }
    async fn get_session(
        &self,
        _ctx: &RequestContext,
        _handle: SessionHandle,
    ) -> Result<SessionView, datazen::shared::error::ApiError> {
        Err(datazen::shared::error::ApiError::invalid_argument("unused"))
    }
    async fn execute_in_session(
        &self,
        _ctx: &RequestContext,
        _request: ExecuteInSessionRequest,
    ) -> Result<ExecutionReceipt, datazen::shared::error::ApiError> {
        Err(datazen::shared::error::ApiError::invalid_argument("unused"))
    }
    async fn execute_at_target(
        &self,
        _ctx: &RequestContext,
        _request: ExecuteAtTargetRequest,
    ) -> Result<ExecutionReceipt, datazen::shared::error::ApiError> {
        Err(datazen::shared::error::ApiError::invalid_argument("unused"))
    }
    async fn set_session_context(
        &self,
        _ctx: &RequestContext,
        _request: SetSessionContextRequest,
    ) -> Result<ContextChangeReceipt, datazen::shared::error::ApiError> {
        Err(datazen::shared::error::ApiError::invalid_argument("unused"))
    }
    async fn close_session(
        &self,
        _ctx: &RequestContext,
        _request: CloseSessionRequest,
    ) -> Result<CloseReceipt, datazen::shared::error::ApiError> {
        Err(datazen::shared::error::ApiError::invalid_argument("unused"))
    }
    async fn get_execution(
        &self,
        _ctx: &RequestContext,
        _execution_id: ExecutionId,
    ) -> Result<ExecutionView, datazen::shared::error::ApiError> {
        Ok(ExecutionView {
            execution_id: ExecutionId::new("exe_fake"),
            state: ExecutionStateEnum::Succeeded,
            effect_outcome: datazen::execution::EffectOutcome::Completed,
            provenance: None,
            artifact_ids: Vec::new(),
            result_completeness: datazen::execution::ResultCompleteness::Complete,
            truncation_reason: None,
            error_code: None,
            runtime_binding: None,
        })
    }
    async fn cancel_execution(
        &self,
        _ctx: &RequestContext,
        _execution_id: ExecutionId,
    ) -> Result<CancelReceipt, datazen::shared::error::ApiError> {
        Err(datazen::shared::error::ApiError::invalid_argument("unused"))
    }
    async fn subscribe_events(
        &self,
        _ctx: &RequestContext,
        _stream_id: StreamId,
        _after_sequence: Option<EventSequence>,
    ) -> Result<EventSubscription, datazen::shared::error::ApiError> {
        Err(datazen::shared::error::ApiError::invalid_argument("unused"))
    }
}

#[tokio::test]
async fn t8_history_recorder_lands_a_row_for_sql_commands() {
    let store = memory_store();
    let directory: Arc<dyn SessionDirectory> =
        Arc::new(datazen::execution::runtime::directory::InMemorySessionDirectory::new());
    let recorder = HistoryRecorder::new(store.clone());
    let ctx = RequestContext::new(
        datazen::shared::id::OrganizationId::new("org-a"),
        datazen::shared::id::PrincipalId::new("user-1"),
        None,
        datazen::shared::id::ClientInstanceId::new("http"),
        datazen::shared::id::RequestId::new("req-1"),
        None,
    );

    // SQL 命令：受理即 spawn，终态（即时）后落账一行。
    recorder.record_on_acceptance(
        Arc::new(TerminalNowUseCases),
        directory.clone(),
        ctx.clone(),
        "query".into(),
        Some("SELECT 1".into()),
        Some("conn-1".into()),
        None,
        ExecutionId::new("exe_fake"),
    );
    // 非 SQL 命令：不 spawn、不落账。
    recorder.record_on_acceptance(
        Arc::new(TerminalNowUseCases),
        directory.clone(),
        ctx.clone(),
        "list_databases".into(),
        None,
        Some("conn-1".into()),
        None,
        ExecutionId::new("exe_fake"),
    );

    // 轮询等待 spawn 任务完成（即时终态，几百毫秒内必然落账）。
    let scoped = ServerQueryHistoryStore::for_identity(store.clone(), "org-a", "user-1");
    let mut landed = Vec::new();
    for _ in 0..40 {
        landed = scoped
            .page(HistoryPageQuery {
                limit: 10,
                ..HistoryPageQuery::default()
            })
            .await
            .expect("page")
            .entries;
        if !landed.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(landed.len(), 1, "恰好一条：SQL 命令落账、非 SQL 不落");
    assert_eq!(landed[0].connection_id, "conn-1");
    assert_eq!(landed[0].sql, "SELECT 1");
    assert!(landed[0].success);

    // 落账行按提交者隔离：user-2 的边界读不到它。
    let peer = ServerQueryHistoryStore::for_identity(store, "org-a", "user-2");
    let peer_rows = peer.page(HistoryPageQuery::default()).await.expect("page");
    assert!(
        peer_rows.entries.is_empty(),
        "落账行按 (org, principal) 隔离"
    );
}

// ── 夹具助手 ────────────────────────────────────────────────────────────────

/// 经路由创建一条收藏（走真实 handler 语义）并返回铸造 id。
async fn seed_favorite_via_router(app: &Router, session: &AuthenticatedSession) -> String {
    let (status, wire) = call(
        app,
        "POST",
        "/api/v1/query-favorites",
        Some(session),
        Some(token_body(serde_json::json!({
            "connectionId": "conn-1",
            "title": "seeded",
            "sql": "SELECT 1",
        }))),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{wire}");
    wire["id"].as_str().expect("id").to_owned()
}

/// 直接经存储边界种一条历史行（分页/管理面的种子；落账语义由 t8 钉）。
async fn seed_history(
    store: &Arc<AdminStore>,
    org: &str,
    principal: &str,
    connection_id: &str,
    sql: &str,
    success: bool,
) {
    let scoped = ServerQueryHistoryStore::for_identity(store.clone(), org, principal);
    scoped
        .add(QueryHistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            connection_id: connection_id.to_owned(),
            database: String::new(),
            schema: None,
            sql: sql.to_owned(),
            executed_at: chrono::Utc::now(),
            execution_time_ms: 12,
            rows_affected: None,
            success,
            error_message: None,
        })
        .await
        .expect("history seeded");
}

/// 显式未消费的导入占位（防漂移；删除需随实现同步）。
const _: fn(&DbSessionId) -> &str = |id| id.as_str();
const _: fn(&ConnectionId) -> &str = |id| id.as_str();
