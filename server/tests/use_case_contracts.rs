//! WR-C3 HTTP 腿：named_views / settings / connections / sessions 用例契约
//! （同用例、axum 绑定）。
//!
//! 测试内**自建最小 axum Router**——不改 `server/src/**`：只绑定 WR-C2
//! 冻结契约（http-api-contract §6.9 的 named-views 三端点、§6.1 的
//! settings 两端点）的端点形态，handler 调**同一个** modules 用例
//! （`datazen::named_views::NamedViewService` / `datazen::settings::
//! SettingsService`，装配层注入规格共享的夹具仓储），用
//! `tower::ServiceExt::oneshot` 发请求跑共享规格
//! （`datazen::use_case_contracts_test_support::{named_views, settings}`）。
//!
//! settings 域的错误 wire 事实：该域没有 application 契约码——wire 体的
//! `code` 字段**恒为 null**，「无码」本身是与 IPC 腿一致的契约事实。
//!
//! 七个 scenario 测试与 IPC 腿（桌面
//! `commands/named_views_use_case_contract_tests.rs`）与 modules 无宿主
//! 基线**逐字同名**——「两腿同绿」核对到每条用例；规格本体不感知任何
//! HTTP 词汇，形态差异只允许出现在本文件的装配层断言（文件尾的 pin）。
//!
//! # 本腿装配层自持的形态事实（规格零感知）
//!
//! - 端点形态 = WR-C2 §6.9：`GET /api/v1/named-views?connectionId=&database=
//!   &schema=&table=`、`PUT /api/v1/named-views`（`{ view }`，空 id 铸造 →
//!   201，更新 → 200）、`DELETE /api/v1/named-views/{id}` → 剩余集合；
//! - 错误 wire 体是 **JSON 对象** `{code, message}`（与 IPC 腿的纯字符串
//!   信封相对照）；application 契约码进 `code` 字段，application 文本进
//!   `message` 字段；状态码是传输事实（409/400/500 的映射属装配裁决，
//!   规格 `ContractFailure` 不携带状态码）；
//! - scope 查询参数的**部分省略**语义（契约只说「全部可选，缺省列全部」）
//!   由本腿装配裁决：`connectionId` 与 `table` 同时在场才成 scope 过滤，
//!   否则列全部——权威裁决随 WR-D2 真实 handler 落地；
//! - 夹具 scope 取值刻意 URL 安全，查询串直拼不做转义。

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post, put};
use axum::{Json, Router};
use datazen::connection::{
    ProfileDraft, ProfilePatch, ProfileRecord, ProfileRepository, ProfileScope, ProfileView,
};
use datazen::execution::application::requests::{
    AttachmentRequest, CloseSessionRequest, OpenSessionRequest, SetSessionContextRequest,
};
use datazen::execution::application::use_cases::ConnectionUseCases;
use datazen::execution::application::{
    policy::{
        AuthorizationAction, AuthorizationDecision, AuthorizationSubject, DelegationGrant,
        PolicyChangeStream, PolicyService,
    },
    submission::SubmissionPresentation,
    RuntimeConnectionUseCases,
};
use serde_json::Value;
use datazen::execution::{
    CloseReceipt, ContextChangeReceipt, IdempotentOperation, OpenSessionReceipt, SessionDirectory,
    SessionHandle, SessionOwner, SessionState, SessionView, SubmissionToken,
    SubmissionTokenIssuer, VerifiedSubmission,
};
use datazen::named_views::{NamedView, NamedViewError, NamedViewScope, NamedViewService};
use datazen::settings::{SettingsError, SettingsService, SharedSettings};
use datazen::shared::context::{OwnerRef, RequestContext};
use datazen::shared::error::{ApiError, ApiErrorCode, PortError};
use datazen::shared::id::{
    ClientInstanceId, ConfigRevision, ConnectionId, Counter, IdempotencyKey, OrganizationId,
    PrincipalId, RequestId, Timestamp, WorkerId,
};
use datazen::use_case_contracts_test_support::connections::{
    self as connections_contract, ConnectionsContractBinding, ContractTokenIssuer,
    MemoryProfileStore,
};
use datazen::use_case_contracts_test_support::sessions::{
    self as sessions_contract, ContractPolicy, ContractSessionBackend, SessionsContractBinding,
};
use datazen::use_case_contracts_test_support::ContractResult;
use datazen::use_case_contracts_test_support::named_views::{
    self as contract, ContractIdMinter, MemoryViewStore, NamedViewsContractBinding,
};
use datazen::use_case_contracts_test_support::settings::{
    self as settings_contract, MemorySettingsStore, SettingsContractBinding,
};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use tower::ServiceExt;

use datazen::execution::runtime::directory::InMemorySessionDirectory;
use datazen_server::http::middleware::auth::AuthenticatedSession;
use datazen_server::http::middleware::authorize::NOT_VISIBLE_MESSAGE;
use datazen_server::http::response::ApiErrorResponse;
use datazen_server::storage::login_sessions::LoginSessionMode;

// ── 装配层：夹具仓储注入 + 端点形态（WR-C2 §6.9）──────────────────────────

/// 命名视图服务从**共享规格夹具**装配：与 IPC 腿同一份 setup 词汇，
/// 不同存储实现（这里是内存假体——服务端的仓储适配器随 WR-D2 落地）。
fn contract_service() -> (Arc<NamedViewService>, Arc<MemoryViewStore>) {
    let store = Arc::new(MemoryViewStore::default());
    let service = Arc::new(NamedViewService::new(
        store.clone(),
        Arc::new(ContractIdMinter::default()),
    ));
    (service, store)
}

/// 错误映射（装配裁决）：application 契约码与文本进 wire 体，状态码按
/// 失败族给出。真实 server host 的映射随 WR-D3 统一错误中间件上线；
/// 规格对状态码零断言，这条映射只服务本腿 pin。
fn application_error_response(error: NamedViewError) -> Response {
    let status = match &error {
        NamedViewError::NameExists { .. } => StatusCode::CONFLICT,
        NamedViewError::Invalid(_) => StatusCode::BAD_REQUEST,
        NamedViewError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    let body = serde_json::json!({
        "code": error.code(),
        "message": error.to_string(),
    });
    (status, Json(body)).into_response()
}

/// `GET /api/v1/named-views`。scope 四元组进查询参数（WR-C2 §6.9）；
/// `connectionId` 与 `table` 同时在场才成 scope 过滤（装配裁决，见模块头）。
async fn list_named_views_handler(
    State(service): State<Arc<NamedViewService>>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let scope = match (
        params.get("connectionId"),
        params.get("database"),
        params.get("schema"),
        params.get("table"),
    ) {
        (Some(connection_id), database, schema, Some(table)) => Some(NamedViewScope {
            connection_id: connection_id.clone(),
            database: database.cloned(),
            schema: schema.cloned(),
            table: table.clone(),
        }),
        _ => None,
    };
    match service.list(scope.as_ref()).await {
        Ok(views) => (StatusCode::OK, Json(views)).into_response(),
        Err(error) => application_error_response(error),
    }
}

#[derive(Deserialize)]
struct SaveNamedViewBody {
    view: NamedView,
}

/// `PUT /api/v1/named-views`。空 `id` 由后端铸造 → 201；已知 id 更新 → 200
/// （WR-C2 §6.9 的成功侧裁决；「空 id 即铸造」是 application 语义，装配
/// 只把它翻成状态码）。
async fn save_named_view_handler(
    State(service): State<Arc<NamedViewService>>,
    Json(body): Json<SaveNamedViewBody>,
) -> Response {
    let minted = body.view.id.trim().is_empty();
    match service.save(body.view).await {
        Ok(saved) => {
            if minted {
                (StatusCode::CREATED, Json(saved)).into_response()
            } else {
                (StatusCode::OK, Json(saved)).into_response()
            }
        }
        Err(error) => application_error_response(error),
    }
}

/// `DELETE /api/v1/named-views/{id}`。未知 id 无操作（application 幂等
/// 语义），答案恒为剩余集合 → 200。
async fn delete_named_view_handler(
    State(service): State<Arc<NamedViewService>>,
    Path(id): Path<String>,
) -> Response {
    match service.delete(&id).await {
        Ok(remaining) => (StatusCode::OK, Json(remaining)).into_response(),
        Err(error) => application_error_response(error),
    }
}

/// 本腿被测 Router：端点形态与 WR-C2 §6.9 一一对应，中间件链不进本刀
/// （requestId/认证/体闸随 WR-D 真实装配；本腿钉的是「handler 调同一
/// 用例」的用例半边）。
fn contract_router(service: Arc<NamedViewService>) -> Router {
    Router::new()
        .route(
            "/api/v1/named-views",
            get(list_named_views_handler).put(save_named_view_handler),
        )
        .route(
            "/api/v1/named-views/{id}",
            axum::routing::delete(delete_named_view_handler),
        )
        .with_state(service)
}

// ── binding：HTTP 腿对共享规格端口的实现 ──────────────────────────────────

struct HttpBinding {
    app: Router,
    store: Arc<MemoryViewStore>,
}

/// 夹具 scope 值是 URL 安全的（`contract_scope`/`other_scope` 的取值
/// 纪律），查询串直拼。
fn scope_query(scope: &NamedViewScope) -> String {
    let mut query = format!("connectionId={}", scope.connection_id);
    if let Some(database) = &scope.database {
        query.push_str(&format!("&database={database}"));
    }
    if let Some(schema) = &scope.schema {
        query.push_str(&format!("&schema={schema}"));
    }
    query.push_str(&format!("&table={}", scope.table));
    query
}

async fn wire(response: Response) -> (StatusCode, serde_json::Value) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    let value = serde_json::from_slice::<serde_json::Value>(&bytes)
        .expect("every body of this router is JSON");
    (status, value)
}

/// 成功侧解析回 application DTO；错误侧抽回规格的错误观测词汇。
fn outcome<T: DeserializeOwned>(status: StatusCode, value: serde_json::Value) -> ContractResult<T> {
    if status.is_success() {
        return serde_json::from_value(value).map_err(|error| {
            datazen::use_case_contracts_test_support::ContractFailure {
                code: None,
                message: format!("wire body did not deserialize into the application DTO: {error}"),
            }
        });
    }
    Err(datazen::use_case_contracts_test_support::ContractFailure {
        code: value["code"].as_str().map(str::to_string),
        message: value["message"].as_str().unwrap_or_default().to_string(),
    })
}

/// `?Send` 与共享规格端口同口径（见 modules `NamedViewsContractBinding`
/// 文档）：binding 由单个测试的栈持有，oneshot 顺序 await。
#[async_trait(?Send)]
impl NamedViewsContractBinding for HttpBinding {
    async fn seed(&self, views: Vec<NamedView>) -> Result<(), String> {
        // setup 直写装配的存储（不经用例、不经 HTTP）——与 IPC 腿经
        // DesktopRepository 落盘同位。
        self.store.seed(views).await;
        Ok(())
    }

    async fn list(&self, scope: Option<NamedViewScope>) -> ContractResult<Vec<NamedView>> {
        let uri = match &scope {
            Some(scope) => format!("/api/v1/named-views?{}", scope_query(scope)),
            None => "/api/v1/named-views".to_string(),
        };
        let response = self
            .app
            .clone()
            .oneshot(
                Request::get(uri)
                    .body(Body::empty())
                    .expect("request body is empty"),
            )
            .await
            .expect("oneshot dispatch");
        let (status, value) = wire(response).await;
        outcome(status, value)
    }

    async fn save(&self, view: NamedView) -> ContractResult<NamedView> {
        let response = self
            .app
            .clone()
            .oneshot(
                Request::put("/api/v1/named-views")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&serde_json::json!({ "view": view }))
                            .expect("the request body serializes"),
                    ))
                    .expect("request body"),
            )
            .await
            .expect("oneshot dispatch");
        let (status, value) = wire(response).await;
        outcome(status, value)
    }

    async fn delete(&self, id: String) -> ContractResult<Vec<NamedView>> {
        let response = self
            .app
            .clone()
            .oneshot(
                Request::delete(format!("/api/v1/named-views/{id}"))
                    .body(Body::empty())
                    .expect("request body is empty"),
            )
            .await
            .expect("oneshot dispatch");
        let (status, value) = wire(response).await;
        outcome(status, value)
    }
}

fn harness() -> HttpBinding {
    let (service, store) = contract_service();
    HttpBinding {
        app: contract_router(service),
        store,
    }
}

// ── 共享规格：七个 scenario，两腿逐字同名 ─────────────────────────────────

#[tokio::test]
async fn saves_a_new_view_and_returns_the_stored_record() {
    contract::saves_a_new_view_and_returns_the_stored_record(&harness()).await;
}

#[tokio::test]
async fn listing_orders_default_first_then_by_name() {
    contract::listing_orders_default_first_then_by_name(&harness()).await;
}

#[tokio::test]
async fn renaming_keeps_the_identity_and_the_creation_time() {
    contract::renaming_keeps_the_identity_and_the_creation_time(&harness()).await;
}

#[tokio::test]
async fn the_one_default_per_scope_rule_is_enforced_in_one_write() {
    contract::the_one_default_per_scope_rule_is_enforced_in_one_write(&harness()).await;
}

#[tokio::test]
async fn deleting_an_unknown_id_is_a_no_op_and_the_known_id_is_gone() {
    contract::deleting_an_unknown_id_is_a_no_op_and_the_known_id_is_gone(&harness()).await;
}

#[tokio::test]
async fn duplicate_names_in_one_scope_are_refused() {
    contract::duplicate_names_in_one_scope_are_refused(&harness()).await;
}

#[tokio::test]
async fn invalid_records_are_refused_before_the_repository() {
    contract::invalid_records_are_refused_before_the_repository(&harness()).await;
}

// ── 装配层断言（形态差异唯一允许出现的位置）───────────────────────────────

/// 成功侧状态码：空 id 铸造 → 201，已知 id 更新 → 200（WR-C2 §6.9）。
/// 这是传输事实，规格不携带状态码——差异钉在本腿装配层。
#[tokio::test]
async fn save_on_the_wire_answers_201_for_a_minted_view_and_200_for_an_update() {
    let binding = harness();
    binding.seed(Vec::new()).await.expect("seed");
    let response = binding
        .app
        .clone()
        .oneshot(
            Request::put("/api/v1/named-views")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "view": contract::draft_fixture("Monthly orders", &contract::contract_scope())
                    }))
                    .expect("the request body serializes"),
                ))
                .expect("request body"),
        )
        .await
        .expect("oneshot dispatch");
    assert_eq!(response.status(), StatusCode::CREATED, "empty id is minted");

    let mut update = contract::draft_fixture("Renamed", &contract::contract_scope());
    update.id = "view-kept".into();
    binding
        .seed(vec![contract::stored_fixture(
            "view-kept",
            "Old name",
            &contract::contract_scope(),
            false,
        )])
        .await
        .expect("seed the known id");
    let response = binding
        .app
        .clone()
        .oneshot(
            Request::put("/api/v1/named-views")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "view": update }))
                        .expect("the request body serializes"),
                ))
                .expect("request body"),
        )
        .await
        .expect("oneshot dispatch");
    assert_eq!(response.status(), StatusCode::OK, "a known id updates");
}

/// 错误 wire 形态 pin：规则拒绝是 409，wire 体是 **JSON 对象**
/// `{code, message}`（与 IPC 腿的纯字符串信封相对照——形态差异钉在
/// 各自装配层）；`code` 字段携带 application 契约码。
#[tokio::test]
async fn name_exists_on_the_wire_is_a_409_with_the_contract_code_in_the_body() {
    let binding = harness();
    binding
        .seed(vec![contract::stored_fixture(
            "view-1",
            "Users",
            &contract::contract_scope(),
            false,
        )])
        .await
        .expect("seed");
    let response = binding
        .app
        .clone()
        .oneshot(
            Request::put("/api/v1/named-views")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "view": contract::draft_fixture("users", &contract::contract_scope())
                    }))
                    .expect("the request body serializes"),
                ))
                .expect("request body"),
        )
        .await
        .expect("oneshot dispatch");
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let (_, value) = wire(response).await;
    assert_eq!(value["code"], "grid.view.nameExists");
    assert_eq!(
        value["message"],
        "grid.view.nameExists: a saved view with this name already exists for this table"
    );
}

/// scope 查询参数 pin：查询串进**线**过滤（wire 语义），与共享规格经
/// binding 断言的 scope 过滤互为表里。
#[tokio::test]
async fn scope_query_params_drive_the_wire_filter() {
    let binding = harness();
    binding
        .seed(vec![
            contract::stored_fixture("view-a", "Users", &contract::contract_scope(), false),
            contract::stored_fixture("view-b", "Users", &contract::other_scope(), false),
        ])
        .await
        .expect("seed two scopes");
    let response = binding
        .app
        .clone()
        .oneshot(
            Request::get(format!(
                "/api/v1/named-views?{}",
                scope_query(&contract::contract_scope())
            ))
            .body(Body::empty())
            .expect("request body is empty"),
        )
        .await
        .expect("oneshot dispatch");
    assert_eq!(response.status(), StatusCode::OK);
    let (_, value) = wire(response).await;
    let views: Vec<NamedView> =
        serde_json::from_value(value).expect("the answer is a NamedView array");
    assert_eq!(views.len(), 1, "the query string filters on the wire");
    assert_eq!(views[0].id, "view-a");
}

/// 不完整 scope 查询参数（缺 `table`）不成过滤：列全部——本腿装配裁决
/// 的显式 pin（契约只说「全部可选，缺省列全部」；权威裁决随 WR-D2）。
#[tokio::test]
async fn a_partial_scope_query_does_not_filter_the_wire_answer() {
    let binding = harness();
    binding
        .seed(vec![
            contract::stored_fixture("view-a", "Users", &contract::contract_scope(), false),
            contract::stored_fixture("view-b", "Users", &contract::other_scope(), false),
        ])
        .await
        .expect("seed two scopes");
    let response = binding
        .app
        .clone()
        .oneshot(
            Request::get("/api/v1/named-views?connectionId=conn-contract")
                .body(Body::empty())
                .expect("request body is empty"),
        )
        .await
        .expect("oneshot dispatch");
    let (_, value) = wire(response).await;
    let views: Vec<NamedView> =
        serde_json::from_value(value).expect("the answer is a NamedView array");
    assert_eq!(
        views.len(),
        2,
        "no table in the query means no scope filter"
    );
}

// ── settings 域装配层：服务注入 + 端点形态（http-api-contract §6.1）────────

/// 设置服务从**共享规格夹具**装配：与 IPC 腿同一份 setup 词汇，不同存储
/// 实现（这里是内存假体——服务端的仓储适配器随 WR-D2 落地）。
fn settings_contract_service() -> (Arc<SettingsService>, Arc<MemorySettingsStore>) {
    let store = Arc::new(MemorySettingsStore::default());
    let service = Arc::new(SettingsService::new(store.clone()));
    (service, store)
}

/// settings 错误映射（装配裁决）：本域没有 application 契约码——wire 体
/// `code` 字段**恒为 null**（与 IPC 腿一致的「无码」契约事实），application
/// 文本进 `message` 字段；状态码按失败族给出（校验拒绝 400 / 存储失败
/// 500）。真实 server host 的映射随 WR-D3 统一错误中间件上线；规格对
/// 状态码零断言，这条映射只服务本腿 pin。
fn settings_error_response(error: SettingsError) -> Response {
    let status = match &error {
        SettingsError::Validation(_) => StatusCode::BAD_REQUEST,
        SettingsError::Repository(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    let code: Option<String> = None;
    let body = serde_json::json!({
        "code": code,
        "message": error.to_string(),
    });
    (status, Json(body)).into_response()
}

/// `GET /api/v1/settings` → 200 `SharedSettings`（§6.1）。
async fn get_settings_handler(
    State(service): State<Arc<SettingsService>>,
) -> Response {
    match service.read().await {
        Ok(settings) => (StatusCode::OK, Json(settings)).into_response(),
        Err(error) => settings_error_response(error),
    }
}

#[derive(Deserialize)]
struct SaveSettingsBody {
    settings: SharedSettings,
}

/// `PUT /api/v1/settings`（`{ settings }`）→ 200 `SharedSettings`（§6.1；
/// 与 named-views 的铸造 201 相对——settings 无铸造，保存即 200）。
async fn put_settings_handler(
    State(service): State<Arc<SettingsService>>,
    Json(body): Json<SaveSettingsBody>,
) -> Response {
    match service.save(body.settings).await {
        Ok(settings) => (StatusCode::OK, Json(settings)).into_response(),
        Err(error) => settings_error_response(error),
    }
}

/// settings 被测 Router：端点形态与 §6.1 一一对应（GET 读、PUT 保存），
/// 中间件链不进本刀（requestId/认证/体闸随 WR-D 真实装配）。
fn settings_contract_router(service: Arc<SettingsService>) -> Router {
    Router::new()
        .route(
            "/api/v1/settings",
            get(get_settings_handler).put(put_settings_handler),
        )
        .with_state(service)
}

// ── settings binding：HTTP 腿对共享规格端口的实现 ─────────────────────────

struct SettingsHttpBinding {
    app: Router,
    store: Arc<MemorySettingsStore>,
}

async fn settings_wire(response: Response) -> (StatusCode, serde_json::Value) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    let value = serde_json::from_slice::<serde_json::Value>(&bytes)
        .expect("every body of this router is JSON");
    (status, value)
}

/// settings 腿的 wire 结果解析：成功侧回 application DTO；错误侧把
/// `{code: null, message}` 抽回规格的错误观测词汇（code 恒 `None`）。
fn settings_outcome(status: StatusCode, value: serde_json::Value) -> ContractResult<SharedSettings> {
    if status.is_success() {
        return serde_json::from_value(value).map_err(|error| {
            datazen::use_case_contracts_test_support::ContractFailure {
                code: None,
                message: format!("wire body did not deserialize into the application DTO: {error}"),
            }
        });
    }
    Err(datazen::use_case_contracts_test_support::ContractFailure {
        code: value["code"].as_str().map(str::to_string),
        message: value["message"].as_str().unwrap_or_default().to_string(),
    })
}

/// `?Send` 与共享规格端口同口径（见 modules `SettingsContractBinding`
/// 文档）：binding 由单个测试的栈持有，oneshot 顺序 await。
#[async_trait(?Send)]
impl SettingsContractBinding for SettingsHttpBinding {
    async fn seed(&self, settings: SharedSettings) -> Result<(), String> {
        // setup 直写装配的存储（不经用例、不经 HTTP）——与 IPC 腿经
        // DesktopRepository 落盘同位。
        self.store.seed(settings).await;
        Ok(())
    }

    async fn fail_writes(&self) -> Result<(), String> {
        self.store.fail_writes().await;
        Ok(())
    }

    async fn get(&self) -> ContractResult<SharedSettings> {
        let response = self
            .app
            .clone()
            .oneshot(
                Request::get("/api/v1/settings")
                    .body(Body::empty())
                    .expect("request body is empty"),
            )
            .await
            .expect("oneshot dispatch");
        let (status, value) = settings_wire(response).await;
        settings_outcome(status, value)
    }

    async fn save(&self, settings: SharedSettings) -> ContractResult<SharedSettings> {
        let response = self
            .app
            .clone()
            .oneshot(
                Request::put("/api/v1/settings")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&serde_json::json!({ "settings": settings }))
                            .expect("the request body serializes"),
                    ))
                    .expect("request body"),
            )
            .await
            .expect("oneshot dispatch");
        let (status, value) = settings_wire(response).await;
        settings_outcome(status, value)
    }
}

fn settings_harness() -> SettingsHttpBinding {
    let (service, store) = settings_contract_service();
    SettingsHttpBinding {
        app: settings_contract_router(service),
        store,
    }
}

// ── settings 共享规格：三个 scenario，三腿逐字同名 ─────────────────────────

#[tokio::test]
async fn saving_settings_normalizes_and_round_trips() {
    settings_contract::saving_settings_normalizes_and_round_trips(&settings_harness()).await;
}

#[tokio::test]
async fn invalid_enum_values_are_refused_before_the_repository() {
    settings_contract::invalid_enum_values_are_refused_before_the_repository(&settings_harness())
        .await;
}

#[tokio::test]
async fn a_storage_failure_surfaces_without_a_contract_code() {
    settings_contract::a_storage_failure_surfaces_without_a_contract_code(&settings_harness())
        .await;
}

// ── settings 装配层断言（形态差异唯一允许出现的位置）───────────────────────

/// 端点形态 pin：GET 回 **200** + 完整共享文档；PUT 也回 **200**（§6.1
/// 裁决——settings 无铸造语义，与 named-views 的 201 相对照）。这是传输
/// 事实，规格不携带状态码。
#[tokio::test]
async fn settings_wire_answers_200_for_both_read_and_save() {
    let binding = settings_harness();
    let seeded = settings_contract::contract_settings();
    binding.seed(seeded.clone()).await.expect("seed");

    let response = binding
        .app
        .clone()
        .oneshot(
            Request::get("/api/v1/settings")
                .body(Body::empty())
                .expect("request body is empty"),
        )
        .await
        .expect("oneshot dispatch");
    assert_eq!(response.status(), StatusCode::OK);
    let (_, value) = settings_wire(response).await;
    let read: SharedSettings = serde_json::from_value(value).expect("the answer is SharedSettings");
    assert_eq!(read, seeded, "the wire answer is the stored document");

    let mut update = seeded.clone();
    update.preferences.language = "zh-CN".into();
    let response = binding
        .app
        .clone()
        .oneshot(
            Request::put("/api/v1/settings")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "settings": update }))
                        .expect("the request body serializes"),
                ))
                .expect("request body"),
        )
        .await
        .expect("oneshot dispatch");
    assert_eq!(response.status(), StatusCode::OK, "save answers 200");
}

/// 错误 wire 形态 pin：校验拒绝是 400，wire 体是 **JSON 对象**
/// `{code: null, message}`——`code` 恒 null 是本域契约事实（无契约码），
/// application 原句进 `message`。
#[tokio::test]
async fn settings_validation_refusal_on_the_wire_is_a_400_with_a_codeless_body() {
    let binding = settings_harness();
    binding
        .seed(settings_contract::contract_settings())
        .await
        .expect("seed");
    let mut invalid = settings_contract::contract_settings();
    invalid.preferences.theme.mode = "neon".into();

    let response = binding
        .app
        .clone()
        .oneshot(
            Request::put("/api/v1/settings")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "settings": invalid }))
                        .expect("the request body serializes"),
                ))
                .expect("request body"),
        )
        .await
        .expect("oneshot dispatch");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let (_, value) = settings_wire(response).await;
    assert!(
        value["code"].is_null(),
        "settings has no contract code: {}",
        value["code"]
    );
    assert_eq!(value["message"], "Invalid settings enum value");
}

/// 存储失败 wire 形态 pin：写路径故障是 500，wire 体 `{code: null,
/// message}`——application 文本（`backend unavailable: …`）原样进
/// `message`，与 IPC 腿的宿主固定句相对照（差异钉在各自装配层）。
#[tokio::test]
async fn settings_storage_failure_on_the_wire_is_a_500_with_a_codeless_body() {
    let binding = settings_harness();
    binding
        .seed(settings_contract::contract_settings())
        .await
        .expect("seed");
    binding.fail_writes().await.expect("writes fail");

    let response = binding
        .app
        .clone()
        .oneshot(
            Request::put("/api/v1/settings")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "settings": settings_contract::contract_settings()
                    }))
                    .expect("the request body serializes"),
                ))
                .expect("request body"),
        )
        .await
        .expect("oneshot dispatch");
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let (_, value) = settings_wire(response).await;
    assert!(value["code"].is_null(), "storage failures carry no code");
    assert_eq!(value["message"], "backend unavailable: write failed");
}

// ══ connections 域（WR-C3 余域）与两腿共用的最小认证链形状 ══════════════════
//
// 真实 `server::http::domains` 是 `pub(crate)`（WR-C2 §6 读侧路由由
// composition root 预装配），不进集成测试可见面——本刀沿 settings 腿先例
// 在测试内搭**最小 Router**：端点形态与 handler 语义逐条对照真实域文件
// （`server/src/http/domains/connections.rs` / `sessions.rs`），handler 调
// **同一个** modules 用例面。真实 Router 的 merge 装配（认证链 L2–L6）随
// WR-D3/D4；主线若把 `domains` 提为 `pub mod`（或经 test-support 再导出），
// 本腿可整体换成 `domains::connections_router` 直绑——登记为主线建议。

/// 本腿认证链事实（org/principal 与共享规格夹具同一组织；测试请求用
/// `.extension(...)` 注入，模拟 L2 认证链产物，真实中间件链不进本刀）。
const CONTRACT_PRINCIPAL: &str = "user-contract";

fn authenticated_session() -> AuthenticatedSession {
    AuthenticatedSession {
        organization_id: connections_contract::CONTRACT_ORGANIZATION.to_owned(),
        principal_id: CONTRACT_PRINCIPAL.to_owned(),
        session_id: "contract-login-session".to_owned(),
        permission_version: 1,
        token_epoch: 1,
        auth_mode: LoginSessionMode::Cookie,
    }
}

/// 与真实 `http::router::SessionIdentity` 同形的提取器：身份只来自认证链
/// 扩展（INV-S1），测试装配里 client 实例与真实形状同值（"http"）。
struct SessionIdentity {
    ctx: RequestContext,
}

impl<S: Send + Sync> FromRequestParts<S> for SessionIdentity {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let Some(session) = parts.extensions.get::<AuthenticatedSession>() else {
            return Err(unauthorized());
        };
        Ok(Self {
            ctx: RequestContext::new(
                OrganizationId::new(session.organization_id.clone()),
                PrincipalId::new(session.principal_id.clone()),
                None,
                ClientInstanceId::new("http"),
                RequestId::new("req-contract-http"),
                None,
            ),
        })
    }
}

/// 与 binding 侧 `http_context()` 同一身份事实（令牌签发与出示的交叉核对
/// 两侧必须一致）。
fn http_context() -> RequestContext {
    RequestContext::new(
        OrganizationId::new(connections_contract::CONTRACT_ORGANIZATION),
        PrincipalId::new(CONTRACT_PRINCIPAL),
        None,
        ClientInstanceId::new("http"),
        RequestId::new("req-contract-http"),
        None,
    )
}

fn unauthorized() -> Response {
    ApiErrorResponse::new(ApiError::new(
        ApiErrorCode::Unauthenticated,
        "Authentication required.",
    ))
    .with_status(StatusCode::UNAUTHORIZED)
    .into_response()
}

fn invalid_argument(message: impl Into<String>) -> Response {
    ApiErrorResponse::new(ApiError::new(ApiErrorCode::InvalidArgument, message)).into_response()
}

fn error_response(error: ApiError) -> Response {
    ApiErrorResponse::new(error).into_response()
}

/// 越组织/不存在的统一 404（防 ID 枚举；与 L6 同一条固定文案——两条腿
/// 的域文件共用同一个常量来源）。
fn not_visible() -> Response {
    error_response(ApiError::new(ApiErrorCode::NotFound, NOT_VISIBLE_MESSAGE))
}

// ── connections 域装配层：端口注入 + 端点形态（§6.3）──────────────────────

fn connections_contract_service() -> (Arc<MemoryProfileStore>, Arc<ContractTokenIssuer>) {
    (
        Arc::new(MemoryProfileStore::default()),
        Arc::new(ContractTokenIssuer::default()),
    )
}

/// 连接域的装配状态（与真实 `domains::ConnectionsState` 同形）。
#[derive(Clone)]
struct ConnectionsState {
    profiles: Arc<dyn ProfileRepository>,
    issuer: Arc<dyn SubmissionTokenIssuer>,
}

/// 本域的 `PortError` → wire 映射（`CasConflict` → `configRevisionMismatch`
/// 是端口模块头的 server host 裁决——真实域文件的逐字镜像）。
fn profile_error(error: PortError) -> ApiError {
    match error {
        PortError::CasConflict { entity, id } => ApiError::new(
            ApiErrorCode::ConfigRevisionMismatch,
            format!("{entity} {id} changed concurrently; re-read configRevision and retry."),
        ),
        PortError::NotFound(_) => ApiError::new(ApiErrorCode::NotFound, NOT_VISIBLE_MESSAGE),
        PortError::BackendUnavailable(detail) | PortError::ProviderTimeout(detail) => {
            ApiError::new(ApiErrorCode::ServiceUnavailable, detail)
        }
        other => ApiError::new(ApiErrorCode::ServiceUnavailable, other.to_string()),
    }
}

/// 令牌载荷缺省过期（与真实域文件的 DISTANT_FUTURE 同值）。
const DISTANT_FUTURE: &str = "9999-12-31T23:59:59Z";

/// 请求体 → 令牌出示（真实 `presentation_of` 的镜像：submissionToken 接受
/// 字符串或 `{idempotencyKey, expiresAt}`）。
fn presentation_of(body: &Value, ctx: &RequestContext) -> Result<SubmissionPresentation, Response> {
    let (idempotency_key, expires_at) = match body.get("submissionToken") {
        Some(Value::String(raw)) => (raw.clone(), DISTANT_FUTURE.to_string()),
        Some(Value::Object(map)) => (
            map.get("idempotencyKey")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            map.get("expiresAt")
                .and_then(Value::as_str)
                .unwrap_or(DISTANT_FUTURE)
                .to_string(),
        ),
        _ => return Err(invalid_argument("submissionToken is required.")),
    };
    Ok(SubmissionPresentation {
        token: SubmissionToken {
            idempotency_key: IdempotencyKey::new(idempotency_key),
            expires_at: Timestamp::new(expires_at),
        },
        claimed_organization_id: ctx.organization_id.clone(),
        claimed_principal_id: ctx.principal_id.clone(),
        claimed_client_instance_id: ctx.client_instance_id.clone(),
        claimed_delegation_id: ctx.delegation_id.clone(),
    })
}

/// 令牌校验三步（真实域文件的镜像）：签名/过期 → 身份交叉核对 → operation
/// 白名单（配置写入类只有 `createProfile`）。
async fn verify_config_write_token(
    state: &ConnectionsState,
    ctx: &RequestContext,
    body: &Value,
) -> Result<VerifiedSubmission, Response> {
    let presentation = presentation_of(body, ctx)?;
    let verified = match state.issuer.verify(&presentation).await {
        Ok(verified) => verified,
        Err(PortError::TokenInvalid) => {
            return Err(error_response(ApiError::new(
                ApiErrorCode::Unauthenticated,
                "Submission token is invalid, unknown-keyed, or expired.",
            )));
        }
        Err(source) => {
            return Err(error_response(ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                format!("token verification failed: {source}"),
            )));
        }
    };
    if verified.organization_id != ctx.organization_id
        || verified.principal_id != ctx.principal_id
        || verified.client_instance_id != ctx.client_instance_id
    {
        return Err(error_response(ApiError::new(
            ApiErrorCode::Unauthenticated,
            "Submission identity does not match the authenticated context.",
        )));
    }
    if verified.operation != IdempotentOperation::CreateProfile {
        return Err(invalid_argument(
            "This endpoint only accepts createProfile tokens.",
        ));
    }
    Ok(verified)
}

/// `expectedRevision` 的 wire 形态是 Counter（十进制字符串，CM-01）；数字
/// 字面量也收（真实域文件的镜像）。
fn expected_revision_of(body: &Value) -> Option<u64> {
    match body.get("expectedRevision") {
        Some(Value::String(raw)) => raw.trim().parse::<u64>().ok(),
        Some(Value::Number(value)) => value.as_u64(),
        _ => None,
    }
}

async fn connections_list_handler(
    State(state): State<ConnectionsState>,
    identity: SessionIdentity,
) -> Response {
    match state
        .profiles
        .list(&identity.ctx, ProfileScope::from_context(&identity.ctx))
        .await
    {
        Ok(records) => {
            let views: Vec<ProfileView> = records
                .iter()
                .filter(|record| {
                    record.organization_id == identity.ctx.organization_id && record.enabled
                })
                .map(ProfileView::from_record)
                .collect();
            Json(views).into_response()
        }
        Err(error) => error_response(profile_error(error)),
    }
}

async fn connections_create_handler(
    State(state): State<ConnectionsState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let verified = match verify_config_write_token(&state, &identity.ctx, &body).await {
        Ok(verified) => verified,
        Err(response) => return response,
    };
    let draft: ProfileDraft = match serde_json::from_value(body) {
        Ok(draft) => draft,
        Err(source) => return invalid_argument(format!("draft: {source}")),
    };
    match state
        .profiles
        .create(&identity.ctx, draft, &verified.idempotency_key)
        .await
    {
        Ok(record) => (
            StatusCode::CREATED,
            Json(ProfileView::from_record(&record)),
        )
            .into_response(),
        Err(error) => error_response(profile_error(error)),
    }
}

async fn connections_patch_handler(
    State(state): State<ConnectionsState>,
    identity: SessionIdentity,
    Path(connection_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let verified = match verify_config_write_token(&state, &identity.ctx, &body).await {
        Ok(verified) => verified,
        Err(response) => return response,
    };
    let _ = verified.idempotency_key; // 令牌即受理键；CAS/禁用的幂等由仓储版本承担
    let connection_id = ConnectionId::new(connection_id);
    // 判别（§6.3 注）：顶层 `enabled: false` 只属于 disable 语义。
    if matches!(body.get("enabled"), Some(Value::Bool(false))) {
        return connections_disable(state, identity.ctx, connection_id, body).await;
    }
    connections_update(state, identity.ctx, connection_id, body).await
}

/// disable 语义：先可见性判定，再核对 `expectedRevision`（handler 侧 CAS
/// ——端口 `disable` 不带版本），最后翻 `enabled`（真实域文件的镜像）。
async fn connections_disable(
    state: ConnectionsState,
    ctx: RequestContext,
    connection_id: ConnectionId,
    body: Value,
) -> Response {
    let Some(expected) = expected_revision_of(&body) else {
        return invalid_argument("expectedRevision is required (decimal string counter).");
    };
    let record = match state.profiles.get(&ctx, connection_id.clone()).await {
        Ok(Some(record)) => record,
        Ok(None) => return not_visible(),
        Err(error) => return error_response(profile_error(error)),
    };
    if record.config_revision.counter().get() != expected {
        return error_response(ApiError::new(
            ApiErrorCode::ConfigRevisionMismatch,
            "The connection configuration changed concurrently; re-read configRevision and retry.",
        ));
    }
    match state.profiles.disable(&ctx, connection_id).await {
        Ok(record) => Json(ProfileView::from_record(&record)).into_response(),
        Err(error) => error_response(profile_error(error)),
    }
}

/// update 语义：`{expectedRevision, patch}`，CAS 冲突 → 409
/// `configRevisionMismatch`（真实域文件的镜像）。
async fn connections_update(
    state: ConnectionsState,
    ctx: RequestContext,
    connection_id: ConnectionId,
    body: Value,
) -> Response {
    let Some(expected) = expected_revision_of(&body) else {
        return invalid_argument("expectedRevision is required (decimal string counter).");
    };
    let patch: ProfilePatch = match body.get("patch").cloned() {
        Some(patch) => match serde_json::from_value(patch) {
            Ok(patch) => patch,
            Err(source) => return invalid_argument(format!("patch: {source}")),
        },
        None => return invalid_argument("patch is required."),
    };
    match state
        .profiles
        .compare_and_set(&ctx, connection_id, ConfigRevision::new(expected), patch)
        .await
    {
        Ok(record) => Json(ProfileView::from_record(&record)).into_response(),
        Err(error) => error_response(profile_error(error)),
    }
}

fn connections_contract_router(profiles: Arc<MemoryProfileStore>, issuer: Arc<ContractTokenIssuer>) -> Router {
    Router::new()
        .route(
            "/api/v1/connections",
            get(connections_list_handler).post(connections_create_handler),
        )
        .route(
            "/api/v1/connections/{id}",
            patch(connections_patch_handler),
        )
        .with_state(ConnectionsState {
            profiles,
            issuer,
        })
}

// ── connections binding：HTTP 腿对共享规格端口的实现 ──────────────────────

struct ConnectionsHttpBinding {
    app: Router,
    store: Arc<MemoryProfileStore>,
    issuer: Arc<ContractTokenIssuer>,
    ctx: RequestContext,
}

impl ConnectionsHttpBinding {
    /// 配置写路由的令牌是传输形状的一半（§6.3 `& SubmitRequest`）：规格
    /// 场景只携带 application 词汇，本腿在装配层自签（签发-消费同一 issuer
    /// 实例——真实装配里与 `handler_issue_token` 同一约定）。
    async fn config_write_token(&self) -> SubmissionToken {
        self.issuer
            .issue(&self.ctx, IdempotentOperation::CreateProfile, None)
            .await
            .expect("the config-write token issues")
    }

    async fn dispatch(&self, request: Request<Body>) -> (StatusCode, Value) {
        let response = self
            .app
            .clone()
            .oneshot(request)
            .await
            .expect("oneshot dispatch");
        wire(response).await
    }
}

#[async_trait(?Send)]
impl ConnectionsContractBinding for ConnectionsHttpBinding {
    async fn seed(&self, records: Vec<ProfileRecord>) -> Result<(), String> {
        self.store.seed(records).await;
        Ok(())
    }

    async fn issue_create_profile_token(&self) -> ContractResult<SubmissionToken> {
        // 签发面照 handler_issue_token：issuer.issue 即签发（真实 Router 把
        // 它挂在 POST /api/v1/submission-tokens；DomainState 的其余面不进
        // 本刀，签发端口同一）。
        self.issuer
            .issue(&self.ctx, IdempotentOperation::CreateProfile, None)
            .await
            .map_err(connections_contract::port_failure)
    }

    async fn create(
        &self,
        draft: ProfileDraft,
        token: SubmissionToken,
    ) -> ContractResult<ProfileView> {
        let mut body = serde_json::to_value(&draft).expect("the draft serializes");
        body["submissionToken"] =
            Value::String(token.idempotency_key.as_str().to_owned());
        let (status, value) = self
            .dispatch(
                Request::post("/api/v1/connections")
                    .header("content-type", "application/json")
                    .extension(authenticated_session())
                    .body(Body::from(
                        serde_json::to_vec(&body).expect("the request body serializes"),
                    ))
                    .expect("request body"),
            )
            .await;
        outcome(status, value)
    }

    async fn update(
        &self,
        connection_id: ConnectionId,
        expected_revision: ConfigRevision,
        patch: ProfilePatch,
    ) -> ContractResult<ProfileView> {
        let token = self.config_write_token().await;
        let body = serde_json::json!({
            "expectedRevision": expected_revision.get().to_string(),
            "patch": serde_json::to_value(&patch).expect("the patch serializes"),
            "submissionToken": token.idempotency_key.as_str(),
        });
        let (status, value) = self
            .dispatch(
                Request::patch(format!("/api/v1/connections/{}", connection_id.as_str()))
                    .header("content-type", "application/json")
                    .extension(authenticated_session())
                    .body(Body::from(
                        serde_json::to_vec(&body).expect("the request body serializes"),
                    ))
                    .expect("request body"),
            )
            .await;
        outcome(status, value)
    }

    async fn disable(&self, connection_id: ConnectionId) -> ContractResult<ProfileView> {
        // §6.3 的 disable wire 形状带 expectedRevision（handler 侧 CAS）——
        // 端口 `disable` 不带版本，当前版本由本腿从装配的仓储读出（setup
        // 事实，不是规格词汇）。
        let record = self
            .store
            .get(&self.ctx, connection_id.clone())
            .await
            .expect("the disable target reads")
            .expect("the disable target exists");
        let token = self.config_write_token().await;
        let body = serde_json::json!({
            "expectedRevision": record.config_revision.get().to_string(),
            "enabled": false,
            "submissionToken": token.idempotency_key.as_str(),
        });
        let (status, value) = self
            .dispatch(
                Request::patch(format!("/api/v1/connections/{}", connection_id.as_str()))
                    .header("content-type", "application/json")
                    .extension(authenticated_session())
                    .body(Body::from(
                        serde_json::to_vec(&body).expect("the request body serializes"),
                    ))
                    .expect("request body"),
            )
            .await;
        outcome(status, value)
    }

    async fn list(&self) -> ContractResult<Vec<ProfileView>> {
        let (status, value) = self
            .dispatch(
                Request::get("/api/v1/connections")
                    .extension(authenticated_session())
                    .body(Body::empty())
                    .expect("request body is empty"),
            )
            .await;
        outcome(status, value)
    }
}

fn connections_harness() -> ConnectionsHttpBinding {
    let (store, issuer) = connections_contract_service();
    ConnectionsHttpBinding {
        app: connections_contract_router(store.clone(), issuer.clone()),
        store,
        issuer,
        ctx: http_context(),
    }
}

// ── connections 共享规格：三个 scenario，三腿逐字同名 ─────────────────────

#[tokio::test]
async fn a_created_connection_answers_a_purged_view_and_appears_in_the_list() {
    connections_contract::a_created_connection_answers_a_purged_view_and_appears_in_the_list(
        &connections_harness(),
    )
    .await;
}

#[tokio::test]
async fn a_stale_expected_revision_is_refused_as_config_revision_mismatch() {
    connections_contract::a_stale_expected_revision_is_refused_as_config_revision_mismatch(
        &connections_harness(),
    )
    .await;
}

#[tokio::test]
async fn a_disabled_connection_reports_disabled_and_leaves_the_default_list() {
    connections_contract::a_disabled_connection_reports_disabled_and_leaves_the_default_list(
        &connections_harness(),
    )
    .await;
}

// ── connections 装配层断言（形态差异唯一允许出现的位置）───────────────────

/// 端点形态 pin：POST 回答 **201**（§6.3 的受理语义），GET 列表回答 **200**
/// 数组。缺令牌的创建被 **400** 拒绝（`invalidArgument`）——令牌列 ✓ 的
/// wire 事实，规格不携带状态码。
#[tokio::test]
async fn connections_wire_answers_201_for_create_and_400_without_a_token() {
    let binding = connections_harness();
    binding.seed(Vec::new()).await.expect("seed");

    let mut body = serde_json::to_value(connections_contract::contract_draft())
        .expect("the draft serializes");
    body["submissionToken"] = Value::Null; // 缺令牌：presentation_of 拒绝
    let (status, value) = binding
        .dispatch(
            Request::post("/api/v1/connections")
                .header("content-type", "application/json")
                .extension(authenticated_session())
                .body(Body::from(
                    serde_json::to_vec(&body).expect("the request body serializes"),
                ))
                .expect("request body"),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "no token, no受理");
    assert_eq!(value["code"], "invalidArgument");

    let token = binding
        .issue_create_profile_token()
        .await
        .expect("a createProfile token issues");
    let mut body = serde_json::to_value(connections_contract::contract_draft())
        .expect("the draft serializes");
    body["submissionToken"] = Value::String(token.idempotency_key.as_str().to_owned());
    let (status, _) = binding
        .dispatch(
            Request::post("/api/v1/connections")
                .header("content-type", "application/json")
                .extension(authenticated_session())
                .body(Body::from(
                    serde_json::to_vec(&body).expect("the request body serializes"),
                ))
                .expect("request body"),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "the create is answered 201");

    let (status, value) = binding
        .dispatch(
            Request::get("/api/v1/connections")
                .extension(authenticated_session())
                .body(Body::empty())
                .expect("request body is empty"),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "the list is answered 200");
    let views: Vec<ProfileView> =
        serde_json::from_value(value).expect("the answer is a ProfileView array");
    assert_eq!(views.len(), 1, "the created profile is listed");
}

/// 错误 wire 形态 pin：CAS 冲突是 **409**，wire 体是 **JSON 对象**
/// `{code, message}`（IPC 腿同码不同句——形态差异钉在各自装配层）。
#[tokio::test]
async fn connections_cas_conflict_on_the_wire_is_a_409_with_the_contract_code() {
    let binding = connections_harness();
    binding.seed(Vec::new()).await.expect("seed");
    let token = binding
        .issue_create_profile_token()
        .await
        .expect("a createProfile token issues");
    let created = binding
        .create(connections_contract::contract_draft(), token)
        .await
        .expect("a valid draft creates");
    let base = created.config_revision.get();

    let mut patch = ProfilePatch::default();
    patch.name = Some("Renamed".to_owned());
    let ok_body = serde_json::json!({
        "expectedRevision": base.to_string(),
        "patch": patch,
        "submissionToken": binding.config_write_token().await.idempotency_key.as_str(),
    });
    let (status, _) = binding
        .dispatch(
            Request::patch(format!("/api/v1/connections/{}", created.connection_id.as_str()))
                .header("content-type", "application/json")
                .extension(authenticated_session())
                .body(Body::from(
                    serde_json::to_vec(&ok_body).expect("the request body serializes"),
                ))
                .expect("request body"),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "the current revision updates");

    let stale_body = serde_json::json!({
        "expectedRevision": base.to_string(),
        "patch": patch,
        "submissionToken": binding.config_write_token().await.idempotency_key.as_str(),
    });
    let (status, value) = binding
        .dispatch(
            Request::patch(format!("/api/v1/connections/{}", created.connection_id.as_str()))
                .header("content-type", "application/json")
                .extension(authenticated_session())
                .body(Body::from(
                    serde_json::to_vec(&stale_body).expect("the request body serializes"),
                ))
                .expect("request body"),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "the stale revision is a 409");
    assert_eq!(value["code"], "configRevisionMismatch");
    assert!(
        value["message"]
            .as_str()
            .unwrap_or_default()
            .contains("changed concurrently"),
        "the wire sentence carries the re-read guidance: {}",
        value["message"]
    );
}

// ── sessions 域装配层：用例面注入 + 端点形态（§6.4）──────────────────────

/// 会话用例面从**共享规格夹具**装配：真实 `RuntimeConnectionUseCases`
/// （与桌面 IPC 腿同一用例实现）+ 宽松策略假体 + 物理后端假体；目录是
/// server 形态的真实现（`ServerSessionDirectory`）。
fn sessions_contract_service()
-> (
    Arc<dyn ConnectionUseCases>,
    Arc<dyn SessionDirectory>,
    Arc<MemoryProfileStore>,
) {
    let store = Arc::new(MemoryProfileStore::default());
    // 目录与用例**同一实例**（生产装配纪律的镜像：可见性闸查的就是用例
    // open 时登记的那份归属事实）。
    let directory = Arc::new(InMemorySessionDirectory::new());
    let use_cases = Arc::new(RuntimeConnectionUseCases::new_with_parts(
        store.clone(),
        Arc::new(ContractPolicy),
        Arc::new(ContractSessionBackend::default()),
        Arc::new(
            datazen::execution::application::orchestration::SystemRuntimeClock(
                std::time::Instant::now(),
            ),
        ),
        datazen::execution::application::orchestration::RuntimeResultSink::default(),
        directory.clone(),
    ));
    (use_cases, directory, store)
}

/// sessions 域的装配状态（与真实 `domains::SessionsState` 同形）。
#[derive(Clone)]
struct SessionsState {
    sessions: Arc<dyn ConnectionUseCases>,
    directory: Arc<dyn SessionDirectory>,
}

/// 授权可见性闸（真实域文件的镜像）：越组织/未知 `{id}` → 404 固定文案，
/// 先于一切会话存活语义；命中时返回目录句柄（epoch 以目录当前条目补全）。
/// close 专用闸（闭容忍，真实域文件 ensure_owner 的镜像）：
/// `lookup_owner` 不筛 routable——墓碑回放可查已关闭会话的归属事实。
async fn ensure_owner(
    state: &SessionsState,
    ctx: &RequestContext,
    db_session_id: &str,
) -> Result<SessionHandle, Response> {
    let owner = match state
        .directory
        .lookup_owner(datazen::execution::DbSessionId::new(db_session_id.to_string()))
        .await
    {
        Ok(Some(owner)) => owner,
        Ok(None) => return Err(not_visible()),
        Err(source) => {
            return Err(error_response(ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                format!("session directory unavailable: {source}"),
            )))
        }
    };
    if owner.organization_id != ctx.organization_id {
        return Err(not_visible());
    }
    Ok(SessionHandle {
        db_session_id: datazen::execution::DbSessionId::new(db_session_id.to_string()),
        runtime_epoch: owner.runtime_epoch.clone(),
    })
}

async fn ensure_visible(
    state: &SessionsState,
    ctx: &RequestContext,
    db_session_id: &str,
) -> Result<SessionHandle, Response> {
    let owner = match state
        .directory
        .lookup(datazen::execution::DbSessionId::new(db_session_id.to_string()))
        .await
    {
        Ok(Some(owner)) => owner,
        Ok(None) => return Err(not_visible()),
        Err(source) => {
            return Err(error_response(ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                format!("session directory unavailable: {source}"),
            )));
        }
    };
    if owner.organization_id != ctx.organization_id {
        return Err(not_visible());
    }
    Ok(owner.to_handle())
}

/// 体 handle 与路由 `{id}` 的一致性（真实域文件的镜像）。
fn ensure_handle_matches_path(handle: &SessionHandle, db_session_id: &str) -> Result<(), Response> {
    if handle.db_session_id.as_str() != db_session_id {
        return Err(invalid_argument(
            "handle.dbSessionId does not match the addressed session.",
        ));
    }
    Ok(())
}

/// 令牌在场性 + `idempotencyKey` 绑定核对（真实域文件的镜像；签名与
/// operation 绑定由用例层核验）。
fn ensure_submission_token(body: &Value) -> Result<(), Response> {
    let Some(token) = body.get("submissionToken") else {
        return Err(invalid_argument("submissionToken is required."));
    };
    let claimed = token.get("idempotencyKey").and_then(Value::as_str);
    let bound = body.get("idempotencyKey").and_then(Value::as_str);
    if let (Some(claimed), Some(bound)) = (claimed, bound) {
        if claimed != bound {
            return Err(invalid_argument(
                "submissionToken does not bind this request's idempotencyKey.",
            ));
        }
    }
    Ok(())
}

async fn sessions_open_handler(
    State(state): State<SessionsState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    let request: OpenSessionRequest = match serde_json::from_value(body) {
        Ok(request) => request,
        Err(source) => return invalid_argument(format!("openSession: {source}")),
    };
    if let Err(error) = request.validate() {
        return error_response(error);
    }
    match state.sessions.open_session(&identity.ctx, request).await {
        Ok(receipt) => (StatusCode::CREATED, Json(receipt)).into_response(),
        Err(error) => error_response(error),
    }
}

async fn sessions_get_handler(
    State(state): State<SessionsState>,
    identity: SessionIdentity,
    Path(db_session_id): Path<String>,
) -> Response {
    let handle = match ensure_visible(&state, &identity.ctx, &db_session_id).await {
        Ok(handle) => handle,
        Err(response) => return response,
    };
    match state.sessions.get_session(&identity.ctx, handle).await {
        Ok(view) => Json(view).into_response(),
        Err(error) => error_response(error),
    }
}

async fn sessions_context_handler(
    State(state): State<SessionsState>,
    identity: SessionIdentity,
    Path(db_session_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    if let Err(response) = ensure_visible(&state, &identity.ctx, &db_session_id).await {
        return response;
    }
    let request: SetSessionContextRequest = match serde_json::from_value(body) {
        Ok(request) => request,
        Err(source) => return invalid_argument(format!("setSessionContext: {source}")),
    };
    if let Err(response) = ensure_handle_matches_path(&request.handle, &db_session_id) {
        return response;
    }
    if let Err(error) = request.validate() {
        return error_response(error);
    }
    match state.sessions.set_session_context(&identity.ctx, request).await {
        Ok(receipt) => Json(receipt).into_response(),
        Err(error) => error_response(error),
    }
}

async fn sessions_attach_handler(
    State(state): State<SessionsState>,
    identity: SessionIdentity,
    Path(db_session_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    sessions_attach_detach(state, identity, db_session_id, body, true).await
}

async fn sessions_detach_handler(
    State(state): State<SessionsState>,
    identity: SessionIdentity,
    Path(db_session_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    sessions_attach_detach(state, identity, db_session_id, body, false).await
}

/// attach/detach 共享路径（§6.4 的两条路由同形挂载；场景随后续登记）。
async fn sessions_attach_detach(
    state: SessionsState,
    identity: SessionIdentity,
    db_session_id: String,
    body: Value,
    attach: bool,
) -> Response {
    if let Err(response) = ensure_visible(&state, &identity.ctx, &db_session_id).await {
        return response;
    }
    let request: AttachmentRequest = match serde_json::from_value(body) {
        Ok(request) => request,
        Err(source) => return invalid_argument(format!("attachment: {source}")),
    };
    if let Err(response) = ensure_handle_matches_path(&request.handle, &db_session_id) {
        return response;
    }
    if let Err(error) = request.validate() {
        return error_response(error);
    }
    let outcome = if attach {
        state.sessions.attach_session(&identity.ctx, request).await
    } else {
        state.sessions.detach_session(&identity.ctx, request).await
    };
    match outcome {
        Ok(view) => Json(view).into_response(),
        Err(error) => error_response(error),
    }
}

async fn sessions_close_handler(
    State(state): State<SessionsState>,
    identity: SessionIdentity,
    Path(db_session_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    // close 走闭容忍闸（lookup_owner）：幂等墓碑回放需要已关闭会话的归属
    // 事实（与真实域文件的 ensure_owner 同口径，§7.5/CM-05）。
    if let Err(response) = ensure_owner(&state, &identity.ctx, &db_session_id).await {
        return response;
    }
    let request: CloseSessionRequest = match serde_json::from_value(body) {
        Ok(request) => request,
        Err(source) => return invalid_argument(format!("closeSession: {source}")),
    };
    if let Err(response) = ensure_handle_matches_path(&request.handle, &db_session_id) {
        return response;
    }
    if let Err(error) = request.validate() {
        return error_response(error);
    }
    match state.sessions.close_session(&identity.ctx, request).await {
        Ok(receipt) => Json(receipt).into_response(),
        Err(error) => error_response(error),
    }
}

fn sessions_contract_router(state: SessionsState) -> Router {
    Router::new()
        .route("/api/v1/sessions", post(sessions_open_handler))
        .route("/api/v1/sessions/{id}", get(sessions_get_handler))
        .route(
            "/api/v1/sessions/{id}/context",
            put(sessions_context_handler),
        )
        .route(
            "/api/v1/sessions/{id}/attach",
            post(sessions_attach_handler),
        )
        .route(
            "/api/v1/sessions/{id}/detach",
            post(sessions_detach_handler),
        )
        .route("/api/v1/sessions/{id}/close", post(sessions_close_handler))
        .with_state(state)
}

// ── sessions binding：HTTP 腿对共享规格端口的实现 ─────────────────────────

struct SessionsHttpBinding {
    app: Router,
    sessions: Arc<dyn ConnectionUseCases>,
    directory: Arc<dyn SessionDirectory>,
    store: Arc<MemoryProfileStore>,
    ctx: RequestContext,
}

impl SessionsHttpBinding {
    /// 装配裁决模拟：生产装配要求 `SessionsState.directory` 与用例实现持有
    /// **同一**目录实例（真实域文件模块头的接线登记）；本腿的闸目录是独立
    /// 实例，binding 在受理动作之后登记目录条目——闸的归属事实由此与用例
    /// 侧事实保持同源。

    async fn dispatch(&self, request: Request<Body>) -> (StatusCode, Value) {
        let response = self
            .app
            .clone()
            .oneshot(request)
            .await
            .expect("oneshot dispatch");
        wire(response).await
    }
}

#[async_trait(?Send)]
impl SessionsContractBinding for SessionsHttpBinding {
    async fn seed_profile(&self, record: ProfileRecord) -> Result<(), String> {
        self.store.seed(vec![record]).await;
        Ok(())
    }

    async fn client_owner(&self) -> OwnerRef {
        OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new("http"),
            purpose: "contract".to_owned(),
        }
    }

    async fn issue_session_token(
        &self,
        operation: IdempotentOperation,
        handle: Option<&SessionHandle>,
    ) -> ContractResult<SubmissionToken> {
        // 签发面：openSession/setSessionContext 的令牌由用例实例自己核验
        // （真实装配走 handler_issue_token + RuntimeConnectionUseCases 同一
        // 用例实例；本腿直用该实例的签发半边）。
        self.sessions
            .issue_submission_token(&self.ctx, operation, handle)
            .await
            .map_err(sessions_contract::application_failure)
    }

    async fn open(&self, request: OpenSessionRequest) -> ContractResult<OpenSessionReceipt> {
        let mut body = serde_json::to_value(&request).expect("the request serializes");
        body["submissionToken"] = serde_json::json!({
            "idempotencyKey": request.idempotency_key.as_str(),
            "expiresAt": DISTANT_FUTURE,
        });
        let (status, value) = self
            .dispatch(
                Request::post("/api/v1/sessions")
                    .header("content-type", "application/json")
                    .extension(authenticated_session())
                    .body(Body::from(
                        serde_json::to_vec(&body).expect("the request body serializes"),
                    ))
                    .expect("request body"),
            )
            .await;
        let receipt: OpenSessionReceipt = outcome(status, value)?;
        Ok(receipt)
    }

    async fn get(&self, handle: SessionHandle) -> ContractResult<SessionView> {
        let (status, value) = self
            .dispatch(
                Request::get(format!(
                    "/api/v1/sessions/{}",
                    handle.db_session_id.as_str()
                ))
                .extension(authenticated_session())
                .body(Body::empty())
                .expect("request body is empty"),
            )
            .await;
        outcome(status, value)
    }

    async fn set_context(
        &self,
        request: SetSessionContextRequest,
    ) -> ContractResult<ContextChangeReceipt> {
        let mut body = serde_json::to_value(&request).expect("the request serializes");
        body["submissionToken"] = serde_json::json!({
            "idempotencyKey": request.idempotency_key.as_str(),
            "expiresAt": DISTANT_FUTURE,
        });
        let (status, value) = self
            .dispatch(
                Request::put(format!(
                    "/api/v1/sessions/{}/context",
                    request.handle.db_session_id.as_str()
                ))
                .header("content-type", "application/json")
                .extension(authenticated_session())
                .body(Body::from(
                    serde_json::to_vec(&body).expect("the request body serializes"),
                ))
                .expect("request body"),
            )
            .await;
        let receipt: ContextChangeReceipt = outcome(status, value)?;
        Ok(receipt)
    }

    async fn close(&self, request: CloseSessionRequest) -> ContractResult<CloseReceipt> {
        let body = serde_json::to_value(&request).expect("the request serializes");
        let (status, value) = self
            .dispatch(
                Request::post(format!(
                    "/api/v1/sessions/{}/close",
                    request.handle.db_session_id.as_str()
                ))
                .header("content-type", "application/json")
                .extension(authenticated_session())
                .body(Body::from(
                    serde_json::to_vec(&body).expect("the request body serializes"),
                ))
                .expect("request body"),
            )
            .await;
        // 闸目录不在 close 后释放条目：二次 close 还要过闸（§7.5 幂等的
        // wire 事实），闸目录是本腿可见性的事实源。
        outcome(status, value)
    }
}

fn sessions_harness() -> SessionsHttpBinding {
    let (sessions, directory, store) = sessions_contract_service();
    SessionsHttpBinding {
        app: sessions_contract_router(SessionsState {
            sessions: sessions.clone(),
            directory: directory.clone(),
        }),
        sessions,
        directory,
        store,
        ctx: http_context(),
    }
}

// ── sessions 共享规格：四个 scenario，三腿逐字同名 ────────────────────────

#[tokio::test]
async fn opening_a_session_answers_a_receipt_that_get_session_sees() {
    sessions_contract::opening_a_session_answers_a_receipt_that_get_session_sees(&sessions_harness())
        .await;
}

#[tokio::test]
async fn a_context_change_answers_a_receipt_the_new_session_answers_for() {
    sessions_contract::a_context_change_answers_a_receipt_the_new_session_answers_for(
        &sessions_harness(),
    )
    .await;
}

#[tokio::test]
async fn closing_a_session_answers_the_same_receipt_twice() {
    sessions_contract::closing_a_session_answers_the_same_receipt_twice(&sessions_harness()).await;
}

#[tokio::test]
async fn a_stale_expected_context_revision_is_refused_as_context_conflict() {
    sessions_contract::a_stale_expected_context_revision_is_refused_as_context_conflict(
        &sessions_harness(),
    )
    .await;
}

// ── sessions 装配层断言（形态差异唯一允许出现的位置）──────────────────────

/// 端点形态 pin：open 回答 **201** + 回执（`attachmentToken` 在场）；GET
/// 回答 **200** 视图。这是传输事实，规格不携带状态码。
#[tokio::test]
async fn sessions_wire_answers_201_for_open_and_200_for_get() {
    let binding = sessions_harness();
    binding
        .seed_profile(sessions_contract::contract_profile_record())
        .await
        .expect("seed the target profile");
    let token = binding
        .issue_session_token(IdempotentOperation::OpenSession, None)
        .await
        .expect("an openSession token issues");
    let request = OpenSessionRequest::new(
        sessions_contract::contract_target(),
        binding.client_owner().await,
        token.idempotency_key,
    );
    let mut body = serde_json::to_value(&request).expect("the request serializes");
    body["submissionToken"] = serde_json::json!({
        "idempotencyKey": request.idempotency_key.as_str(),
        "expiresAt": DISTANT_FUTURE,
    });
    let (status, value) = binding
        .dispatch(
            Request::post("/api/v1/sessions")
                .header("content-type", "application/json")
                .extension(authenticated_session())
                .body(Body::from(
                    serde_json::to_vec(&body).expect("the request body serializes"),
                ))
                .expect("request body"),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "open is answered 201");
    assert!(
        !value["attachmentToken"].as_str().unwrap_or_default().is_empty(),
        "the receipt carries the attachment token on the wire"
    );

    let db_session_id = value["session"]["handle"]["dbSessionId"]
        .as_str()
        .expect("the receipt carries the handle")
        .to_owned();
    let (status, _) = binding
        .dispatch(
            Request::get(format!("/api/v1/sessions/{db_session_id}"))
                .extension(authenticated_session())
                .body(Body::empty())
                .expect("request body is empty"),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "get is answered 200");
}

/// 可见性闸 pin：越组织/未知 `{id}` 是 **404** 固定文案（防 ID 枚举，与
/// L6 同一条；不是 403，也不是 sessionNotFound 的 409）。
#[tokio::test]
async fn sessions_unknown_id_on_the_wire_is_a_404_with_the_not_visible_text() {
    let binding = sessions_harness();
    let (status, value) = binding
        .dispatch(
            Request::get("/api/v1/sessions/no-such-session")
                .extension(authenticated_session())
                .body(Body::empty())
                .expect("request body is empty"),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(value["code"], "notFound");
    assert_eq!(value["message"], NOT_VISIBLE_MESSAGE);
}
