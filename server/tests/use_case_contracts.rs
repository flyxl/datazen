//! WR-C3 HTTP 腿：named_views 用例契约（同用例、axum 绑定）。
//!
//! 测试内**自建最小 axum Router**——不改 `server/src/**`：只绑定 WR-C2
//! 冻结契约（http-api-contract §6.9）的三个端点形态，handler 调
//! **同一个** modules 用例（`datazen::named_views::NamedViewService`，
//! 装配层注入规格共享的夹具仓储），用 `tower::ServiceExt::oneshot`
//! 发请求跑共享规格（`datazen::use_case_contracts::named_views`）。
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
use axum::extract::{Path, Query, State};
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use datazen::named_views::{NamedView, NamedViewError, NamedViewScope, NamedViewService};
use datazen::use_case_contracts::named_views::{
    self as contract, ContractIdMinter, MemoryViewStore, NamedViewsContractBinding,
};
use datazen::use_case_contracts::ContractResult;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use tower::ServiceExt;

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
            datazen::use_case_contracts::ContractFailure {
                code: None,
                message: format!("wire body did not deserialize into the application DTO: {error}"),
            }
        });
    }
    Err(datazen::use_case_contracts::ContractFailure {
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
