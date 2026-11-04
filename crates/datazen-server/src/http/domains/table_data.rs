//! tableData 域路由（契约 §6.7 读与预览 / §6.8 变更两段式 / §6.9 命名视图
//! 的 HTTP 薄绑定）。tableData 共 **14 方法全部上 HTTP**；本文件是它们的
//! 唯一落点。
//!
//! ## 14 方法 × 状态表（本刀登记）
//!
//! | 方法 | 路由 | 状态 | 说明 |
//! | --- | --- | --- | --- |
//! | `listNamedViews` | `GET /api/v1/named-views` | ✅ 真实现 | 经 `NamedViewService`，`ServerNamedViewStore` 落 v9 表 |
//! | `saveNamedView` | `PUT /api/v1/named-views` | ✅ 真实现 | 空 id 铸造 → 201，已知 id 更新 → 200（§6.9） |
//! | `deleteNamedView` | `DELETE /api/v1/named-views/{id}` | ✅ 真实现 | 未知 id 幂等无操作，恒 200 返剩余集合（§6.9） |
//! | `getTableData` | `POST /api/v1/table-data/reads` | ⛔ fail-closed | 用例（`table_browsing::api::read_table_page`）签名要求 `&dyn DatabaseDriver` + `TargetCommandRunner`（提交/轮询/产物收集的执行半边）；v1 服务端驱动注册表为空（bootstrap `DriverRegistry::with_factories([])`）且 server 尚无 `TargetCommandRunner` 实现，无驱动可注入 → 501 |
//! | `previewTableReadSql` | `POST /api/v1/table-data/read-sql-previews` | ⛔ fail-closed | 同上（`preview_table_read` 同签名；驱动方言 + 运行器取 schema）→ 501 |
//! | `estimateTableRows` | `POST /api/v1/table-data/row-count-estimates` | ⛔ fail-closed | 行数估算走读用例的 count 半边（count strategy / 计数回退），同样需要驱动与运行器 → 501 |
//! | `resolveForeignKeyTargets` | `POST /api/v1/table-data/foreign-key-targets` | ⛔ fail-closed | `services::fk_navigation` 要求会话绑定的 `ForeignKeyNavigationSource`（会话目录 + 驱动元数据往返）；v1 无驱动无会话执行面 → 501 |
//! | `listInboundForeignKeys` | `POST /api/v1/table-data/inbound-foreign-keys` | ⛔ fail-closed | `services::referenced_by` 同上（`get_table_schema`/`get_tables` 驱动往返）→ 501 |
//! | `searchTableValues` | `POST /api/v1/table-data/value-searches` | ⛔ fail-closed | `services::value_search` 在 `ConnectionHandle` 上跑驱动查询；v1 无驱动 → 501 |
//! | `resolveResultOrigin` | `POST /api/v1/table-data/result-origins` | ⛔ fail-closed | 结果来源重推导（result_edit 的 fresh origin 报告）需要驱动 schema 往返 → 501 |
//! | `previewPendingChanges` | `POST /api/v1/table-data/change-plan-previews` | ⛔ fail-closed | `table_editing::preview_changes` 要求 `&dyn DatabaseDriver` + `TableColumnSource`（宿主目录端口：列与主键的真实答案）；v1 无目录来源 → 501 |
//! | `commitPendingChanges` | `POST /api/v1/table-data/change-commits` | ⛔ fail-closed | `commit_changes` 是同步回执写路径（指纹确认的计划经 runtime + 产物半边执行）；写半边未装配且无驱动，fail-closed 优先 → 501 |
//! | `commitQueryResultCell` | `POST /api/v1/table-data/result-cell-commits` | ⛔ fail-closed | 结果单元格写 = 来源重推导（驱动往返）+ `execute_plan`（runtime）；无驱动 → 501 |
//! | `previewResultCellChange` | `POST /api/v1/table-data/result-cell-change-previews` | ⛔ fail-closed | `build_result_cell_change_plan` 要求 `&dyn DatabaseDriver` 与 server 刚算出的 editability 判定 → 501 |
//!
//! fail-closed 的形状沿 `ai_host::execute_target` 先例：路由照常挂载（§6.7
//! / §6.8 在账），handler 显式回答 **501 `capabilityUnsupported`**（
//! not-implemented 类，`http::response` 状态码表的 with_status 覆盖），文案
//! 携带逐方法的不可服务原因——**绝不返回看似成功的结果**。11 条 ⛔ 路由
//! 是后续切片的单点改动处：`TargetCommandRunner` 的 server 实现 + 驱动
//! 注册表点开后，逐条把 handler 换成真用例接线（用例函数已就绪：
//! `table_browsing::api` / `table_editing` / `services::{fk_navigation,
//! value_search,referenced_by}`，宿主侧注入 `Arc<dyn ResourceExecutionHost>`
//! 机器与 `TargetCommandRunner` 适配即可）。
//!
//! ## 命名视图的 wire 形状（与 WR-C3 契约腿逐字一致）
//!
//! `tests/use_case_contracts.rs` 的 named_views HTTP 腿（§6.9 三端点）钉住
//! 的形态，本文件逐字落地：路径 / `{ view }` 体 / 空 id 铸造 → 201、更新 →
//! 200 / DELETE 恒 200 返剩余集合 / 错误体是 `{code, message}` JSON 对象，
//! application 契约码进 `code`（`grid.view.nameExists` → 409、
//! `grid.view.invalid` → 400、存储故障 → 500 + `null` 码——`NamedViewError`
//! 的码空间是封闭的 45 码之外的应用契约码，前端按它分支，不折进
//! `ApiError`）。scope 查询参数的部分省略语义沿用契约腿的装配裁决并在此
//! 定为权威：**`connectionId` 与 `table` 同时在场才成 scope 过滤，否则列
//! 全部**（§6.9「全部可选，缺省列全部」；单独给 connectionId 而无 table
//! 的过滤集恒空，故按腿的口径不构成过滤）。
//!
//! ## 组织/主体边界（INV-01）
//!
//! `NamedViewRepository` 端口无 scope 参数：handler 按 `SessionIdentity`
//! 的组织 + 主体逐请求装配「每 (organization_id, principal_id) 一个实例」
//! 的 `ServerNamedViewStore`（CM-05：读越界空集合、跨边界 id 写拒绝；
//! scope 绑定持久 connectionId，见 `storage::named_views` 模块头）。
//!
//! ## 主线落点（本刀只交付文件，不接线）
//!
//! 1. `crates/datazen-server/src/http/domains/mod.rs`：`pub mod table_data;`
//! 2. `crates/datazen-server/src/storage/mod.rs`：`pub mod named_views;`
//!    （+ `pub use named_views::{ServerNamedViewStore, UuidNamedViewIdMinter};`）
//! 3. `crates/datazen-server/Cargo.toml`：`datazen` features 追加 `"named_views"`（本文件
//!    与 storage 适配器引用 `datazen::named_views`；feature 链 = 既有依赖
//!    的超集，无新拉包）。
//! 4. `crates/datazen-server/src/bootstrap.rs`：`domain_routers` vec 追加
//!    `crate::http::domains::table_data::table_data_router(
//!       crate::http::domains::table_data::TableDataState { store: store.clone() })`
//!    （settings_router 同槽；认证链 L2–L6 与 CSRF 覆盖由 merge 循环保证）。

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use datazen::named_views::{NamedView, NamedViewError, NamedViewScope, NamedViewService};
use datazen::shared::error::{ApiError, ApiErrorCode};

use crate::http::response::ApiErrorResponse;
use crate::http::router::{invalid_argument, SessionIdentity};
use crate::storage::named_views::{ServerNamedViewStore, UuidNamedViewIdMinter};
use crate::storage::AdminStore;

/// tableData 域的装配状态：管理库句柄（v9 `named_views` 表；调用方负责
/// 先 `migrate`）+ 每请求按会话组织与主体装配 service。
#[derive(Clone)]
pub struct TableDataState {
    pub store: Arc<AdminStore>,
}

impl TableDataState {
    /// 按会话事实装配 `NamedViewService`（per (org, principal) 实例口径；
    /// id 铸造 uuid v4，见 `storage::named_views`）。
    fn named_view_service_for(
        &self,
        organization_id: &str,
        principal_id: &str,
    ) -> NamedViewService {
        NamedViewService::new(
            Arc::new(ServerNamedViewStore::scoped(
                organization_id,
                principal_id,
                self.store.clone(),
            )),
            Arc::new(UuidNamedViewIdMinter),
        )
    }
}

/// tableData 域 Router 片段：命名视图三路由（§6.9，✅）+ tableData 读/
/// 预览/变更十一路由（§6.7/§6.8，⛔ fail-closed 登记）。主线把它 merge
/// 进 `build_router_with_auth` 的认证链（L2–L6 之内；PUT/DELETE/POST 是
/// cookie 认证的 unsafe 方法，L4 CSRF 适用）。
pub fn table_data_router(state: TableDataState) -> Router {
    Router::new()
        // §6.9 命名视图（✅ 真实现）。
        .route(
            "/api/v1/named-views",
            get(handler_list_named_views).put(handler_save_named_view),
        )
        .route(
            "/api/v1/named-views/{id}",
            axum::routing::delete(handler_delete_named_view),
        )
        // §6.7 读与预览（⛔ fail-closed 登记，理由见模块头状态表）。
        .route("/api/v1/table-data/reads", post(handler_get_table_data))
        .route(
            "/api/v1/table-data/read-sql-previews",
            post(handler_preview_table_read_sql),
        )
        .route(
            "/api/v1/table-data/row-count-estimates",
            post(handler_estimate_table_rows),
        )
        .route(
            "/api/v1/table-data/foreign-key-targets",
            post(handler_resolve_foreign_key_targets),
        )
        .route(
            "/api/v1/table-data/inbound-foreign-keys",
            post(handler_list_inbound_foreign_keys),
        )
        .route(
            "/api/v1/table-data/value-searches",
            post(handler_search_table_values),
        )
        .route(
            "/api/v1/table-data/result-origins",
            post(handler_resolve_result_origin),
        )
        // §6.8 变更两段式（⛔ fail-closed 登记；两段式的 CSRF 令牌列随
        // 真接线生效——L4 已对全部 POST 适用）。
        .route(
            "/api/v1/table-data/change-plan-previews",
            post(handler_preview_pending_changes),
        )
        .route(
            "/api/v1/table-data/change-commits",
            post(handler_commit_pending_changes),
        )
        .route(
            "/api/v1/table-data/result-cell-commits",
            post(handler_commit_query_result_cell),
        )
        .route(
            "/api/v1/table-data/result-cell-change-previews",
            post(handler_preview_result_cell_change),
        )
        .with_state(state)
}

// ── 命名视图（§6.9，✅）────────────────────────────────────────────────────

/// `listNamedViews`（GET /api/v1/named-views）→ 200 `NamedView[]`。
///
/// scope 四元组进查询参数；`connectionId` 与 `table` 同时在场才成 scope
/// 过滤（权威裁决，见模块头），否则列全部。
async fn handler_list_named_views(
    State(state): State<TableDataState>,
    identity: SessionIdentity,
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
    let service = state.named_view_service_for(
        identity.ctx.organization_id.as_str(),
        identity.ctx.principal_id.as_str(),
    );
    match service.list(scope.as_ref()).await {
        Ok(views) => (StatusCode::OK, Json(views)).into_response(),
        Err(error) => named_view_error_response(error),
    }
}

/// `PUT /api/v1/named-views` 的请求体（§6.9：`{ view }`）。
#[derive(Deserialize)]
struct SaveNamedViewBody {
    view: NamedView,
}

/// `saveNamedView`（PUT /api/v1/named-views，体 `{ view }`）→ 200 更新 /
/// 201 铸造（空 `id` 由后端铸造是 application 语义，装配只翻状态码）。
/// 响应即存储事实（normalize/时间戳后的记录）。
async fn handler_save_named_view(
    State(state): State<TableDataState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let body: SaveNamedViewBody = match serde_json::from_value(body) {
        Ok(body) => body,
        Err(source) => return invalid_argument(format!("view: {source}")),
    };
    let minted = body.view.id.trim().is_empty();
    let service = state.named_view_service_for(
        identity.ctx.organization_id.as_str(),
        identity.ctx.principal_id.as_str(),
    );
    match service.save(body.view).await {
        Ok(saved) => {
            if minted {
                (StatusCode::CREATED, Json(saved)).into_response()
            } else {
                (StatusCode::OK, Json(saved)).into_response()
            }
        }
        Err(error) => named_view_error_response(error),
    }
}

/// `deleteNamedView`（DELETE /api/v1/named-views/{id}）→ 200 剩余集合。
///
/// v1 面唯一删除路由（§6.9 的显式例外）；未知 id 无操作（application
/// 幂等语义），答案恒为剩余集合。
async fn handler_delete_named_view(
    State(state): State<TableDataState>,
    identity: SessionIdentity,
    Path(id): Path<String>,
) -> Response {
    let service = state.named_view_service_for(
        identity.ctx.organization_id.as_str(),
        identity.ctx.principal_id.as_str(),
    );
    match service.delete(&id).await {
        Ok(remaining) => (StatusCode::OK, Json(remaining)).into_response(),
        Err(error) => named_view_error_response(error),
    }
}

/// 本域的 `NamedViewError` → wire 映射：与 WR-C3 契约腿（`tests/
/// use_case_contracts.rs` 的 `application_error_response`）逐字一致——
/// application 契约码进 `code` 字段、application 文本进 `message` 字段，
/// 状态码按失败族（409/400/500）。见模块头「wire 形状」节。
fn named_view_error_response(error: NamedViewError) -> Response {
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

// ── tableData 读/预览/变更（§6.7/§6.8，⛔ fail-closed 登记）────────────────
//
// 十一条路由逐一显式登记（模块头状态表），文案携带逐方法原因；不解读
// 请求体、不产生任何副作用。`SessionIdentity` 提取即认证链声明：未认证
// 请求在 L2 已 401，不进入本层。

/// 501 `capabilityUnsupported`（not-implemented 类；`ai_host::execute_target`
/// 先例同款——`CapabilityUnsupported` 默认 405，501 类由调用点 with_status
/// 显式覆盖）。
fn fail_closed(method: &'static str, reason: &'static str) -> Response {
    let error = ApiError::new(
        ApiErrorCode::CapabilityUnsupported,
        format!(
            "{method} is not served by this host yet: {reason} \
             No statement ran, no plan was built and nothing was written."
        ),
    );
    ApiErrorResponse::new(error)
        .with_status(StatusCode::NOT_IMPLEMENTED)
        .into_response()
}

const REASON_READ: &str = "the read use case needs a resolved driver and a \
TargetCommandRunner execution half (submission, polling and artifact collection); \
the v1 driver registry is empty and the server runner port is not assembled yet.";
const REASON_FK_SOURCE: &str = "the foreign-key services need a session-bound \
ForeignKeyNavigationSource (session directory plus driver metadata round trips); \
the v1 driver registry is empty and no session execution face is assembled.";
const REASON_DRIVER_QUERY: &str = "the service runs driver queries at a connection \
handle; the v1 driver registry is empty.";
const REASON_ORIGIN: &str = "result-origin re-derivation needs a fresh driver schema \
round trip; the v1 driver registry is empty.";
const REASON_PLAN: &str = "change planning needs a resolved driver plus a \
TableColumnSource catalog answer (real columns and primary key); the v1 driver \
registry is empty and no catalog source is assembled.";
const REASON_COMMIT: &str = "change commit executes a fingerprint-confirmed plan \
through the runtime (driver plus artifact half) and answers synchronously; the v1 \
driver registry is empty and the write half is not assembled.";

/// `getTableData`（POST /api/v1/table-data/reads）→ 501（⛔，模块头状态表）。
async fn handler_get_table_data(_identity: SessionIdentity) -> Response {
    fail_closed("getTableData", REASON_READ)
}

/// `previewTableReadSql`（POST /api/v1/table-data/read-sql-previews）→ 501。
async fn handler_preview_table_read_sql(_identity: SessionIdentity) -> Response {
    fail_closed("previewTableReadSql", REASON_READ)
}

/// `estimateTableRows`（POST /api/v1/table-data/row-count-estimates）→ 501。
async fn handler_estimate_table_rows(_identity: SessionIdentity) -> Response {
    fail_closed("estimateTableRows", REASON_READ)
}

/// `resolveForeignKeyTargets`（POST /api/v1/table-data/foreign-key-targets）
/// → 501。
async fn handler_resolve_foreign_key_targets(_identity: SessionIdentity) -> Response {
    fail_closed("resolveForeignKeyTargets", REASON_FK_SOURCE)
}

/// `listInboundForeignKeys`（POST /api/v1/table-data/inbound-foreign-keys）
/// → 501。
async fn handler_list_inbound_foreign_keys(_identity: SessionIdentity) -> Response {
    fail_closed("listInboundForeignKeys", REASON_FK_SOURCE)
}

/// `searchTableValues`（POST /api/v1/table-data/value-searches）→ 501。
async fn handler_search_table_values(_identity: SessionIdentity) -> Response {
    fail_closed("searchTableValues", REASON_DRIVER_QUERY)
}

/// `resolveResultOrigin`（POST /api/v1/table-data/result-origins）→ 501。
async fn handler_resolve_result_origin(_identity: SessionIdentity) -> Response {
    fail_closed("resolveResultOrigin", REASON_ORIGIN)
}

/// `previewPendingChanges`（POST /api/v1/table-data/change-plan-previews）
/// → 501。
async fn handler_preview_pending_changes(_identity: SessionIdentity) -> Response {
    fail_closed("previewPendingChanges", REASON_PLAN)
}

/// `commitPendingChanges`（POST /api/v1/table-data/change-commits）→ 501。
async fn handler_commit_pending_changes(_identity: SessionIdentity) -> Response {
    fail_closed("commitPendingChanges", REASON_COMMIT)
}

/// `commitQueryResultCell`（POST /api/v1/table-data/result-cell-commits）
/// → 501。
async fn handler_commit_query_result_cell(_identity: SessionIdentity) -> Response {
    fail_closed("commitQueryResultCell", REASON_COMMIT)
}

/// `previewResultCellChange`（POST
/// /api/v1/table-data/result-cell-change-previews）→ 501。
async fn handler_preview_result_cell_change(_identity: SessionIdentity) -> Response {
    fail_closed("previewResultCellChange", REASON_ORIGIN)
}
