//! tableData 域路由（契约 §6.7 读与预览 / §6.8 变更两段式 / §6.9 命名视图
//! 的 HTTP 薄绑定）。tableData 共 **14 方法全部上 HTTP**；本文件是它们的
//! 唯一落点。
//!
//! ## 14 方法 × 状态表（W8-a 接线刀刷新）
//!
//! | 方法 | 路由 | 状态 | 说明 |
//! | --- | --- | --- | --- |
//! | `listNamedViews` | `GET /api/v1/named-views` | ✅ 真实现 | 经 `NamedViewService`，`ServerNamedViewStore` 落 v9 表 |
//! | `saveNamedView` | `PUT /api/v1/named-views` | ✅ 真实现 | 空 id 铸造 → 201，已知 id 更新 → 200（§6.9） |
//! | `deleteNamedView` | `DELETE /api/v1/named-views/{id}` | ✅ 真实现 | 未知 id 幂等无操作，恒 200 返剩余集合（§6.9） |
//! | `getTableData` | `POST /api/v1/table-data/reads` | ✅ 真实现 | `read_table_page` 用例（W8-a 接线刀）：profile/config/驱动闸序（handler 侧副本保错误序稳定）→ `ServerTargetRunner` target 分支执行；授权语 = `SessionIdentity.ctx` 真主体 |
//! | `previewTableReadSql` | `POST /api/v1/table-data/read-sql-previews` | ✅ 真实现 | `preview_table_read` 用例（同一闸序；不执行任何语句） |
//! | `estimateTableRows` | `POST /api/v1/table-data/row-count-estimates` | ✅ 真实现 | `estimate_table_rows` 用例（会话面）：目录可见性闸 → `get_session` → 驱动估算语句经 runner manual 分支执行；null = 驱动不能估算，是答案不是错误，**无精确计数回退** |
//! | `resolveForeignKeyTargets` | `POST /api/v1/table-data/foreign-key-targets` | ⛔ fail-closed | `services::fk_navigation` 要求 `ForeignKeyNavigationSource.bind_session` 交出**活 `ConnectionHandle`**（三条驱动往返共享一次绑定）；server 执行面命令经 runtime 派发、无直连句柄面 → 501 |
//! | `listInboundForeignKeys` | `POST /api/v1/table-data/inbound-foreign-keys` | ⛔ fail-closed | `services::referenced_by` 同上（同一端口同一句柄依赖）→ 501 |
//! | `searchTableValues` | `POST /api/v1/table-data/value-searches` | ⛔ fail-closed | `services::value_search` 直接在活 `ConnectionHandle` 上跑驱动查询；无直连句柄面 → 501 |
//! | `resolveResultOrigin` | `POST /api/v1/table-data/result-origins` | ⛔ fail-closed | 来源重推导要求驱动在活句柄上应答（`describe_result_origin(&handle)`）+ 目录缓存同句柄读；无直连句柄面 → 501 |
//! | `previewPendingChanges` | `POST /api/v1/table-data/change-plan-previews` | ✅ 真实现 | `preview_changes` 用例（W8 刀3）：读三端点同款闸序 → `ServerTableColumnSource` 命令面列源（刀1 件）现查列/主键 → 200 `RowChangePlan`；`configRevision` 盖预览时刻的 profile revision（commit CAS 基准）；不设 read_only 闸（桌面镜像：预览不执行任何语句） |
//! | `commitPendingChanges` | `POST /api/v1/table-data/change-commits` | ✅ 真实现 | `commit_changes` + `execute_plan`（W8 刀3）：单次 lease resolve 的 runtime 实例贯穿提交序列；恒 owned-session（宿主开/驱动/关计划自己的会话），`manualHandle` 在场 → 501 明拒（绝不静默降级）；共享层五道闸（上下文/漂移/CAS/read_only/schema 比对）全重检 |
//! | `commitQueryResultCell` | `POST /api/v1/table-data/result-cell-commits` | ⛔ fail-closed | 来源重推导（活句柄）+ 写半边（`execute_plan`）双双缺失 → 501 |
//! | `previewResultCellChange` | `POST /api/v1/table-data/result-cell-change-previews` | ⛔ fail-closed | 同上（写前 origin 重读也是活句柄依赖）→ 501 |
//!
//! fail-closed 的形状沿 `ai_host::execute_target` 先例：路由照常挂载（§6.7
//! / §6.8 在账），handler 显式回答 **501 `capabilityUnsupported`**（
//! not-implemented 类，`http::response` 状态码表的 with_status 覆盖），文案
//! 携带逐端口的不可服务原因——**绝不返回看似成功的结果**。剩 6 条 ⛔ 路由
//! 的不可服务原因已收窄为逐端口缺失：FK/值搜索/来源重推导卡「活
//! `ConnectionHandle` 面」（刀2 的活句柄面），result-cell 二卡「原会话绑定
//! + 执行期结果映射探针」且 preview 被其 commit 半边封住（成对原则的剩余
//! 封条）。变更二（preview/commit）已由 W8 刀3 点亮（命令面列源 + runtime
//! 写半边 + 单次 resolve 的会话序列）。
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

use datazen::connection::domain::catalog::{ConnectionRepository, ConnectionScope};
use datazen::connection::domain::profile::ProfileRepository;
use datazen::execution::application::use_cases::ConnectionUseCases;
use datazen::execution::target::{ExecutionTarget, ObjectTarget};
use datazen::execution::SessionDirectory;
use datazen::execution::SessionHandle;
use datazen::named_views::{NamedView, NamedViewError, NamedViewScope, NamedViewService};
use datazen::schema::application::scope::{
    embedded_schema_of, metadata_schema, table_read_database,
};
// `services::validate_filters` 是本件的 re-export 别名；server 的 datazen
// feature 集不带 `services` 门，直连 executor 底座（同一实现）。
use datazen::shared::context::RequestContext;
use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::shared::id::{ConnectionId, Counter, DbSessionId};
use datazen::table_browsing::api::{
    estimate_table_rows, preview_table_read, read_table_page, TableReadError,
    TableReadPreviewRequest, TableReadRequest, TargetCommandRunner,
};
use datazen::table_browsing::executor::filter_sql::validate_filters;
use datazen::table_editing::validate_table_context;
use datazen::table_editing::{
    commit_changes, preview_changes, ArtifactChunkReader, CommitPendingChangesRequest,
    ConnectionFacts, PendingRowChange, RowChangePlan, RowChangeTableContext, TableEditingError,
};
use datazen_driver_api::ConnectionConfig;

use crate::execution::resource_host::ServerArtifactChunks;
use crate::execution::table_columns::ServerTableColumnSource;
use crate::http::middleware::authorize::NOT_VISIBLE_MESSAGE;
use crate::http::response::ApiErrorResponse;
use crate::http::router::{api_error_response, invalid_argument, SessionIdentity};
use crate::storage::named_views::{ServerNamedViewStore, UuidNamedViewIdMinter};
use crate::storage::{AdminStore, ServerConnections, ServerProfiles};

/// tableData 域的装配状态：管理库句柄（v9 `named_views` 表；调用方负责
/// 先 `migrate`）+ 驱动注册表与执行半边（W8-a P2）+ 连接事实与会话面
/// （W8-a 接线刀）。所有实例与既有域共享（bootstrap 既有纪律）。
#[derive(Clone)]
pub struct TableDataState {
    pub store: Arc<AdminStore>,
    /// 驱动注册表（inventory 懒形；与执行后端 / driverGateway / mcp 目标
    /// 机器同一实例）。读三端点接线时取方言驱动用。
    pub registry: Arc<datazen::execution::runtime::driver_registry::DriverRegistry>,
    /// tableData 执行半边（W8-a P2）：[`TargetCommandRunner`] 端口 server
    /// 实现（`execution::target_runner`）；host/lease 与 driverGateway /
    /// mcp 目标机器同一实例（bootstrap 既有纪律）。
    pub runner: Arc<dyn TargetCommandRunner>,
    /// 连接 profile 事实（W8-a 接线刀）：读三端点的 profile 闸与 config
    /// 回读（desktop `platform_profile` 的 server 对应物）。按请求组织
    /// 读取（`SessionIdentity.ctx` 真主体语义），与执行闸（runner 的
    /// `prepare`）双读同源——版本不一致由 prepare 的 CAS fail-closed。
    pub profiles: Arc<ServerProfiles>,
    /// 连接目录（W8-a 接线刀）：`ConnectionConfig`（database_type / 默认
    /// database / schema）与 read_only 事实源；与 bootstrap 的
    /// `ServerResourceExecutionHost` 同一实例。
    pub connections: Arc<ServerConnections>,
    /// 会话目录（W8-a 接线刀）：estimateTableRows 的可见性闸 + epoch 补全，
    /// 与 sessions 域 / 租约源**同一**实例。
    pub directory: Arc<dyn SessionDirectory>,
    /// 会话用例面（W8-a 接线刀）：estimateTableRows 的 `get_session`；与
    /// sessions 域同一实例（`LeaseConnectionUseCases`，每调用现取租约）。
    pub sessions: Arc<dyn ConnectionUseCases>,
    /// runtime 租约源（W8 刀3）：commitPendingChanges 的写半边经**单次
    /// resolve** 取 `Arc<RuntimeConnectionUseCases>`，贯穿 open_session /
    /// issue_submission_token / execute_in_session / get_execution 轮询 /
    /// close_session 的整个提交序列（CM-63 cell 语义 + 「单次 resolve 是对
    /// 的构造」裁决）。与 MCP / sessions 域同一实例（bootstrap 既有纪律）。
    pub lease: Arc<dyn crate::execution::lease::RuntimeLeaseSource>,
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

// ── tableData 读与预览（§6.7，✅ W8-a 接线刀）──────────────────────────────
//
// 两条 target 面端点共用同一闸序（desktop `table_target.rs::read/preview`
// 的 server 同构投影；宿主侧副本保本传输的错误序稳定，共享层用例重检同
// 一批事实）。`SessionIdentity.ctx` 是授权语：profile/config 读取带请求
// 真主体组织，执行面（runner 的 token/execute/poll/collect）吃同一 ctx
// （P3 语义延伸到 HTTP 面）。

/// 读三端点共用的闸序：profile 存在 → 未禁用 → 驱动类型与请求上下文一致 →
/// 方言驱动可用。返回 (config, 调用方已读版本, 驱动)——版本进用例作 CAS
/// （runner 的 `prepare` 内部还会复验，版本漂移 fail-closed）。
///
/// 错误序照 desktop 逐字对齐：未知/禁用 profile → `NotFound`；驱动类型
/// 漂移 → `InvalidArgument`「Table connection context changed」；注册表
/// 无方言 → `ServiceUnavailable`「SQL driver unavailable」。
async fn resolve_read_target(
    state: &TableDataState,
    ctx: &RequestContext,
    context: &datazen::table_editing::RowChangeTableContext,
) -> Result<
    (
        ConnectionConfig,
        Counter,
        Arc<dyn datazen_driver_api::DatabaseDriver>,
    ),
    Response,
> {
    let profile = state
        .profiles
        .get(ctx, ConnectionId::new(context.connection_id.clone()))
        .await
        .map_err(|error| {
            api_error_response(ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                error.to_string(),
            ))
        })?
        .ok_or_else(|| profile_unavailable())?;
    if !profile.enabled {
        return Err(profile_unavailable());
    }
    let scope = ConnectionScope::new(ctx.organization_id.as_str());
    let config = state
        .connections
        .get(&scope, context.connection_id.as_str())
        .await
        .map_err(|error| {
            api_error_response(ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                error.to_string(),
            ))
        })?
        .ok_or_else(|| profile_unavailable())?;
    if config.database_type != context.driver_type {
        return Err(api_error_response(ApiError::new(
            ApiErrorCode::InvalidArgument,
            "Table connection context changed",
        )));
    }
    let driver = state
        .registry
        .get_sql_driver_by_name(config.database_type.as_str())
        .await
        .ok_or_else(|| {
            api_error_response(ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                "SQL driver unavailable",
            ))
        })?;
    Ok((
        config,
        Counter::new(profile.config_revision.counter().get()),
        driver,
    ))
}

/// 未知/禁用 profile 的统一 404（desktop 副本文案；防枚举口径与 sessions
/// 域一致——不区分「不存在」与「不可见」）。
fn profile_unavailable() -> Response {
    api_error_response(ApiError::new(
        ApiErrorCode::NotFound,
        "Connection profile unavailable",
    ))
}

/// `TableReadError` → wire 映射（宿主自持 HTTP 版；desktop 的
/// `From<TableReadError> for CommandError` 是桌面那份）。`Runtime` 原样
/// 透传（`ApiError` 已是 wire 词汇，`ResourceBusy` 等可处置类别不容折叠）；
/// `Validation` → 400；`Internal` 是用例层故障，closed 码表里最近的诚实
/// 类别是 `ServiceUnavailable`（不是调用方的输入错误）。
fn table_read_error_response(error: TableReadError) -> Response {
    match error {
        TableReadError::Runtime(error) => api_error_response(error),
        TableReadError::Validation(message) => {
            api_error_response(ApiError::new(ApiErrorCode::InvalidArgument, message))
        }
        TableReadError::Internal(message) => {
            api_error_response(ApiError::new(ApiErrorCode::ServiceUnavailable, message))
        }
    }
}

/// `getTableData`（POST /api/v1/table-data/reads）→ 200 `TableDataResult`。
///
/// 体是 `TableReadRequest`（camelCase；共享层 DTO 直接反序列化）。v1 HTTP
/// 面的手动会话句柄随体透传：带 `manualHandle` 的读走 runner 的 manual
/// 分支（租约现取 + 三字段校验），与桌面 IPC 面同一语义。
async fn handler_get_table_data(
    State(state): State<TableDataState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let request: TableReadRequest = match serde_json::from_value(body) {
        Ok(request) => request,
        Err(source) => return invalid_argument(format!("read: {source}")),
    };
    if let Err(error) = validate_table_context(&request.context) {
        return api_error_response(ApiError::new(
            ApiErrorCode::InvalidArgument,
            error.to_string(),
        ));
    }
    if request.page_size == 0 || request.page_size > 10000 {
        return invalid_argument("Invalid table page size");
    }
    let (_config, revision, driver) =
        match resolve_read_target(&state, &identity.ctx, &request.context).await {
            Ok(resolved) => resolved,
            Err(response) => return response,
        };
    // 过滤器先于语句装配拒绝（desktop 同款注释）：这个数据库服务不了的
    // 过滤器，或缺值的过滤器，必须在建语句、用连接之前失败。
    if let Err(rejection) = validate_filters(
        driver.supported_filter_operators(),
        request.filters.as_deref().unwrap_or_default(),
    ) {
        return api_error_response(ApiError::new(
            ApiErrorCode::InvalidArgument,
            rejection.contract_message(),
        ));
    }
    match read_table_page(
        driver.as_ref(),
        request,
        revision,
        identity.ctx.clone(),
        state.runner.as_ref(),
    )
    .await
    {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(error) => table_read_error_response(error),
    }
}

/// `previewTableReadSql`（POST /api/v1/table-data/read-sql-previews）→ 200
/// `TableReadPreviewSql`（不执行任何语句）。
async fn handler_preview_table_read_sql(
    State(state): State<TableDataState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let request: TableReadPreviewRequest = match serde_json::from_value(body) {
        Ok(request) => request,
        Err(source) => return invalid_argument(format!("preview: {source}")),
    };
    if let Err(error) = validate_table_context(&request.context) {
        return api_error_response(ApiError::new(
            ApiErrorCode::InvalidArgument,
            error.to_string(),
        ));
    }
    if request.page_size == 0 || request.page_size > 10000 {
        return invalid_argument("Invalid table page size");
    }
    let (_config, revision, driver) =
        match resolve_read_target(&state, &identity.ctx, &request.context).await {
            Ok(resolved) => resolved,
            Err(response) => return response,
        };
    // 与真读同款拒绝：这个数据库会拒绝的语句的预览是谎言，不是预览。
    if let Err(rejection) = validate_filters(
        driver.supported_filter_operators(),
        request.filters.as_deref().unwrap_or_default(),
    ) {
        return api_error_response(ApiError::new(
            ApiErrorCode::InvalidArgument,
            rejection.contract_message(),
        ));
    }
    match preview_table_read(
        driver.as_ref(),
        request,
        revision,
        identity.ctx.clone(),
        state.runner.as_ref(),
    )
    .await
    {
        Ok(preview) => (StatusCode::OK, Json(preview)).into_response(),
        Err(error) => table_read_error_response(error),
    }
}

// ── 行数估算（§6.7，✅ 会话面）─────────────────────────────────────────────

/// `POST /api/v1/table-data/row-count-estimates` 的请求体（契约
/// `EstimateTableRowsRequest`：dbSessionId 契约，会话面）。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EstimateTableRowsBody {
    db_session_id: String,
    table: String,
    database: Option<String>,
    schema: Option<String>,
}

/// `estimateTableRows` → 200 `number | null`（null = 驱动不能估算，是答案
/// 不是错误；**无精确计数回退**——决定花精确计数的功夫是调用方的事）。
///
/// 会话面闸序：目录可见性闸（越组织/未知 id → 404 固定文案，sessions 域
/// 同一条）→ `get_session`（真主体 ctx）→ 驱动解析 → database/schema 梯子
/// （请求 → 档案 → 驱动默认，与桌面估算爬同一份共享梯子）→ 估算语句经
/// runner 的 manual 分支在会话内执行。绑定由 handler 从会话事实铸造
/// （连接/版本/命名空间三字段与 runner 的 manual 校验自洽）。
async fn handler_estimate_table_rows(
    State(state): State<TableDataState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let request: EstimateTableRowsBody = match serde_json::from_value(body) {
        Ok(request) => request,
        Err(source) => return invalid_argument(format!("estimateTableRows: {source}")),
    };
    if request.db_session_id.trim().is_empty() {
        return invalid_argument("dbSessionId is required");
    }
    if request.table.trim().is_empty() {
        return invalid_argument("table is required");
    }
    // 可见性先于会话存活（sessions 域裁决逐字落地：越组织/未知 → 404 固定
    // 文案，防 ID 枚举）。
    let owner = match state
        .directory
        .lookup_owner(DbSessionId::new(request.db_session_id.clone()))
        .await
    {
        Ok(Some(owner)) => owner,
        Ok(None) => return not_visible(),
        Err(source) => {
            return api_error_response(ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                format!("session directory unavailable: {source}"),
            ));
        }
    };
    if owner.organization_id != identity.ctx.organization_id {
        return not_visible();
    }
    let session = match state
        .sessions
        .get_session(&identity.ctx, owner.to_handle())
        .await
    {
        Ok(view) => view,
        Err(error) => return api_error_response(error),
    };
    let scope = ConnectionScope::new(identity.ctx.organization_id.as_str());
    let config = match state
        .connections
        .get(&scope, session.connection_id.as_str())
        .await
    {
        Ok(Some(config)) => config,
        Ok(None) => return profile_unavailable(),
        Err(error) => {
            return api_error_response(ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                error.to_string(),
            ));
        }
    };
    let driver = match state
        .registry
        .get_sql_driver_by_name(config.database_type.as_str())
        .await
    {
        Some(driver) => driver,
        None => {
            return api_error_response(ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                "SQL driver unavailable",
            ));
        }
    };
    // 梯子与桌面估算爬同一份共享实现（schema::application::scope）。
    let database = table_read_database(request.database.as_deref(), config.database.as_deref());
    let schema = metadata_schema(
        driver.as_ref(),
        request.schema.as_deref(),
        embedded_schema_of(&request.table),
        config.schema.as_deref(),
    );
    let binding = datazen::table_browsing::api::TargetBinding {
        target: ExecutionTarget::new(
            session.connection_id.clone(),
            session.observed_context.namespace.clone(),
        )
        .with_object(ObjectTarget::new("table", request.table.clone())),
        revision: session.config_revision,
        manual_handle: Some(session.handle.clone()),
    };
    match estimate_table_rows(
        driver.as_ref(),
        &binding,
        database,
        schema.as_deref(),
        &request.table,
        identity.ctx.clone(),
        state.runner.as_ref(),
    )
    .await
    {
        Ok(estimate) => (StatusCode::OK, Json(estimate)).into_response(),
        Err(error) => table_read_error_response(error),
    }
}

/// 越组织/不存在的统一 404（sessions 域同一条固定文案；防 ID 枚举）。
fn not_visible() -> Response {
    api_error_response(ApiError::new(ApiErrorCode::NotFound, NOT_VISIBLE_MESSAGE))
}

// ── 变更两段式（§6.8，✅ W8 刀3）──────────────────────────────────────
//
// 成对点亮（评估档 §6.1 硬原则：预览与提交必须同刀亮起——预览先亮 = 调用方
// 拿到永远无法提交的计划，是半个答案）。两条 handler 共用读三端点的闸序
// （`resolve_read_target`）与刀1 的命令面列源；写半边经单次 lease resolve
// 的 runtime 实例贯穿整个提交序列（open/token/execute/close 落同一实例的
// 私有账，CM-63 cell 语义 + 评估档 §4 表第 6 行的构造裁决）。
//
// 语义边界（如实登记）：commit 不提供请求级幂等——HTTP 重试是一次新请求、
// 新令牌、新会话，fingerprint 守计划内容而不守「恰好一次」（共享层既有
// 语义，桌面同权；请求级幂等头留给后续刀，前端重试只在网络层失败且未收到
// 响应时进行）。

/// `POST /api/v1/table-data/change-plan-previews` 的体（契约
/// `PreviewPendingChangesRequest`）。前端 `PendingRowChange.rowIndex` 是
/// UI-only 提示字段，serde 默认忽略未知字段——不做 `deny_unknown_fields`。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewPendingChangesBody {
    context: RowChangeTableContext,
    changes: Vec<PendingRowChange>,
}

/// `POST /api/v1/table-data/change-commits` 的体（契约
/// `CommitPendingChangesRequest`）。
///
/// `manual_handle` 必须显式反序列化并显式拒绝（Some → 501）：serde 默认
/// 忽略未知字段，省掉它会静默把 manual 提交降级为 owned-session 执行——
/// 调用方以为写进了自己的事务，实际落在一个已提交并关闭的陌生会里，
/// 比拒绝更危险。web 形态没有编辑事务面（legacy 轨不上 HTTP、
/// `platform.*` 不进驱动命令目录），manual 路在本宿主显式 closed；放开
/// 条件（web 面出现编辑事务形态）登记在 `w8-knife3-mutation-pair-design.md`
/// §0.2。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitPendingChangesBody {
    db_session_id: String,
    #[serde(default)]
    manual_handle: Option<SessionHandle>,
    plan: RowChangePlan,
    fingerprint: String,
}

/// `previewPendingChanges`（POST /api/v1/table-data/change-plan-previews）
/// → 200 `RowChangePlan`（不执行任何语句）。
///
/// 闸序：serde 解析 → 共享层上下文校验 → 读三端点同款闸序（profile 存在
/// → 未禁用 → 驱动类型漂移 → 方言驱动；preview 不设 read_only 闸——
/// 桌面镜像）→ 列源现查（每调用经命令面跑 `get_table_schema`，预览永远答
/// 「当下」的列与主键）→ `preview_changes` 建计划并盖 config_revision 章
/// （commit CAS 的基准）。
async fn handler_preview_pending_changes(
    State(state): State<TableDataState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let request: PreviewPendingChangesBody = match serde_json::from_value(body) {
        Ok(request) => request,
        Err(source) => return invalid_argument(format!("preview: {source}")),
    };
    if let Err(error) = validate_table_context(&request.context) {
        return api_error_response(ApiError::new(
            ApiErrorCode::InvalidArgument,
            error.to_string(),
        ));
    }
    let (_config, revision, driver) =
        match resolve_read_target(&state, &identity.ctx, &request.context).await {
            Ok(resolved) => resolved,
            Err(response) => return response,
        };
    let columns = ServerTableColumnSource::new(
        state.runner.clone(),
        state.registry.clone(),
        identity.ctx.clone(),
        revision,
    );
    match preview_changes(
        driver.as_ref(),
        request.context,
        &request.changes,
        &columns,
        revision.get(),
    )
    .await
    {
        Ok(plan) => (StatusCode::OK, Json(plan)).into_response(),
        Err(error) => table_editing_error_response(error),
    }
}

/// `commitPendingChanges`（POST /api/v1/table-data/change-commits）→ 200
/// `CommitPendingChangesResponse`（同步回执；契约 §6.8：结果在响应体里的
/// 用 200）。
///
/// 闸序：serde 解析 → manual 路明拒（501，先于一切执行触碰）→ 共享层
/// 上下文校验（桌面「feature layer re-checks the context」双检序镜像）→
/// 读三端点同款闸序（未知/禁用 profile 在任何会话资源分配之前 404，
/// 防枚举口径与读端点同一条）→ 单次 lease resolve → facts 组装 →
/// `commit_changes`（共享层五道闸全重检 → 重读列 → 不可变计划重建 →
/// `execute_plan` owned-session 全链；CAS 双闸：facts 对过期计划拒绝、
/// 会话开立后再对 `opened.config_revision` 复验）。
async fn handler_commit_pending_changes(
    State(state): State<TableDataState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let body: CommitPendingChangesBody = match serde_json::from_value(body) {
        Ok(body) => body,
        Err(source) => return invalid_argument(format!("commit: {source}")),
    };
    if body.manual_handle.is_some() {
        // manual 会话事务在本宿主显式 closed（501 而非 400：删掉 manualHandle
        // 字段请求即合法，「这个子能力此宿主尚未提供」是装配事实而不是请求
        // 错误；与同域其余 not-served 面同一 wire 词汇）。
        let error = ApiError::new(
            ApiErrorCode::CapabilityUnsupported,
            "commitPendingChanges does not serve manual session transactions \
             (manualHandle) on this host yet: the host opens, drives and closes \
             the plan's own session (owned-session mode); send manualHandle as \
             null or omit the field. No statement ran and nothing was written.",
        );
        return ApiErrorResponse::new(error)
            .with_status(StatusCode::NOT_IMPLEMENTED)
            .into_response();
    }
    let request = CommitPendingChangesRequest {
        db_session_id: body.db_session_id,
        plan: body.plan,
        fingerprint: body.fingerprint,
    };
    if let Err(error) = validate_table_context(&request.plan.table) {
        return api_error_response(ApiError::new(
            ApiErrorCode::InvalidArgument,
            error.to_string(),
        ));
    }
    let (config, revision, driver) =
        match resolve_read_target(&state, &identity.ctx, &request.plan.table).await {
            Ok(resolved) => resolved,
            Err(response) => return response,
        };
    // 单次 resolve 的实例贯穿整个提交序列：open_session 回执、令牌签发、
    // execute_in_session 消费、轮询、close_session 幂等回放落同一实例的
    // 私有账（刀0 cell 快路下每调用 resolve 也恒返同一 Arc，但那是碰巧；
    // 单次 resolve 是构造）。持有 Arc 使在途提交对中途 revoke() 免疫
    // （吊销只影响后续 resolve，不影响已借出的实例）。
    let runtime = match state.lease.resolve().await {
        Ok(runtime) => runtime,
        Err(message) => {
            return api_error_response(ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                message,
            ));
        }
    };
    let facts = ConnectionFacts {
        database_type: config.database_type.clone(),
        read_only: config.read_only,
        database: config.database.clone(),
        schema: config.schema.clone(),
        config_revision: revision.get(),
    };
    let columns = ServerTableColumnSource::new(
        state.runner.clone(),
        state.registry.clone(),
        identity.ctx.clone(),
        revision,
    );
    let artifacts = Arc::new(ServerArtifactChunks(runtime.clone())) as Arc<dyn ArtifactChunkReader>;
    match commit_changes(
        driver.as_ref(),
        request,
        &facts,
        &columns,
        None, // manual 路已被上面显式拒绝；HTTP 面恒 owned-session
        runtime,
        artifacts,
        identity.ctx.clone(),
    )
    .await
    {
        Ok(receipt) => (StatusCode::OK, Json(receipt)).into_response(),
        Err(error) => table_editing_error_response(error),
    }
}

/// `TableEditingError` → wire 映射（写半边的宿主自持 HTTP 版；desktop 的
/// `From<TableEditingError> for CommandError` 是桌面那份）。`Validation` →
/// 400（stalePlan / readOnly / affectedMismatch 等共享层守卫文案都是调用方
/// 可修正的输入或状态问题，消息原样上 wire——前端按 `grid.commit.*` 码
/// 前缀分支）；`NotFound` → 404（封闭码表的 404 行）；`NotConfigured` /
/// `Internal` / `Driver` / `Io` / `Json` → 503（用例层故障不是调用方的输入
/// 错误，closed 码表里最近的诚实类别）。
///
/// 已知状态码漂移（如实登记）：`From<ApiError> for TableEditingError` 的
/// 兼容桶把 `ConfigRevisionMismatch`（原生 409）折进 Validation（此处
/// 400）、`SessionQuotaExceeded`（原生 429）折进 Internal（此处 503）——
/// 共享层既定桶定义的投影后果，非本刀新语义；消息文本（含 `grid.commit.*`
/// 码前缀与 requestId 尾注）不受影响，状态码归一属 WR-D4 错误映射刀。
fn table_editing_error_response(error: TableEditingError) -> Response {
    let (code, message) = match error {
        TableEditingError::Validation(message) => (ApiErrorCode::InvalidArgument, message),
        TableEditingError::NotFound(message) => (ApiErrorCode::NotFound, message),
        TableEditingError::NotConfigured(message) => (ApiErrorCode::ServiceUnavailable, message),
        TableEditingError::Internal(message) => (ApiErrorCode::ServiceUnavailable, message),
        TableEditingError::Driver(error) => (ApiErrorCode::ServiceUnavailable, error.to_string()),
        TableEditingError::Io(error) => (ApiErrorCode::ServiceUnavailable, error.to_string()),
        TableEditingError::Json(error) => (ApiErrorCode::ServiceUnavailable, error.to_string()),
    };
    api_error_response(ApiError::new(code, message))
}

// ── FK/来源/值搜索与 result-cell（§6.7/§6.8，⛔ fail-closed：逐端口收窄）──
//
// 六条路由逐一显式登记（模块头状态表）。不可服务原因已按端口收窄：
// FK / 值搜索 / 来源重推导卡「活 ConnectionHandle 面」（server 执行面
// 命令经 runtime 派发，无直连句柄；刀2 的活句柄面点亮它们），result-cell
// 二卡「原会话绑定 + 执行期结果映射探针」且 preview 被其 commit 半边封住
// （成对原则的剩余封条，REASON_COMMIT 仍被它消费）。变更二已由刀3 点亮，
// 不在本节。`SessionIdentity` 提取即认证链声明：未认证请求在
// L2 已 401，不进入本层。

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

const REASON_FK_SOURCE: &str = "the foreign-key services resolve through a \
ForeignKeyNavigationSource whose binding carries a live ConnectionHandle for \
the driver round trips; the server execution face dispatches commands through \
the runtime and exposes no direct handle surface.";
const REASON_DRIVER_QUERY: &str = "the service runs its driver queries at a live \
connection handle; the server execution face dispatches commands through the \
runtime and exposes no direct handle surface.";
const REASON_ORIGIN: &str = "origin re-derivation asks the driver at a live \
connection handle (describe_result_origin) and reads the catalog cache with \
the same handle; the server execution face exposes no direct handle surface.";
const REASON_COMMIT: &str = "change commit executes a fingerprint-confirmed plan \
through the runtime write half (execute_plan) and answers synchronously; the \
write half is not assembled on this host.";

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

// ── t11 错误映射臂钉（W8 刀3；组件级单测——`table_editing_error_response`
// 是本模块私有 fn，集成测试 crate 不可达，臂钉因此落在 `#[cfg(test)]`）。

#[cfg(test)]
mod mutation_error_mapping {
    use super::*;
    use axum::body::to_bytes;

    async fn wire(error: TableEditingError) -> (StatusCode, serde_json::Value) {
        let response = table_editing_error_response(error);
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("error body reads");
        let body: serde_json::Value =
            serde_json::from_slice(&bytes).expect("error body is the four-field wire shape");
        (status, body)
    }

    /// `Validation` → 400 `invalidArgument`（stalePlan / readOnly /
    /// affectedMismatch 的桶；`grid.commit.*` 码前缀原样上 wire）。
    #[tokio::test]
    async fn validation_maps_to_400_invalid_argument() {
        let (status, body) = wire(TableEditingError::Validation(
            "grid.commit.stalePlan: Connection configuration changed; preview the changes again"
                .into(),
        ))
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["code"], "invalidArgument");
        assert!(body["message"]
            .as_str()
            .expect("message")
            .contains("grid.commit.stalePlan"));
    }

    /// `NotFound` → 404 `notFound`（封闭码表的 404 行；`From<ApiError>` 的
    /// NotFound 桶回投影）。
    #[tokio::test]
    async fn not_found_maps_to_404() {
        let (status, body) =
            wire(TableEditingError::NotFound("session has been released".into())).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["code"], "notFound");
    }

    /// `NotConfigured` / `Internal` / `Driver` / `Io` / `Json` → 503
    /// `serviceUnavailable`（用例层故障不是调用方的输入错误）。
    #[tokio::test]
    async fn host_faults_map_to_503_service_unavailable() {
        for (status, body) in [
            wire(TableEditingError::NotConfigured("Driver unavailable".into())).await,
            wire(TableEditingError::Internal("quota gate failed".into())).await,
            wire(TableEditingError::Driver(datazen_driver_api::DriverError::Unsupported(
                "no such command".into(),
            )))
            .await,
            wire(TableEditingError::Io(std::io::Error::other("sink unavailable"))).await,
            wire(TableEditingError::Json(serde_json::from_str::<serde_json::Value>("{")
                .expect_err("malformed json")))
            .await,
        ] {
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
            assert_eq!(body["code"], "serviceUnavailable", "{body}");
        }
    }
}
