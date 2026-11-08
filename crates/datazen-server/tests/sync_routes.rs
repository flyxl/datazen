//! Data Sync 域路由集成测试（W8 刀3e-3；契约 §6.16 资产与读面）。
//!
//! 两档夹具（schema_diff_routes 先例）：
//!
//! - **router-only 腿**：`AuthenticatedSession` request extension 直插
//!   （绕过登录与 CSRF，只测域 handler 面）——任务/档案 CRUD、令牌闸、
//!   配对分类、迁移历史读面、job 读回的空集语义与跨组织 404。
//! - **wired 腿**：`build_router_with_auth` 全链（L2–L6）——未认证 401。

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

use datazen::history::MigrationRunRecord;
use datazen_server::csrf::{self, CsrfSecret};
use datazen_server::http::router::{build_router_with_auth, AuthDeps};
use datazen_server::policy::{PolicyService, Role};
use datazen_server::storage::login_sessions::{LoginSessionMode, LoginSessions, NewLoginSession};
use datazen_server::storage::migration_runs::ServerMigrationRuns;
use datazen_server::storage::sync_assets::{ServerSyncProfiles, ServerSyncTasks};
use datazen_server::storage::{AdminStore, ServerConnections, ServerJobs};
use datazen_server::sync::{SyncJobs, SyncRuntimeState};

const SERVER_SECRET: &[u8] = b"sync-routes-secret";

// ── 公共件 ──────────────────────────────────────────────────────────────────

fn memory_store() -> Arc<AdminStore> {
    let store = AdminStore::open_in_memory().expect("in-memory store");
    store.migrate().expect("migrations apply");
    Arc::new(store)
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn future_timestamp(hours: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::hours(hours))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// 装配一条 sync 域路由（router-only：extension 直插会话）。
async fn sync_app(store: Arc<AdminStore>, org: &str, principal: &str) -> axum::Router {
    let state = sync_state(&store);
    let mut app = datazen_server::http::domains::sync::sync_router(state);
    let session = session_of(org, principal);
    app = app.layer(axum::middleware::from_fn(
        move |mut req: Request<Body>, next: axum::middleware::Next| {
            let session = session.clone();
            async move {
                req.extensions_mut().insert(session);
                next.run(req).await
            }
        },
    ));
    app
}

fn session_of(
    org: &str,
    principal: &str,
) -> datazen_server::http::middleware::auth::AuthenticatedSession {
    datazen_server::http::middleware::auth::AuthenticatedSession {
        organization_id: org.into(),
        principal_id: principal.into(),
        session_id: "sess-1".into(),
        permission_version: 1,
        token_epoch: 0,
        auth_mode: LoginSessionMode::Cookie,
    }
}

fn sync_state(store: &Arc<AdminStore>) -> datazen_server::http::domains::sync::SyncState {
    let tasks = Arc::new(ServerSyncTasks::new(store.clone()));
    let profiles = Arc::new(ServerSyncProfiles::new(store.clone()));
    let migration_runs = Arc::new(ServerMigrationRuns::new(store.clone()));
    let plans: Arc<dyn datazen::migration::data_sync::plan_store::SyncPlanRepository> =
        Arc::new(datazen::migration::data_sync::plan_store::InMemorySyncPlanRepository::new());
    let runtime = SyncRuntimeState::new();
    let job_host = Arc::new(datazen::jobs::JobHost::new(
        Arc::new(ServerJobs::new(
            store.clone(),
            Arc::new(datazen::jobs::domain::time::PortJobClock::new(
                datazen::shared::clock::SystemClock,
            )),
            900,
        )),
        Arc::new(std::sync::Mutex::new(
            datazen::execution::runtime::budget::BudgetLedger::new(
                datazen::execution::runtime::budget::BudgetConfig::team_default(),
            ),
        )),
        Arc::new(datazen::jobs::domain::time::PortJobClock::new(
            datazen::shared::clock::SystemClock,
        )),
    ));
    let jobs = SyncJobs::new(job_host.clone(), plans.clone(), runtime.clone());
    // 执行面（router-only 腿：空驱动注册表——会话解析 fail-closed，钉的是
    // HTTP 绑定面[解析/错误映射/认证链]，绿路径在 tests/sync_execution.rs
    // 的 wired 夹具直驱服务层）。
    let execution = {
        use datazen_server::execution::budget_bridge::BudgetPortBridge;
        use datazen_server::execution::driver_exec::ServerDriverExecBackend;
        use datazen_server::execution::lease::ServerRuntimeLeaseSource;
        use datazen_server::execution::policy_adapter::ServerExecutionPolicy;
        use datazen_server::execution::secret_provider::ServerSecretVault;
        use datazen_server::schema_diff::endpoints::SessionEndpoints;

        let vault = Arc::new(
            ServerSecretVault::new(store.clone(), SERVER_SECRET.to_vec()).expect("secret vault"),
        );
        let connections = Arc::new(ServerConnections::new(store.clone(), vault));
        let repo_profiles = Arc::new(datazen_server::storage::ServerProfiles::new(store.clone()));
        let scope = datazen::connection::domain::ConnectionScope::new("org-1");
        let registry =
            Arc::new(datazen::execution::runtime::driver_registry::DriverRegistry::new());
        let bridge = Arc::new(BudgetPortBridge::new(
            Arc::new(
                datazen_server::execution::budget::ServerBudgetCoordinator::new(
                    8,
                    10_000,
                    datazen::shared::id::OrganizationId::new("org-1"),
                ),
            ),
            datazen::shared::id::OrganizationId::new("org-1"),
        ));
        let backend = Arc::new(ServerDriverExecBackend::new(
            registry,
            connections.clone(),
            repo_profiles.clone(),
            scope,
            bridge,
            None,
        ));
        let policy = Arc::new(ServerExecutionPolicy::new(
            Arc::new(
                datazen_server::policy::PolicyService::new(store.clone(), SERVER_SECRET.to_vec())
                    .expect("policy engine"),
            ),
            store.clone(),
            Arc::new(datazen::shared::clock::SystemClock),
        ));
        let directory =
            Arc::new(datazen::execution::runtime::directory::InMemorySessionDirectory::new());
        let lease_source = Arc::new(ServerRuntimeLeaseSource::new_with_shared_parts(
            repo_profiles,
            policy,
            backend,
            datazen::execution::application::orchestration::RuntimeResultSink::default(),
            directory.clone(),
        ));
        let endpoints = SessionEndpoints {
            directory,
            lease: lease_source,
            connections: connections.clone(),
        };
        let comparison_root: Arc<
            dyn datazen::migration::data_sync::comparison_store::ComparisonStorageRoot,
        > = Arc::new(
            datazen::migration::data_sync::comparison_store::LocalComparisonStorageRoot::new(
                std::env::temp_dir().join(format!("dz-sync-routes-{}", uuid::Uuid::new_v4())),
                datazen::shared::id::OrganizationId::new("org-1"),
            ),
        );
        datazen_server::sync::SyncExecution::new(
            job_host,
            plans,
            runtime,
            endpoints,
            connections,
            Arc::new(
                datazen::migration::data_transfer::transfer::adapter_registry::SyncAdapterRegistry::new(),
            ),
            comparison_root,
            migration_runs.clone(),
            profiles.clone(),
            tasks.clone(),
        )
    };
    datazen_server::http::domains::sync::SyncState {
        store: store.clone(),
        tasks,
        profiles,
        migration_runs,
        jobs,
        execution,
    }
}

async fn call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    cookie: &str,
    csrf_token: Option<&str>,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::COOKIE, cookie);
    if let Some(token) = csrf_token {
        builder = builder.header(csrf::CSRF_HEADER, token);
    }
    let request = if let Some(body) = body {
        builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .expect("request with body")
    } else {
        builder.body(Body::empty()).expect("request")
    };
    let response = app.clone().oneshot(request).await.expect("oneshot");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body bytes");
    let body = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::String(
            String::from_utf8_lossy(&bytes).into_owned(),
        ))
    };
    (status, body)
}

fn token_body(extra: serde_json::Value) -> serde_json::Value {
    let mut body = extra;
    body["submissionToken"] = serde_json::json!({ "idempotencyKey": "idem-1" });
    body["idempotencyKey"] = serde_json::json!("idem-1");
    body
}

/// 合法档案体（`deny_unknown_fields`：字段全集，camelCase）。
fn profile_body(id: &str) -> serde_json::Value {
    serde_json::json!({
        "version": 1,
        "id": id,
        "name": "Nightly users",
        "sourceConnectionId": "c-src",
        "targetConnectionId": "c-tgt",
        "sourceDatabase": "app_dev",
        "targetDatabase": "app_prod",
        "sourceSchema": "public",
        "targetSchema": "public",
        "tables": [
            {
                "sourceTable": "users",
                "targetTable": "users",
                "enabled": true
            }
        ],
        "options": {
            "insert": true,
            "update": true,
            "delete": false,
            "matchingStrategy": "primaryKey",
            "batchSize": 1000,
            "largeValueMode": "full"
        },
        "createdAt": "2026-01-01T00:00:00.000Z",
        "updatedAt": "2026-01-01T00:00:00.000Z"
    })
}

/// 经公共仓储 API 落连接目录行（connection_exists 闸的在场事实）。
async fn seed_connections(store: &Arc<AdminStore>, org: &str, ids: &[&str]) {
    use datazen::connection::domain::catalog::ConnectionRepository as _;
    use datazen_driver_api::SslMode;
    let vault = Arc::new(
        datazen_server::execution::secret_provider::ServerSecretVault::new(
            store.clone(),
            SERVER_SECRET.to_vec(),
        )
        .expect("secret vault"),
    );
    let connections = Arc::new(ServerConnections::new(store.clone(), vault));
    let scope = datazen::connection::domain::ConnectionScope::new(org);
    for id in ids {
        connections
            .save(
                &scope,
                datazen_driver_api::ConnectionConfig {
                    id: id.to_string(),
                    name: id.to_string(),
                    database_type: "mockdb".into(),
                    host: Some("localhost".into()),
                    port: Some(1),
                    database: Some("app".into()),
                    schema: Some("public".into()),
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
    }
}

fn migration_run(id: &str, operation: &str, status: &str) -> MigrationRunRecord {
    MigrationRunRecord {
        id: id.to_owned(),
        operation: operation.to_owned(),
        status: status.to_owned(),
        outcome: "none".to_owned(),
        phase: "finished".to_owned(),
        profile_id: None,
        profile_revision: None,
        source_connection_id: Some("c-src".to_owned()),
        target_connection_id: Some("c-tgt".to_owned()),
        started_at: now_rfc3339(),
        finished_at: Some(now_rfc3339()),
        selected_count: 10,
        committed_count: 8,
        failed_count: 0,
        conflict_count: 0,
        cancelled: false,
        rollback_outcome: "none".to_owned(),
        error_summary: None,
    }
}

// ── t1：档案 CRUD 往返 + 令牌闸 + 连接在场性 ─────────────────────────────

#[tokio::test]
async fn profiles_crud_roundtrip_with_token_and_connection_gates() {
    let store = memory_store();
    let app = sync_app(store.clone(), "org-1", "user-1").await;

    // 空集。
    let (status, body) = call(&app, "GET", "/api/v1/sync/profiles", "c=1", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().map(Vec::len), Some(0));

    // 无令牌 → 400。
    let (status, _) = call(
        &app,
        "PUT",
        "/api/v1/sync/profiles",
        "c=1",
        None,
        Some(profile_body("p1")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 令牌不绑定 → 400。
    let mut unbound = profile_body("p1");
    unbound["submissionToken"] = serde_json::json!({ "idempotencyKey": "other" });
    unbound["idempotencyKey"] = serde_json::json!("idem-1");
    let (status, _) = call(
        &app,
        "PUT",
        "/api/v1/sync/profiles",
        "c=1",
        None,
        Some(unbound),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 连接不在场 → 域校验拒绝（桌面同文案）。
    let (status, body) = call(
        &app,
        "PUT",
        "/api/v1/sync/profiles",
        "c=1",
        None,
        Some(token_body(profile_body("p1"))),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("connection no longer exists"),
        "连接在场性文案：{body}"
    );

    // 版本不支持 → 共享层校验拒绝。
    let mut bad_version = profile_body("p1");
    bad_version["version"] = serde_json::json!(99);
    let (status, _) = call(
        &app,
        "PUT",
        "/api/v1/sync/profiles",
        "c=1",
        None,
        Some(token_body(bad_version)),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 落连接目录行后保存 → 204。
    seed_connections(&store, "org-1", &["c-src", "c-tgt"]).await;
    let (status, _) = call(
        &app,
        "PUT",
        "/api/v1/sync/profiles",
        "c=1",
        None,
        Some(token_body(profile_body("p1"))),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // list 回读：一条，updated_at 已被域层推进（不再是 epoch）。
    let (status, body) = call(&app, "GET", "/api/v1/sync/profiles", "c=1", None, None).await;
    assert_eq!(status, StatusCode::OK);
    let list = body.as_array().expect("profile list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["id"], "p1");
    assert_ne!(
        list[0]["updatedAt"].as_str().unwrap_or_default(),
        "2026-01-01T00:00:00.000Z",
        "updated_at 由域层推进"
    );

    // upsert 改名 → 204；list 跟随。
    let mut renamed = profile_body("p1");
    renamed["name"] = serde_json::json!("Nightly users v2");
    let (status, _) = call(
        &app,
        "PUT",
        "/api/v1/sync/profiles",
        "c=1",
        None,
        Some(token_body(renamed)),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, body) = call(&app, "GET", "/api/v1/sync/profiles", "c=1", None, None).await;
    assert_eq!(body[0]["name"], "Nightly users v2");

    // delete → 204；幂等（第二次同应答）；list 清零。
    let (status, _) = call(
        &app,
        "DELETE",
        "/api/v1/sync/profiles/p1",
        "c=1",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = call(
        &app,
        "DELETE",
        "/api/v1/sync/profiles/p1",
        "c=1",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "delete 幂等");
    let (_, body) = call(&app, "GET", "/api/v1/sync/profiles", "c=1", None, None).await;
    assert_eq!(body.as_array().map(Vec::len), Some(0));
}

// ── t2：任务面读 + 幂等删 ─────────────────────────────────────────────────

#[tokio::test]
async fn tasks_face_reads_and_idempotent_delete() {
    let store = memory_store();
    let app = sync_app(store.clone(), "org-1", "user-1").await;

    let (status, body) = call(&app, "GET", "/api/v1/sync/tasks", "c=1", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().map(Vec::len), Some(0), "任务面读为空");

    let (status, _) = call(
        &app,
        "DELETE",
        "/api/v1/sync/tasks/never-existed",
        "c=1",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "任务删除幂等");

    // 越组织任务删除同应答（防枚举口径）。
    let other = sync_app(store.clone(), "org-2", "user-2").await;
    let (status, _) = call(
        &other,
        "DELETE",
        "/api/v1/sync/tasks/never-existed",
        "c=1",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

// ── t3：配对分类（共享层纯函数投影）───────────────────────────────────────

#[tokio::test]
async fn pairing_classification_is_the_shared_projection() {
    let store = memory_store();
    let app = sync_app(store, "org-1", "user-1").await;

    // 同族 V1 → direct + family。
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/sync/pairings",
        "c=1",
        None,
        Some(serde_json::json!({
            "sourceDatabaseType": "postgresql",
            "targetDatabaseType": "postgresql"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["path"], "direct");
    assert_eq!(body["supported"], true);
    assert_eq!(body["family"], "postgresql");

    // 异构 → 不支持（Transfer 语义，非 Sync）。
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/sync/pairings",
        "c=1",
        None,
        Some(serde_json::json!({
            "sourceDatabaseType": "postgresql",
            "targetDatabaseType": "mongodb"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["supported"], false);

    // 未知字段 → 400（deny_unknown_fields）。
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/sync/pairings",
        "c=1",
        None,
        Some(serde_json::json!({
            "sourceDatabaseType": "postgresql",
            "targetDatabaseType": "postgresql",
            "extra": true
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ── t4：迁移运行历史读面（过滤 + 单读 + 跨组织 404）──────────────────────

#[tokio::test]
async fn migration_runs_list_get_and_cross_org_404() {
    let store = memory_store();
    let runs = ServerMigrationRuns::new(store.clone());
    runs.save("org-1", migration_run("run-1", "dataSync", "succeeded"))
        .await
        .expect("seed run-1");
    runs.save("org-1", migration_run("run-2", "schemaDiff", "failed"))
        .await
        .expect("seed run-2");
    runs.save("org-2", migration_run("run-3", "dataSync", "succeeded"))
        .await
        .expect("seed run-3 (other org)");

    let app = sync_app(store.clone(), "org-1", "user-1").await;

    // list：本组织两条，按 started_at 倒序。
    let (status, body) = call(&app, "GET", "/api/v1/migration-runs", "c=1", None, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total"], 2, "跨组织行不可见");
    assert_eq!(body["items"].as_array().map(Vec::len), Some(2));

    // operation 过滤。
    let (status, body) = call(
        &app,
        "GET",
        "/api/v1/migration-runs?operation=dataSync",
        "c=1",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["id"], "run-1");
    assert_eq!(body["items"][0]["operation"], "dataSync");

    // 分页缺省：limit=25、offset=0（桌面同款）。
    let (_, body) = call(&app, "GET", "/api/v1/migration-runs", "c=1", None, None).await;
    assert_eq!(body["limit"], 25);
    assert_eq!(body["offset"], 0);

    // 单读：命中。
    let (status, body) = call(
        &app,
        "GET",
        "/api/v1/migration-runs/run-1",
        "c=1",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["id"], "run-1");
    assert_eq!(body["committedCount"], 8);

    // 单读：未知 → 404。
    let (status, _) = call(
        &app,
        "GET",
        "/api/v1/migration-runs/never-existed",
        "c=1",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 单读：他组织行 → 404（统一 404，防枚举）。
    let (status, _) = call(
        &app,
        "GET",
        "/api/v1/migration-runs/run-3",
        "c=1",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "跨组织读 404");
}

// ── t5：sync job 读面（空而真实）+ 取消令牌闸 ─────────────────────────────

#[tokio::test]
async fn sync_job_reads_and_cancel_over_an_empty_state() {
    let store = memory_store();
    let app = sync_app(store.clone(), "org-1", "user-1").await;

    // list → 空集（真读面，非 501 占位）。
    let (status, body) = call(&app, "GET", "/api/v1/sync/jobs", "c=1", None, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body.as_array().map(Vec::len), Some(0));

    // get / details / preview → 404。
    let (status, _) = call(&app, "GET", "/api/v1/sync/jobs/never", "c=1", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &app,
        "GET",
        "/api/v1/sync/jobs/never/details",
        "c=1",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &app,
        "GET",
        "/api/v1/sync/jobs/never/preview",
        "c=1",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // cancel：无令牌 → 400。
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/sync/jobs/never/cancel",
        "c=1",
        None,
        Some(serde_json::json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // cancel：未知 id + 令牌 → 200 { cancelled: false }（幂等 no-op——
    // 服务端 id 受理时铸造，无预受理取消窗口）。
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/sync/jobs/never/cancel",
        "c=1",
        None,
        Some(token_body(serde_json::json!({}))),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["cancelled"], false);
}

// ── t6：wired 全链——未认证 401 ─────────────────────────────────────────────

#[tokio::test]
async fn unauthenticated_requests_are_rejected() {
    let store = memory_store();
    let app = build_router_with_auth(AuthDeps {
        store: store.clone(),
        policy: Arc::new(
            PolicyService::new(store.clone(), SERVER_SECRET.to_vec()).expect("policy"),
        ),
        server_secret: SERVER_SECRET.to_vec(),
        idle_ttl_seconds: 3600,
        rate_limit: None,
        sse: None,
        oidc: None,
        domains: None,
        mcp: None,
        domain_routers: Some(vec![datazen_server::http::domains::sync::sync_router(
            sync_state(&store),
        )]),
    })
    .expect("router");

    for uri in [
        "/api/v1/sync/tasks",
        "/api/v1/sync/profiles",
        "/api/v1/sync/jobs",
        "/api/v1/migration-runs",
    ] {
        let request = Request::builder()
            .method("GET")
            .uri(uri)
            .body(Body::empty())
            .expect("request");
        let response = app.clone().oneshot(request).await.expect("oneshot");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "无会话 {uri}");
    }
}

// ── t7：执行面九路由的 HTTP 绑定面（刀3e-4b）──────────────────────────────

/// 执行面的解析/错误映射/资源寻址（绿路径在 tests/sync_execution.rs 的
/// wired 夹具直驱服务层——这里的 router-only 腿钉 HTTP 薄绑定的契约形状）。
#[tokio::test]
async fn execution_faces_parse_and_map_errors() {
    let store = memory_store();
    let app = sync_app(store.clone(), "org-1", "user-1").await;

    // inspect：未知会话 → 400（防枚举统一文案，经 HTTP 薄绑定透传）。
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/sync/inspections",
        "c=1",
        None,
        Some(serde_json::json!({
            "sourceDbSessionId": "no-such-session",
            "targetDbSessionId": "also-none"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("does not exist or is not visible"),
        "{body}"
    );

    // inspect：坏体（缺必填）→ 400 invalidArgument（手动解析对齐 400 口径，
    // 不是 axum 的 422）。
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/sync/inspections",
        "c=1",
        None,
        Some(serde_json::json!({ "sourceDbSessionId": "s" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // revalidate：坏体 → 400。
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/sync/revalidations",
        "c=1",
        None,
        Some(serde_json::json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // compare：坏体 → 400。
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/sync/comparisons",
        "c=1",
        None,
        Some(serde_json::json!({ "tables": "not-a-list" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // prepare：坏体 → 400（受理前拒绝，无 Job 落库）。
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/sync/jobs/prepare",
        "c=1",
        None,
        Some(serde_json::json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // apply：坏体 → 400。
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/sync/jobs/apply",
        "c=1",
        None,
        Some(serde_json::json!({ "request": {} })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // execute：坏体 → 400。
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/sync/executions",
        "c=1",
        None,
        Some(serde_json::json!({ "request": { "planId": "" } })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // SQL 预览：未知计划 → 400（「不存在」与「过期」同文案，跨组织同应答）。
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/sync/sql-preview",
        "c=1",
        None,
        Some(serde_json::json!({
            "planId": "never-issued",
            "options": {
                "insert": true,
                "update": true,
                "delete": false,
                "matchingStrategy": "primaryKey",
                "batchSize": 1000,
                "largeValueMode": "full"
            }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    // 冲突检查：未知任务 → 404。
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/sync/tasks/no-such-task/conflicts",
        "c=1",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 恢复核验：未知 job → 404（kind 过滤先行）。
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/sync/jobs/never/recovery-verification",
        "c=1",
        None,
        Some(serde_json::json!({
            "sourceDbSessionId": "s",
            "targetDbSessionId": "t",
            "sourceDatabase": "app",
            "targetDatabase": "app"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// 执行面九路由在认证链之内（L2 未认证 → 401，先于任何域逻辑）。
#[tokio::test]
async fn execution_routes_require_authentication() {
    let store = memory_store();
    let app = build_router_with_auth(AuthDeps {
        store: store.clone(),
        policy: Arc::new(
            PolicyService::new(store.clone(), SERVER_SECRET.to_vec()).expect("policy"),
        ),
        server_secret: SERVER_SECRET.to_vec(),
        idle_ttl_seconds: 3600,
        rate_limit: None,
        sse: None,
        oidc: None,
        domains: None,
        mcp: None,
        domain_routers: Some(vec![datazen_server::http::domains::sync::sync_router(
            sync_state(&store),
        )]),
    })
    .expect("router");

    for uri in [
        "/api/v1/sync/inspections",
        "/api/v1/sync/revalidations",
        "/api/v1/sync/comparisons",
        "/api/v1/sync/jobs/prepare",
        "/api/v1/sync/jobs/apply",
        "/api/v1/sync/executions",
        "/api/v1/sync/sql-preview",
        "/api/v1/sync/tasks/t-1/conflicts",
        "/api/v1/sync/jobs/j-1/recovery-verification",
    ] {
        let request = Request::builder()
            .method("POST")
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .expect("request");
        let response = app.clone().oneshot(request).await.expect("oneshot");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "无会话 {uri}");
    }
}
