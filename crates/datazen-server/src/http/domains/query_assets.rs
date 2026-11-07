//! 查询资产域路由（W8 刀3a；契约 §6.12 的 HTTP 薄绑定）——收藏与查询
//! 历史从 legacy IPC 命令轨收敛进 MethodMap 的第一刀。
//!
//! | 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
//! | --- | --- | --- | --- | --- |
//! | `listQueryFavorites` | `GET /api/v1/query-favorites?connectionId=` | — | 200 | — → `FavoriteQuery[]` |
//! | `createQueryFavorite` | `POST /api/v1/query-favorites` | ✓ | 201 | `FavoriteDraft & SubmitRequest` → `FavoriteQuery` |
//! | `deleteQueryFavorite` | `DELETE /api/v1/query-favorites/{id}` | ✓ | 204 | — |
//! | `getQueryHistoryPage` | `GET /api/v1/query-history` | — | 200 | 过滤平铺查询参数 → `QueryHistoryPage` |
//! | `deleteQueryHistory` | `DELETE /api/v1/query-history/{id}` | ✓ | 200 | — → `{ deleted }`（行数让调用方拒绝 no-op，桌面同语义） |
//! | `clearQueryHistory` | `DELETE /api/v1/query-history` | ✓ | 200 | — → `{ purged }` |
//! | `purgeHistory` | `POST /api/v1/query-history/purge` | ✓ | 200 | `{ scope, retainDays? }` → `{ purged }` |
//!
//! 域裁决（契约 §6.12 全文逐字对照）：
//!
//! - **归属 = per (organization, principal)**（v9 named_views 同款口径）：
//!   收藏与历史都是「主体自己的查询资产」，scoped store 按认证链事实
//!   （`SessionIdentity.ctx`）逐请求装配；越界读取天然落空、写只落本
//!   边界（CM-05）。请求体/查询参数**没有**归属字段——INV-01。
//! - **桌面专属两命令不进 HTTP 面**：`get_favorites_root` /
//!   `refresh_favorites` 是文件系统语义（收藏目录路径对接同步服务、
//!   重扫目录接收同步文件）——服务端收藏是库行，没有根目录也没有
//!   需要重扫的目录；WEB_CAPABILITIES 声明缺位，web 端如实报
//!   `CapabilityUnsupported`。
//! - **查询历史的写面不在本域**：落账点是 execution 域路由的受理后
//!   钩子（`execution::history_recorder`——SQL 命令终态后 best-effort
//!   追加，桌面 `record_sql_command_outcome` 的 transport 对偶；执行
//!   应答不被落账失败阻断）。本域只读/管理。
//! - **幂等**：收藏 DELETE 的未知 id = no-op（204；named_views 同款
//!   「未知删除是幂等」）；历史 DELETE 返回行数（桌面同语义：调用方
//!   据此拒绝无操作的成功）。
//! - **令牌**：写面（create/delete/clear/purge）带 `SubmitRequest`，
//!   handler 核在场性 + `idempotencyKey` 绑定（sessions 域同款；HMAC 轨
//!   签发——查询资产写面是配置类操作，不绑 runtime 会话）。
//! - **错误 wire**：存储故障 → 503 `serviceUnavailable`（settings 域
//!   同款映射）；体校验失败 → 400 `invalidArgument`。

use std::sync::Arc;

use axum::extract::{Path, Query as QueryParams, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use datazen::history::{FavoriteDraft, FavoriteQuery, HistoryOrder, HistoryScope};

use crate::http::router::{api_error_response, invalid_argument, SessionIdentity};
use crate::storage::query_assets::{
    HistoryPageQuery, HistoryStoreError, ServerQueryFavoritesStore, ServerQueryHistoryStore,
};
use crate::storage::AdminStore;

/// 查询资产域的装配状态（composition root 注入；无隐式单例）。scoped
/// store 在 handler 内按认证链事实装配（INV-01），state 只持库。
#[derive(Clone)]
pub struct QueryAssetsState {
    /// 管理库（收藏与历史同库；迁移由 bootstrap 负责）。
    pub store: Arc<AdminStore>,
}

/// 查询资产域 Router 片段。主线把它 merge 进 `build_router_with_auth`
/// 的认证链（L2–L6 之内）。
pub fn query_assets_router(state: QueryAssetsState) -> Router {
    Router::new()
        .route(
            "/api/v1/query-favorites",
            get(handler_list_favorites).post(handler_create_favorite),
        )
        .route(
            "/api/v1/query-favorites/{id}",
            delete(handler_delete_favorite),
        )
        .route(
            "/api/v1/query-history",
            get(handler_query_history_page).delete(handler_clear_history),
        )
        .route(
            "/api/v1/query-history/{id}",
            delete(handler_delete_history_entry),
        )
        .route("/api/v1/query-history/purge", post(handler_purge_history))
        .with_state(state)
}

// ── 共享判定 ────────────────────────────────────────────────────────────────

/// 令牌在场性 + `idempotencyKey` 绑定核对（sessions 域同款）。
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

/// 存储故障 → 503（settings 域同款映射；读面不吞错）。
fn store_unavailable(error: HistoryStoreError) -> Response {
    api_error_response(datazen::shared::error::ApiError::new(
        datazen::shared::error::ApiErrorCode::ServiceUnavailable,
        format!("query assets store: {error}"),
    ))
}

// ── favorites ───────────────────────────────────────────────────────────────

/// `listQueryFavorites`（GET /api/v1/query-favorites?connectionId=）→ 200
/// `FavoriteQuery[]`。
async fn handler_list_favorites(
    State(state): State<QueryAssetsState>,
    identity: SessionIdentity,
    QueryParams(params): QueryParams<FavoritesListParams>,
) -> Response {
    let store = ServerQueryFavoritesStore::for_identity(
        state.store,
        identity.ctx.organization_id.as_str(),
        identity.ctx.principal_id.as_str(),
    );
    match store.list(params.connection_id.as_deref()).await {
        Ok(favorites) => (StatusCode::OK, Json(favorites)).into_response(),
        Err(error) => store_unavailable(error),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FavoritesListParams {
    #[serde(default)]
    connection_id: Option<String>,
}

/// `createQueryFavorite`（POST /api/v1/query-favorites）→ 201
/// `FavoriteQuery`。id 服务端铸造；`created_at` 服务端时刻。
async fn handler_create_favorite(
    State(state): State<QueryAssetsState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    let draft: FavoriteDraft = match serde_json::from_value(body) {
        Ok(draft) => draft,
        Err(source) => return invalid_argument(format!("createQueryFavorite: {source}")),
    };
    if let Err(message) = draft.validate() {
        return invalid_argument(message);
    }
    let store = ServerQueryFavoritesStore::for_identity(
        state.store,
        identity.ctx.organization_id.as_str(),
        identity.ctx.principal_id.as_str(),
    );
    match store.add(draft).await {
        Ok(favorite) => (StatusCode::CREATED, Json(favorite)).into_response(),
        Err(error) => store_unavailable(error),
    }
}

/// `deleteQueryFavorite`（DELETE /api/v1/query-favorites/{id}）→ 204。
/// 未知 id = 幂等 no-op（同 204；named_views 的删除语义同款）。
async fn handler_delete_favorite(
    State(state): State<QueryAssetsState>,
    identity: SessionIdentity,
    Path(favorite_id): Path<String>,
) -> Response {
    let store = ServerQueryFavoritesStore::for_identity(
        state.store,
        identity.ctx.organization_id.as_str(),
        identity.ctx.principal_id.as_str(),
    );
    match store.delete(&favorite_id).await {
        // 行数不回 wire：跨边界/未知/命中三者的不可区分口径（防枚举）。
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => store_unavailable(error),
    }
}

// ── query history ───────────────────────────────────────────────────────────

/// `getQueryHistoryPage`（GET /api/v1/query-history）→ 200
/// `QueryHistoryPage`。过滤平铺为查询参数（契约 §6.12；概要 §6.3 的
/// listJobs 同款）。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryPageParams {
    #[serde(default = "default_history_limit")]
    limit: usize,
    #[serde(default)]
    connection_id: Option<String>,
    #[serde(default)]
    database: Option<String>,
    #[serde(default)]
    schema: Option<String>,
    #[serde(default)]
    search: Option<String>,
    #[serde(default)]
    since: Option<String>,
    #[serde(default)]
    until: Option<String>,
    #[serde(default)]
    order: Option<String>,
}

fn default_history_limit() -> usize {
    // 共享层 DEFAULT_HISTORY_PAGE_SIZE 的路由缺省（200；过滤在 limit 前
    // 施加，桌面 history_db 同缺省）。
    datazen::history::DEFAULT_HISTORY_PAGE_SIZE
}

async fn handler_query_history_page(
    State(state): State<QueryAssetsState>,
    identity: SessionIdentity,
    QueryParams(params): QueryParams<HistoryPageParams>,
) -> Response {
    let order = match params.order.as_deref() {
        None | Some("") => None,
        Some(raw) => match HistoryOrder::parse(raw) {
            Some(order) => Some(order),
            None => {
                return invalid_argument(format!(
                    "unknown order '{raw}' (expected recent | oldest | slowest)."
                ));
            }
        },
    };
    let query = HistoryPageQuery {
        limit: params.limit,
        connection_id: params.connection_id,
        database: params.database,
        schema: params.schema,
        search: params.search,
        since: params.since,
        until: params.until,
        order,
    };
    let store = ServerQueryHistoryStore::for_identity(
        state.store,
        identity.ctx.organization_id.as_str(),
        identity.ctx.principal_id.as_str(),
    );
    match store.page(query).await {
        Ok(page) => (StatusCode::OK, Json(page)).into_response(),
        Err(error) => store_unavailable(error),
    }
}

/// `deleteQueryHistory`（DELETE /api/v1/query-history/{id}）→ 200
/// `{ deleted }`。行数让调用方拒绝 no-op（桌面 `delete_query_history`
/// 同语义；`Ok(0)` = id 不在边界内或不存在）。
async fn handler_delete_history_entry(
    State(state): State<QueryAssetsState>,
    identity: SessionIdentity,
    Path(entry_id): Path<String>,
) -> Response {
    let store = ServerQueryHistoryStore::for_identity(
        state.store,
        identity.ctx.organization_id.as_str(),
        identity.ctx.principal_id.as_str(),
    );
    match store.delete(&entry_id).await {
        Ok(deleted) => (StatusCode::OK, Json(json!({ "deleted": deleted }))).into_response(),
        Err(error) => store_unavailable(error),
    }
}

/// `clearQueryHistory`（DELETE /api/v1/query-history）→ 200 `{ purged }`。
/// 经 purge 的 query 作用域完成（`HistoryService::clear_query_history`
/// 同款语义）。
async fn handler_clear_history(
    State(state): State<QueryAssetsState>,
    identity: SessionIdentity,
) -> Response {
    let store = ServerQueryHistoryStore::for_identity(
        state.store,
        identity.ctx.organization_id.as_str(),
        identity.ctx.principal_id.as_str(),
    );
    match store.purge(HistoryScope::Query, None).await {
        Ok(purged) => (StatusCode::OK, Json(json!({ "purged": purged }))).into_response(),
        Err(error) => store_unavailable(error),
    }
}

/// `purgeHistory`（POST /api/v1/query-history/purge）→ 200 `{ purged }`。
/// 体 `{ scope, retainDays? }`；`workflow` 作用域的词汇属 Workflow 功能族
/// （本表未承载），按 0 行如实回答——不是错误。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PurgeHistoryBody {
    scope: String,
    #[serde(default)]
    retain_days: Option<u32>,
}

async fn handler_purge_history(
    State(state): State<QueryAssetsState>,
    identity: SessionIdentity,
    Json(body): Json<PurgeHistoryBody>,
) -> Response {
    let scope = match HistoryScope::parse(&body.scope) {
        Some(scope) => scope,
        None => {
            return invalid_argument(format!(
                "invalid history scope: {} (expected query | workflow | all).",
                body.scope
            ));
        }
    };
    let store = ServerQueryHistoryStore::for_identity(
        state.store,
        identity.ctx.organization_id.as_str(),
        identity.ctx.principal_id.as_str(),
    );
    match store.purge(scope, body.retain_days).await {
        Ok(purged) => (StatusCode::OK, Json(json!({ "purged": purged }))).into_response(),
        Err(error) => store_unavailable(error),
    }
}

/// `FavoriteQuery` 的显式使用占位（域 wire 词汇；防未用导入漂移）。
const _: fn(&FavoriteQuery) -> &str = |favorite| favorite.id.as_str();
