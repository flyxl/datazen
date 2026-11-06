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
//! | `resolveForeignKeyTargets` | `POST /api/v1/table-data/foreign-key-targets` | ✅ 真实现 | FK 跳转（W8 刀2）：共享借出助手（可见性闸 → 单次 resolve → `borrow_live_connection_by_id` 活句柄）→ `ServerForeignKeyNavigationSource` 绑定 → `services::fk_navigation::resolve_foreign_key_targets` 原样跑（三条驱动往返共享一次绑定，`exists:false` 是成功答案）；实现在 `table_data_live.rs` |
//! | `listInboundForeignKeys` | `POST /api/v1/table-data/inbound-foreign-keys` | ✅ 真实现 | 反向目录（W8 刀2）：同一端口同一适配器 → `services::referenced_by::list_inbound_foreign_keys`（≤200 次 schema 读，`truncated` 如实早停）；实现在 `table_data_live.rs` |
//! | `searchTableValues` | `POST /api/v1/table-data/value-searches` | ✅ 真实现 | 值搜索（W8 刀2）：活句柄借出 → estimate 同款共享梯子（`table_read_database`/`metadata_schema`）→ `services::value_search::search_table_values` 原样跑（绑定或内联二形、LIKE 转义全在共享层）；实现在 `table_data_live.rs` |
//! | `resolveResultOrigin` | `POST /api/v1/table-data/result-origins` | ✅ 真实现 | 来源重推导读半边（W8 刀2）：活句柄借出 → `describe_result_origin(&handle)`（驱动答不出 → 400 + `grid.result.originUnavailable`，不是错误）→ 列/主键半边走 `schema_cache`（miss = nothing is provable，按只读处理）→ `evaluate_result_editability`（只读裁决带 reason 也是 200）；实现在 `table_data_live.rs` |
//! | `previewPendingChanges` | `POST /api/v1/table-data/change-plan-previews` | ✅ 真实现 | `preview_changes` 用例（W8 刀3）：读三端点同款闸序 → `ServerTableColumnSource` 命令面列源（刀1 件）现查列/主键 → 200 `RowChangePlan`；`configRevision` 盖预览时刻的 profile revision（commit CAS 基准）；不设 read_only 闸（桌面镜像：预览不执行任何语句） |
//! | `commitPendingChanges` | `POST /api/v1/table-data/change-commits` | ✅ 真实现 | `commit_changes` + `execute_plan`（W8 刀3）：单次 lease resolve 的 runtime 实例贯穿提交序列；恒 owned-session（宿主开/驱动/关计划自己的会话），`manualHandle` 在场 → 501 明拒（绝不静默降级）；共享层五道闸（上下文/漂移/CAS/read_only/schema 比对）全重检 |
//! | `commitQueryResultCell` | `POST /api/v1/table-data/result-cell-commits` | ✅ 真实现 | 原会话绑定写回（W8 刀4）：binding 复验（execution 台账跨请求可读，`dbSessionId` + `executionId` 指认）→ 会话上下文比对 → 只读预检 → 共享层 `commit_result_cell_changes`（五道闸重检 + 原会话 `execute_in_session` + 轮询收集 + rowsAffected 双形状兼容）；事务语义随会话，会话永不被本半边关闭；`manualHandle` 在场 → 501 明拒 |
//! | `previewResultCellChange` | `POST /api/v1/table-data/result-cell-change-previews` | ✅ 真实现 | 单格变更计划（W8 刀4，与 commit 同刀成对点亮）：前闸同 commit → 写前 origin **重读**（刀2 读半边同源：`describe_result_origin` + schema cache + `evaluate_result_editability`，不信任客户端声明映射）→ 行定位（产物重读 + 结果集坐标 `rowIndex`）→ 单格 `PendingRowChange` → `preview_changes` 共享层计划（与刀3 同一份，输出同形状） |
//!
//! **tableData 域 14 方法全部点亮，域内 fail-closed 面清零**（W8 刀4 收口；
//! 11 条 501 逐刀点亮：读三端点 + 变更二（刀3/#192）、FK 二 + 值搜索 +
//! origin 读半边（刀2/#193）、result-cell 二（刀4/本刀），另三条为
//! W8-a 接线刀的 `getTableData` / `previewTableReadSql` / `estimateTableRows`）。
//! fail-closed 的形状沿 `ai_host::execute_target` 先例，仅在点亮前使用：
//! 路由照常挂载（§6.7 / §6.8 在账），handler 显式回答 **501
//! `capabilityUnsupported`**，文案携带逐端口的不可服务原因——绝不返回看似
//! 成功的结果。变更二（preview/commit）由 W8 刀3 点亮（命令面列源 +
//! runtime 写半边 + 单次 resolve 的会话序列）；FK 二 + 值搜索 + 来源重推导
//! 由 W8 刀2 点亮（活句柄面 + `SchemaCache`，读半边落在
//! `table_data_live.rs`）；result-cell 二由 W8 刀4 点亮（原会话绑定写回，
//! 实现见本文件尾部与共享层 `table_editing::write_exec::
//! commit_result_cell_changes`）。
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

use std::collections::{BTreeMap, HashMap};
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
use datazen::execution::application::orchestration::RuntimeConnectionUseCases;
use datazen::execution::application::use_cases::ConnectionUseCases;
use datazen::execution::target::{ExecutionTarget, ObjectTarget};
use datazen::execution::SessionDirectory;
use datazen::execution::{
    ContextConfidence, ExecutionId, ExecutionState, RuntimeResultBinding, SessionHandle,
};
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
use datazen::table_editing::result_edit::{
    evaluate_result_editability, read_only_error_code, GridReadOnlyReason, ResultErrorCode,
    WriteTarget,
};
use datazen::table_editing::validate_table_context;
use datazen::table_editing::{
    commit_changes, commit_result_cell_changes, preview_changes, ArtifactChunkReader,
    CommitPendingChangesRequest, ConnectionFacts, PendingRowChange, RowChangePlan,
    RowChangeTableContext, TableEditingError,
};
use datazen_driver_api::ConnectionConfig;
use datazen_driver_api::SqlTarget;

use crate::execution::resource_host::ServerArtifactChunks;
use crate::execution::table_columns::ServerTableColumnSource;
use crate::http::domains::table_data_live::{
    borrow_live_for_session, connection_config_of, handler_list_inbound_foreign_keys,
    handler_resolve_foreign_key_targets, handler_resolve_result_origin,
    handler_search_table_values, lend, nonblank, source_table_columns,
};
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
    /// W8 刀2：四读端点（FK 二/值搜索/origin）的活句柄借出经同一字段
    /// resolve 当前实例——设计档的 `lease_source` 字段由刀3 已加的本字段
    /// 承担（同型，不另立重复字段）。
    pub lease: Arc<dyn crate::execution::lease::RuntimeLeaseSource>,
    /// 目录缓存（W8 刀2）：origin 的列/主键读通路径（`schema/cache.rs` 的
    /// `get_columns`，TTL 300s、缓存键第一维 connection_id）。bootstrap
    /// 全局一个实例（与桌面 `state.schema_cache` 同形）；失效接线
    /// （DDL/刷新 → `invalidate`）随刀3 的写半边定，不在刀2。
    pub schema_cache: Arc<datazen::schema::cache::SchemaCache>,
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
            // W8 刀2：活句柄面真实现（table_data_live.rs）。
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
/// 域一致——不区分「不存在」与「不可见」）。`pub(crate)`：活句柄面
/// （`table_data_live.rs`）的档案回退 miss 同一条文案，规则单家不复制。
pub(crate) fn profile_unavailable() -> Response {
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
/// `pub(crate)`：活句柄面（`table_data_live.rs`）的可见性闸同一条文案。
pub(crate) fn not_visible() -> Response {
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
            return api_error_response(ApiError::new(ApiErrorCode::ServiceUnavailable, message));
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

// ── 来源/单元格写半边（§6.7/§6.8，✅ W8 刀4：result-cell 二成对点亮）──
//
// 最后两条 fail-closed 路由的真实现（`w8-knife4-result-cell-design.md`）。
// 成对原则同刀3：预览与提交同刀亮起。桌面参照面
// （`commands/result_write.rs` 七步）的 server 事实源替换见设计档 §2：
// `RuntimeResultBinding` 不跨请求携带——客户端携 `dbSessionId` +
// `executionId`，server 用 execution 台账（共享 sink，跨请求可读）复验
// `runtime_binding` + Succeeded；桌面的 statementMetadata 探针由**写前
// origin 重读**（刀2 读半边同源：`describe_result_origin` + schema cache +
// `evaluate_result_editability`）替换——不信任客户端声明的映射；行定位从
// 产物块重读（#191 后产物词汇已统一：output `kind: rows` 通道优先，typed
// `chunk.rows` 兜底）。
//
// 错误映射（设计档 §4）：Validation → 400（`grid.result.*` / `grid.commit.*`
// 码前缀原样上 wire）；execution/会话 miss → 404 固定文案（防枚举口径与
// 四读同一条）；binding 比对失败（runtime_binding 缺席 / 会话漂移 / 结果非
// Succeeded）→ 409 `contextConflict`；`manualHandle` 在场 → 501 明拒（刀3
// 同款裁决）。提交半边的五道闸在共享层重检
// （`commit_result_cell_changes`），宿主不复制；原会话执行不开关事务
// （事务语义随会话），会话永不被本半边关闭（那是客户端的活会话）。

/// `previewResultCellChange` 的体。`statementIndex` + `rowIndex` 是结果集
/// 坐标（不是主键值——桌面请求体同词汇）；`sql` / `resultColumns` 与
/// `resolveResultOrigin` 同一词汇（origin 重推导的输入）；`database` /
/// `schema` / `table` 是可选的收窄提示（桌面 `ResolveResultOriginRequest`
/// 的 narrow-mapping 半边同形）。`value` 直接反序列化为 driver
/// `Value`（untagged serde——与 `PendingRowChange.currentValues` 同一词汇，
/// 计划构建与参数绑定零转换，不经 serde_json 中转）。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewResultCellBody {
    db_session_id: String,
    execution_id: String,
    statement_index: u32,
    row_index: usize,
    column: String,
    value: datazen_driver_api::Value,
    sql: String,
    result_columns: Vec<String>,
    #[serde(default)]
    database: Option<String>,
    #[serde(default)]
    schema: Option<String>,
    #[serde(default)]
    table: Option<String>,
}

/// `commitQueryResultCell` 的体：binding 指认（`dbSessionId` + `executionId`）
/// + 预览回传的 plan + fingerprint。行定位不重做——fingerprint 覆盖
/// `rowIdentity`（预览时从产物行读出的主键值），行序漂移表现为 fingerprint
/// 失配（设计档 §6 风险 1 的诚实失败）。`manual_handle` 必须显式反序列化并
/// 显式拒绝（Some → 501，刀3 同款；serde 默认忽略未知字段，省掉它会静默把
/// 请求降级——原会话即绑定句柄、宿主自持，客户端没有任何合法的 manual 值）。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitResultCellBody {
    db_session_id: String,
    execution_id: String,
    plan: RowChangePlan,
    fingerprint: String,
    #[serde(default)]
    manual_handle: Option<SessionHandle>,
}

/// 409 `contextConflict`（binding 比对失败家族；设计档 §4）。
fn context_conflict(message: &str) -> Response {
    api_error_response(ApiError::new(
        ApiErrorCode::ContextConflict,
        message.to_owned(),
    ))
}

/// 两条 result-cell 端点共用的前闸产物：单次 resolve 的 runtime（借出内部
/// 的同一 `Arc`）、复验通过的原会话绑定、连接配置、profile revision 与方言
/// 驱动。
struct ResultCellTarget {
    runtime: Arc<RuntimeConnectionUseCases>,
    binding: RuntimeResultBinding,
    view: datazen::execution::application::views::ExecutionView,
    borrowed: crate::http::domains::table_data_live::BorrowedLive,
    config: ConnectionConfig,
    revision: Counter,
    driver: Arc<dyn datazen_driver_api::DatabaseDriver>,
}

/// 两条端点共用的前闸（设计档 §3 ①②③，成对同序）：
///
/// 1. **目录可见性 + 活句柄借出**（四读端点同一条路径，单次 resolve，
///    RAII 归还）；
/// 2. **① binding 复验**：execution 台账跨请求可读——miss → 404 固定文案；
///    非 Succeeded / `runtime_binding` 缺席 / 会话 id 与 epoch 和请求指认的
///    活会话不符 → 409 `contextConflict`；
/// 3. **② 会话上下文比对**（能力感知，刀3 owned-session 裁决同款）：会话
///    现观察与结果产生时的记录观察两侧都是驱动能力事实，任一侧不可观察
///    （server 形态全部驱动）是能力缺失不是漂移——两侧都 Confirmed 才拿
///    `WriteTarget` 说话（单一「同一目标」表示，桌面镜像）；
/// 4. **profile/config/驱动闸**：session.connection_id 起算（连接不是请求
///    的一部分——从会话解析，四读同裁决）；未知/禁用 profile 在任何资源
///    分配之前 404（防枚举同一条）；
/// 5. **③ 只读预检**：以 `grid.result.*` 码拒绝（两条端点同闸——result-cell
///    预览的出口是可写裁决，只读连接上它是死答案，先拒更诚实；桌面把同一
///    拒绝放在 commit 前，码表同源）。
async fn resolve_result_cell_target(
    state: &TableDataState,
    ctx: &RequestContext,
    db_session_id: &str,
    execution_id: &str,
) -> Result<ResultCellTarget, Response> {
    let borrowed = borrow_live_for_session(state, ctx, db_session_id).await?;
    let view = match borrowed
        .runtime
        .get_execution(ctx, ExecutionId::new(execution_id.to_owned()))
        .await
    {
        Ok(view) => view,
        Err(error) => {
            return Err(match error.code {
                // 未知与越权 execution 同答 404 固定文案（防枚举；台账读
                // 不是目录读，固定文案借用 sessions 域裁决的同一条）。
                ApiErrorCode::NotFound | ApiErrorCode::PermissionDenied => not_visible(),
                _ => api_error_response(error),
            });
        }
    };
    if view.state != ExecutionState::Succeeded {
        return Err(context_conflict(
            "Result is not a succeeded execution; only a succeeded result can be written back",
        ));
    }
    let binding = match view.runtime_binding.clone() {
        Some(binding) => binding,
        None => {
            return Err(context_conflict(
                "Result requires its original live session",
            ));
        }
    };
    if binding.handle.db_session_id.as_str() != db_session_id
        || binding.handle != borrowed.session.handle
    {
        return Err(context_conflict(
            "Result requires its original live session",
        ));
    }
    // ② 会话上下文比对（能力感知，见模块注释）。
    if borrowed.session.observed_context.confidence == ContextConfidence::Confirmed {
        if let Some(expected) = view.provenance.as_ref().filter(|provenance| {
            provenance.context_after.confidence == ContextConfidence::Confirmed
        }) {
            if WriteTarget::of(&borrowed.session.observed_context)
                != WriteTarget::of(&expected.context_after)
            {
                return Err(context_conflict(
                    "Result session context changed; execute the query again before editing",
                ));
            }
        }
    }
    // profile/config/驱动闸（resolve_read_target 门序同构，起点是会话的
    // 连接而不是请求里的上下文——连接不是请求的一部分）。
    let profile = state
        .profiles
        .get(
            ctx,
            ConnectionId::new(borrowed.session.connection_id.as_str().to_owned()),
        )
        .await
        .map_err(|error| {
            api_error_response(ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                error.to_string(),
            ))
        })?
        .ok_or_else(profile_unavailable)?;
    if !profile.enabled {
        return Err(profile_unavailable());
    }
    let config = connection_config_of(state, ctx, borrowed.session.connection_id.as_str()).await?;
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
    // ③ 只读预检（码表在共享层 result_edit；`SessionReadOnly` 的拒绝码与
    // origin 裁决同族——同一决策的两端词汇一致）。
    if config.read_only {
        let code = read_only_error_code(GridReadOnlyReason::SessionReadOnly);
        return Err(api_error_response(ApiError::new(
            ApiErrorCode::InvalidArgument,
            format!("{code}: Connection is read-only; result cells cannot be written back"),
        )));
    }
    Ok(ResultCellTarget {
        runtime: borrowed.runtime.clone(),
        binding,
        view,
        borrowed,
        config,
        revision: Counter::new(profile.config_revision.counter().get()),
        driver,
    })
}

/// 读一个执行的全部产物块（桌面 `chunks` 助手的 server 同构投影：分页计数
/// 来自首块的 `publishedChunkCount`）。读取走 [`ServerArtifactChunks`] 端口
/// （刀3 提交半边同款），错误 503（台账/产物故障不是调用方的输入错误）。
async fn collect_execution_chunks(
    runtime: &Arc<RuntimeConnectionUseCases>,
    ctx: &RequestContext,
    view: &datazen::execution::application::views::ExecutionView,
) -> Result<Vec<datazen::execution::ResultChunk>, Response> {
    let reader = ServerArtifactChunks(runtime.clone());
    let mut chunks = Vec::new();
    for artifact in &view.artifact_ids {
        let first = reader
            .read_chunk(ctx, artifact, Counter::ZERO)
            .await
            .map_err(api_error_response)?;
        for index in 0..first.published_chunk_count.get() {
            let chunk = if index == 0 {
                first.clone()
            } else {
                reader
                    .read_chunk(ctx, artifact, Counter::new(index))
                    .await
                    .map_err(api_error_response)?
            };
            chunks.push(chunk);
        }
    }
    Ok(chunks)
}

/// 产物行收集（#191 后的统一词汇表，MCP `read_rows` 同款双通道裁决）：
/// 语句载荷在 output 通道（`kind: "rows"` + `statementIndex`）优先，typed
/// `chunk.rows` 通道兜底（`publish_rows` 形状）。`statement_index` 筛掉同一
/// 执行内其他语句的块。
fn statement_rows(chunks: &[datazen::execution::ResultChunk], statement_index: u32) -> Vec<Value> {
    let mut rows = Vec::new();
    for chunk in chunks {
        if chunk.source.statement_index != statement_index {
            continue;
        }
        match chunk
            .output
            .as_ref()
            .and_then(|output| output.get("kind"))
            .and_then(Value::as_str)
        {
            Some("rows") => {
                if let Some(batch) = chunk
                    .output
                    .as_ref()
                    .and_then(|output| output.get("rows"))
                    .and_then(Value::as_array)
                {
                    rows.extend(batch.iter().cloned());
                }
            }
            _ => rows.extend(chunk.rows.iter().cloned()),
        }
    }
    rows
}

/// 两条端点的信封预检（桌面 required 文案镜像）：非空 `dbSessionId` /
/// `executionId` / `sql` / `column` 与非空 `resultColumns`。commit 的体另有
/// plan / fingerprint（共享层不可变重建会拒绝空 planId / 空 fingerprint）。
fn result_cell_envelope_error(
    db_session_id: &str,
    execution_id: &str,
    sql: &str,
    column: &str,
    result_columns: &[String],
) -> Option<Response> {
    if db_session_id.trim().is_empty() {
        return Some(invalid_argument("dbSessionId is required"));
    }
    if execution_id.trim().is_empty() {
        return Some(invalid_argument("executionId is required"));
    }
    if sql.trim().is_empty() {
        return Some(invalid_argument("sql is required"));
    }
    if column.trim().is_empty() {
        return Some(invalid_argument("column is required"));
    }
    if result_columns.is_empty() {
        return Some(invalid_argument("resultColumns is required"));
    }
    None
}

/// `previewResultCellChange`（POST
/// /api/v1/table-data/result-cell-change-previews）→ 200 `RowChangePlan`
/// （与刀3 preview 同形状：planId / fingerprint / configRevision 十进制
/// 字符串 / updates；不执行任何语句）。
///
/// 闸序（设计档 §3 ①~⑤）：serde → 信封预检 → 前闸（binding 复验 / 会话
/// 上下文比对 / profile+驱动 / 只读预检）→ ④ origin 重读（刀2 读半边同源
/// 梯子：`describe_result_origin` 驱动答不出 → 400 `originUnavailable`；列/
/// 主键半边走 schema cache——miss 即 nothing is provable，裁决只读）→
/// 行定位（产物重读 + 结果集坐标 `rowIndex`；NULL 主键 → 400
/// `rowNotAddressable`）→ 单格 [`PendingRowChange`]（`rowIdentity` 从产物行
/// 的主键列值构造；目标列必须在 passthrough 投影里，主键列写回是 ChangeSet
/// 编辑器的领地——桌面镜像）→ [`preview_changes`]（共享层，与刀3 同一份；
/// 列源 = 刀1 命令面，现查「当下」的列与主键）。
async fn handler_preview_result_cell_change(
    State(state): State<TableDataState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let request: PreviewResultCellBody = match serde_json::from_value(body) {
        Ok(request) => request,
        Err(source) => return invalid_argument(format!("previewResultCellChange: {source}")),
    };
    if let Some(response) = result_cell_envelope_error(
        &request.db_session_id,
        &request.execution_id,
        &request.sql,
        &request.column,
        &request.result_columns,
    ) {
        return response;
    }
    let target = match resolve_result_cell_target(
        &state,
        &identity.ctx,
        &request.db_session_id,
        &request.execution_id,
    )
    .await
    {
        Ok(target) => target,
        Err(response) => return response,
    };
    // ④ origin 重读（写前不信任客户端声明的映射；读半边规则单家）。
    let (driver, handle) = match lend(target.borrowed.lease.connection()) {
        Ok(lent) => lent,
        Err(response) => return response,
    };
    let config = &target.config;
    // sql_target 梯子（origin 端点同形：request 赢过 profile；schema 两个
    // 来源都过驱动的 schema 级声明过滤——无 schema 级的驱动拒绝任何
    // schema 参数）。
    let database = nonblank(request.database.as_deref())
        .or_else(|| nonblank(config.database.as_deref()))
        .map(str::to_owned);
    let schema = nonblank(request.schema.as_deref())
        .filter(|_| driver.has_schema_level())
        .or_else(|| nonblank(config.schema.as_deref()).filter(|_| driver.has_schema_level()))
        .map(str::to_owned);
    let origin = match driver
        .describe_result_origin(
            &handle,
            SqlTarget::new(database.as_deref(), schema.as_deref()),
            &request.result_columns,
            &request.sql,
        )
        .await
    {
        Ok(origin) => origin,
        Err(error) => {
            // 驱动答不出不是本端点的错误：映射 400 + 码表文案（origin 端点
            // 同族；写前重读的拒绝与读半边的裁决同词汇）。
            return api_error_response(ApiError::new(
                ApiErrorCode::InvalidArgument,
                format!(
                    "{}: the driver cannot describe where this result came from ({error})",
                    read_only_error_code(GridReadOnlyReason::UnknownOrigin)
                ),
            ));
        }
    };
    let columns = source_table_columns(
        &state,
        target.borrowed.session.connection_id.as_str(),
        driver.as_ref(),
        &handle,
        config,
        request.database.as_deref(),
        request.schema.as_deref(),
        &origin,
        request.table.as_deref(),
    )
    .await;
    let primary_keys = columns
        .as_ref()
        .map(|columns| columns.primary_keys.clone())
        .unwrap_or_default();
    let verdict = evaluate_result_editability(
        &origin,
        &primary_keys,
        &request.result_columns,
        !config.read_only,
    );
    if !verdict.writable {
        let reason = verdict
            .reason
            .expect("a read-only verdict carries its reason");
        let code = read_only_error_code(reason);
        return api_error_response(ApiError::new(
            ApiErrorCode::InvalidArgument,
            format!(
                "{code}: this result cannot be written back; the origin verdict refused the write"
            ),
        ));
    }
    // 行定位：产物重读 + 结果集坐标（rowIndex 是结果集行序，不是主键值）。
    let chunks = match collect_execution_chunks(&target.runtime, &identity.ctx, &target.view).await
    {
        Ok(chunks) => chunks,
        Err(response) => return response,
    };
    let rows = statement_rows(&chunks, request.statement_index);
    let not_addressable = |message: String| {
        api_error_response(ApiError::new(
            ApiErrorCode::InvalidArgument,
            format!("{}: {message}", ResultErrorCode::RowNotAddressable.as_str()),
        ))
    };
    let row: Vec<Option<datazen_driver_api::Value>> = match rows.get(request.row_index) {
        Some(row) => match serde_json::from_value(row.clone()) {
            Ok(row) => row,
            Err(_) => {
                return not_addressable("the result row payload is not a value row".to_owned());
            }
        },
        None => {
            return not_addressable(format!(
                "the result has no row at rowIndex {} for statementIndex {}",
                request.row_index, request.statement_index
            ));
        }
    };
    // 主键行标识：verdict 的 identity_columns（结果列名 → 真实主键列名）。
    // 全部主键列都在结果里是可写裁决的前置（`KeyNotSelected` 拒掉缺列），
    // 所以逐列取值只会因 NULL 失败。
    let mut row_identity: BTreeMap<String, Option<datazen_driver_api::Value>> = BTreeMap::new();
    for identity in &verdict.identity_columns {
        let index = match request
            .result_columns
            .iter()
            .position(|name| name == &identity.column)
        {
            Some(index) => index,
            None => {
                return not_addressable(format!(
                    "identity column '{}' is missing from the reported result columns",
                    identity.column
                ));
            }
        };
        let value = match row.get(index).cloned().flatten() {
            Some(value) => value,
            None => {
                return not_addressable(
                    "NULL values in the primary key cannot be edited".to_owned(),
                );
            }
        };
        row_identity.insert(identity.source_column.clone(), Some(value));
    }
    // 目标列：必须在 passthrough 投影里（不信任客户端声明的映射）；主键列
    // 写回是 ChangeSet 编辑器的领地（桌面镜像文案）。
    if verdict
        .identity_columns
        .iter()
        .any(|identity| identity.column == request.column)
    {
        return api_error_response(ApiError::new(
            ApiErrorCode::InvalidArgument,
            "Primary key edits require the table ChangeSet editor",
        ));
    }
    let source_column = match verdict
        .editable_columns
        .iter()
        .find(|editable| editable.column == request.column)
    {
        Some(editable) => editable.source_column.clone(),
        None => {
            return api_error_response(ApiError::new(
                ApiErrorCode::InvalidArgument,
                format!(
                    "{}: '{}' is not a passthrough copy of a base-table column; the result column cannot be written",
                    ResultErrorCode::WriteDenied.as_str(),
                    request.column
                ),
            ));
        }
    };
    let old_value = request
        .result_columns
        .iter()
        .position(|name| name == &request.column)
        .and_then(|index| row.get(index).cloned().flatten())
        .unwrap_or(datazen_driver_api::Value::Null);
    // 单格变更 → 共享层计划构建（与刀3 同一份 `build_row_change_plan`；
    // relation.catalog 缺席回落到 origin 读的生效 database/schema——
    // request → profile 梯子，共享层 facts 门按 profile 口径比对）。
    let relation = match verdict.table.clone() {
        Some(relation) => relation,
        None => {
            return api_error_response(ApiError::new(
                ApiErrorCode::InvalidArgument,
                format!(
                    "{}: the writable origin verdict lost its relation",
                    ResultErrorCode::WriteDenied.as_str()
                ),
            ));
        }
    };
    let table_context = RowChangeTableContext {
        connection_id: target.borrowed.session.connection_id.as_str().to_owned(),
        db_session_id: request.db_session_id.clone(),
        driver_type: config.database_type.clone(),
        database: relation.catalog.clone().or_else(|| database.clone()),
        schema: relation.schema.clone().or_else(|| schema.clone()),
        table: relation.name.clone(),
    };
    let change = PendingRowChange {
        row_identity,
        original_values: std::iter::once((source_column.clone(), Some(old_value))).collect(),
        current_values: std::iter::once((source_column.clone(), Some(request.value.clone())))
            .collect(),
        changed_columns: vec![source_column],
        delete_marked: false,
        insert_marked: false,
    };
    let revision = target.revision;
    let column_source = ServerTableColumnSource::new(
        state.runner.clone(),
        state.registry.clone(),
        identity.ctx.clone(),
        revision,
    );
    match preview_changes(
        driver.as_ref(),
        table_context,
        std::slice::from_ref(&change),
        &column_source,
        revision.get(),
    )
    .await
    {
        Ok(plan) => (StatusCode::OK, Json(plan)).into_response(),
        Err(error) => table_editing_error_response(error),
    }
}

/// `commitQueryResultCell`（POST /api/v1/table-data/result-cell-commits）
/// → 200 `CommitPendingChangesResponse`（同步回执；契约 §6.8：结果在响应体
/// 里的用 200）。
///
/// 闸序：serde → manual 明拒（501，先于一切执行触碰）→ 信封预检 → 前闸
/// （同 preview：binding 复验 / 会话上下文比对 / profile+驱动 / 只读预检）
/// → 共享层 [`commit_result_cell_changes`]（五道闸重检 + 原会话
/// `execute_in_session` + 轮询收集 + rowsAffected 两种产物形状兼容）。
/// plan 与 origin 的对应关系由预览链建立、fingerprint 钉住；提交半边复验
/// binding 与五道闸，不重做 origin 重读（设计档 §3 ⑥⑦；宿主不复制共享层
/// 规则）。
async fn handler_commit_query_result_cell(
    State(state): State<TableDataState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let request: CommitResultCellBody = match serde_json::from_value(body) {
        Ok(request) => request,
        Err(source) => return invalid_argument(format!("commitQueryResultCell: {source}")),
    };
    if request.manual_handle.is_some() {
        // 原会话即绑定句柄（server 自持），manual 事务面在本宿主显式
        // closed（501 而非 400：删掉 manualHandle 字段请求即合法，「这个
        // 子能力此宿主尚未提供」是装配事实而不是请求错误；刀3 同款词汇）。
        let error = ApiError::new(
            ApiErrorCode::CapabilityUnsupported,
            "commitQueryResultCell does not serve manual session transactions \
             (manualHandle) on this host yet: the write always runs in the \
             session that produced the result (the original-session binding). \
             Send manualHandle as null or omit the field. No statement ran and \
             nothing was written.",
        );
        return ApiErrorResponse::new(error)
            .with_status(StatusCode::NOT_IMPLEMENTED)
            .into_response();
    }
    if request.db_session_id.trim().is_empty() {
        return invalid_argument("dbSessionId is required");
    }
    if request.execution_id.trim().is_empty() {
        return invalid_argument("executionId is required");
    }
    let target = match resolve_result_cell_target(
        &state,
        &identity.ctx,
        &request.db_session_id,
        &request.execution_id,
    )
    .await
    {
        Ok(target) => target,
        Err(response) => return response,
    };
    let revision = target.revision;
    let facts = ConnectionFacts {
        database_type: target.config.database_type.clone(),
        read_only: target.config.read_only,
        database: target.config.database.clone(),
        schema: target.config.schema.clone(),
        config_revision: revision.get(),
    };
    let shared_request = CommitPendingChangesRequest {
        db_session_id: request.db_session_id,
        plan: request.plan,
        fingerprint: request.fingerprint,
    };
    let artifacts =
        Arc::new(ServerArtifactChunks(target.runtime.clone())) as Arc<dyn ArtifactChunkReader>;
    let columns = ServerTableColumnSource::new(
        state.runner.clone(),
        state.registry.clone(),
        identity.ctx.clone(),
        revision,
    );
    match commit_result_cell_changes(
        target.driver.as_ref(),
        shared_request,
        &facts,
        &columns,
        target.binding.handle.clone(),
        target.runtime.clone(),
        artifacts,
        identity.ctx.clone(),
    )
    .await
    {
        Ok(receipt) => (StatusCode::OK, Json(receipt)).into_response(),
        Err(error) => table_editing_error_response(error),
    }
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
        let (status, body) = wire(TableEditingError::NotFound(
            "session has been released".into(),
        ))
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["code"], "notFound");
    }

    /// `NotConfigured` / `Internal` / `Driver` / `Io` / `Json` → 503
    /// `serviceUnavailable`（用例层故障不是调用方的输入错误）。
    #[tokio::test]
    async fn host_faults_map_to_503_service_unavailable() {
        for (status, body) in [
            wire(TableEditingError::NotConfigured(
                "Driver unavailable".into(),
            ))
            .await,
            wire(TableEditingError::Internal("quota gate failed".into())).await,
            wire(TableEditingError::Driver(
                datazen_driver_api::DriverError::Unsupported("no such command".into()),
            ))
            .await,
            wire(TableEditingError::Io(std::io::Error::other(
                "sink unavailable",
            )))
            .await,
            wire(TableEditingError::Json(
                serde_json::from_str::<serde_json::Value>("{").expect_err("malformed json"),
            ))
            .await,
        ] {
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
            assert_eq!(body["code"], "serviceUnavailable", "{body}");
        }
    }
}
