//! tableData 域的**活句柄面**（W8 刀2）：FK 二（§6.7 DB-07/DB-20）+ 值搜索
//! （§6.7 DB-19）+ 来源重推导（§6.7 DB-14 读半边）四条读端点的真实现。
//! 路由仍由 `table_data.rs` 的 `table_data_router` 挂载，handler 换引用到本
//! 文件；模块头状态表的 ✅ 行随本文件点亮。
//!
//! ## 共享缺口链（为什么四条共用一个借出助手）
//!
//! 四条端点的服务层全部已存在且单家（规则零复制）：
//!
//! - FK 二条吃 `ForeignKeyNavigationSource` 端口（`services/ports.rs`）：
//!   `bind_session` 交出 [`FkSessionBinding`]（driver + handle + database/
//!   schema 档案回退 + guard），共享解析器 `resolve_foreign_key_targets`（三条
//!   驱动往返）与 `list_inbound_foreign_keys`（≤200+1 条 schema 读）拿绑定后
//!   跑全部规则。
//! - 值搜索直接吃活句柄：`services::value_search::search_table_values` 的
//!   第二参就是 `&ConnectionHandle`。
//! - origin 吃驱动契约 + 目录缓存：`describe_result_origin(&handle)` +
//!   [`SchemaCache::get_columns`]（TTL 300s，缓存键第一维 connection_id）+
//!   `evaluate_result_editability`。
//!
//! 它们共同缺的是 server 侧的**活句柄借出**：租约源慢路构造的 runtime 实例
//! 经 [`crate::execution::lease::ServerRuntimeLeaseSource::with_live_source`]
//! 接线 `ServerLiveConnectionSource` 后，[`borrow_live_for_session`] 走
//! `borrow_live_connection_by_id`（授权 → 会话视图 → 实例登记行 → epoch 校验
//! → 端口借出）拿到 `(SessionView, LiveConnectionLease)`。租约是 RAII：显式
//! `release` 与 `Drop` 落同一归还例程，取消/超时/panic/`?` 提前返回全部自动
//! 归还——没有一条异常路径可以绕过它。
//!
//! ## 闸序（estimate 会话面闸序的活句柄版）
//!
//! 每条 handler：①请求体反序列化（共享层 DTO 直接吃 camelCase wire）+ 信封
//! 预检（镜像桌面各命令的 required 文案）→ ②[`borrow_live_for_session`]：
//! 目录可见性闸（越组织/未知 → 404 固定文案，sessions 域防枚举裁决逐字落地）
//! → 单次 `lease.resolve()`（刀0 cell 语义：与开会话的实例是同一个 `Arc`，
//! 「A 请求开的会话 B 请求能借」的根据）→ `borrow_live_connection_by_id`
//! （`ApiError` 直接上 wire：`SessionNotFound`→409、`ConfigRevisionMismatch`→409、
//! 授权拒→403——闸序已把「未知/越组织」收窄成 404，409 只剩真实竞态与配置
//! 漂移，是诚实失败）。
//!
//! ## 错误映射（宿主自持 HTTP 版；桌面 `From<FkNavigationError> for
//! CommandError` 是桌面那份）
//!
//! - FK 二条：`Coded{code,message}` → 400 `invalidArgument`，文案
//!   `{code}: {message}`（`grid.fk.*` 前缀是前端分类契约）；`Session(NotFound)`
//!   → 404；`Session(Unavailable)` → 503（closed 码表里最近的诚实类别，与
//!   `table_read_error_response` 的 Internal 桶同款裁决）。
//! - 值搜索：`ValueSearchError` → 400，文案 `error.to_string()`
//!   （`grid.valueSearch.*` 前缀；桌面 `value_search.rs` 同映射）。
//! - origin：驱动答不出**不是错误**——映射 400，文案携带
//!   `grid.result.originUnavailable`（桌面 `describe_origin` 同族）；目录缓存
//!   `get_columns` 失败收敛为「nothing is provable」，按只读裁决处理（不是错误）。

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use datazen::connection::domain::catalog::ConnectionRepository as _;
use datazen::execution::application::orchestration::{LiveConnectionError, LiveConnectionLease};
use datazen::execution::application::LivePurpose;
use datazen::schema::application::scope::{
    embedded_schema_of, metadata_schema, table_read_database,
};
use datazen::schema::cache::CachedColumns;
use datazen::services::fk_navigation::{FkNavigationError, ResolveForeignKeyTargetsRequest};
use datazen::services::ports::{FkSessionBinding, FkSessionError, ForeignKeyNavigationSource};
use datazen::services::referenced_by::ListInboundForeignKeysRequest;
use datazen::services::value_search::SearchTableValuesRequest;
use datazen::shared::context::RequestContext;
use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::shared::id::DbSessionId;
use datazen_driver_api::{ConnectionHandle, LegacyCatalogReader, SqlTarget};

use crate::http::domains::table_data::{not_visible, profile_unavailable, TableDataState};
use crate::http::router::{api_error_response, invalid_argument, SessionIdentity};

// ── 共享借出助手 ────────────────────────────────────────────────────────────

/// 借出结果：会话视图（连接 id / 配置版本的权威投影）+ 活连接租约（RAII，
/// 掉出作用域即归还）+ 本次借出 resolve 到的 runtime 实例（刀 0 cell 语义：
/// 与开会话的实例是同一个 `Arc`；result-cell 写半边要拿它读执行台账与
/// 产物块——同一请求内不再二次 resolve，「单次 resolve 是构造」的刀 3
/// 裁决沿用到写半边）。
pub(crate) struct BorrowedLive {
    pub session: datazen::execution::SessionView,
    pub lease: LiveConnectionLease,
    /// 本次借出内部 resolve 到的 runtime 实例（与 borrow 走的是同一个
    /// `Arc`，不是第二次 resolve）。
    pub runtime: Arc<datazen::execution::application::orchestration::RuntimeConnectionUseCases>,
}

/// 四条读端点共用的活句柄借出路径（estimate 闸序的活句柄版）：
///
/// 1. **目录可见性闸**：`lookup_owner` miss 或越组织 → 404 固定文案
///    （sessions 域防枚举裁决逐字落地——不区分「不存在」与「不可见」）；
/// 2. **单次 resolve**（err → 503，`LeaseConnectionUseCases` 同款）；
/// 3. **借出**：`borrow_live_connection_by_id`（授权 → 会话视图 → 实例登记行
///    → epoch 校验 → 端口借出；登记与签发同成败）。
pub(crate) async fn borrow_live_for_session(
    state: &TableDataState,
    ctx: &RequestContext,
    db_session_id: &str,
) -> Result<BorrowedLive, Response> {
    // 可见性先于会话存活（sessions 域裁决逐字落地：越组织/未知 → 404 固定
    // 文案，防 ID 枚举）。
    let owner = match state
        .directory
        .lookup_owner(DbSessionId::new(db_session_id.to_owned()))
        .await
    {
        Ok(Some(owner)) => owner,
        Ok(None) => return Err(not_visible()),
        Err(source) => {
            return Err(api_error_response(ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                format!("session directory unavailable: {source}"),
            )));
        }
    };
    if owner.organization_id != ctx.organization_id {
        return Err(not_visible());
    }
    let runtime = state.lease.resolve().await.map_err(|message| {
        api_error_response(ApiError::new(ApiErrorCode::ServiceUnavailable, message))
    })?;
    let (session, lease) = runtime
        .borrow_live_connection_by_id(ctx, db_session_id, LivePurpose::CommandExecution)
        .await
        .map_err(api_error_response)?;
    Ok(BorrowedLive {
        session,
        lease,
        runtime,
    })
}

/// 取出借用的 `(driver, handle)`。借出刚刚成功，`connection()` 在这里拒绝只
/// 可能是「借用窗内会话被关闭/回收」（`Revoked`）——错误映射沿
/// `convert::live` 逐臂同码表（SessionLost 桶 → 409；NotWired/Refused 桶 →
/// 503），宁可让调用方走执行路径拿 `SessionLost`，不许拿看似可用的句柄打
/// 驱动。
pub(crate) fn lend(
    connection: Result<&datazen::execution::application::LiveConnection, LiveConnectionError>,
) -> Result<
    (
        Arc<dyn datazen_driver_api::DatabaseDriver>,
        ConnectionHandle,
    ),
    Response,
> {
    let connection = connection.map_err(|error| {
        let code = match &error {
            LiveConnectionError::SessionLost(_)
            | LiveConnectionError::RuntimeEpochMismatch
            | LiveConnectionError::Revoked => ApiErrorCode::SessionLost,
            LiveConnectionError::NotWired | LiveConnectionError::Refused(_) => {
                ApiErrorCode::ServiceUnavailable
            }
        };
        api_error_response(ApiError::new(code, "live connection port operation failed"))
    })?;
    Ok((Arc::clone(connection.driver()), connection.handle().clone()))
}

/// 连接目录读取（值搜索/origin 的档案回退源）：miss → `profile_unavailable`
/// 404（防枚举口径与读端点同一条）；目录故障 → 503。
pub(crate) async fn connection_config_of(
    state: &TableDataState,
    ctx: &RequestContext,
    connection_id: &str,
) -> Result<datazen_driver_api::ConnectionConfig, Response> {
    let scope = datazen::connection::domain::ConnectionScope::new(ctx.organization_id.as_str());
    match state.connections.get(&scope, connection_id).await {
        Ok(Some(config)) => Ok(config),
        Ok(None) => Err(profile_unavailable()),
        Err(error) => Err(api_error_response(ApiError::new(
            ApiErrorCode::ServiceUnavailable,
            error.to_string(),
        ))),
    }
}

// ── FK 二条：ServerForeignKeyNavigationSource ──────────────────────────────

/// 把共享 FK 解析器的 `bind_session` 落到 server 的借出路径（桌面
/// `HostNavigationSource` 的 server 同构投影）：resolve 当前 runtime →
/// borrow 活句柄 → 档案回退 → 绑定。guard 是**租约本体**
/// （`Box::new(lease)`）——绑定活过解析器的全部驱动往返，函数返回即绑定
/// drop → lease drop → 归还（端口文档的硬要求「binding must outlive every
/// driver call」由「guard 就是租约本体」结构性满足）。
pub(crate) struct ServerForeignKeyNavigationSource {
    lease: Arc<dyn crate::execution::lease::RuntimeLeaseSource>,
    connections: Arc<dyn datazen::connection::domain::catalog::ConnectionRepository>,
    scope: datazen::connection::domain::ConnectionScope,
    ctx: RequestContext,
}

impl ServerForeignKeyNavigationSource {
    /// 按请求构造（ctx 是每请求事实；connections/scope 与域状态同实例）。
    pub fn new(state: &TableDataState, ctx: RequestContext) -> Self {
        Self {
            lease: state.lease.clone(),
            connections: state.connections.clone(),
            scope: datazen::connection::domain::ConnectionScope::new(ctx.organization_id.as_str()),
            ctx,
        }
    }
}

#[async_trait::async_trait]
impl ForeignKeyNavigationSource for ServerForeignKeyNavigationSource {
    async fn bind_session(&self, db_session_id: &str) -> Result<FkSessionBinding, FkSessionError> {
        let runtime = self
            .lease
            .resolve()
            .await
            .map_err(FkSessionError::Unavailable)?;
        let (session, lease) = runtime
            .borrow_live_connection_by_id(&self.ctx, db_session_id, LivePurpose::CommandExecution)
            .await
            .map_err(|error| {
                // 桌面 resolve.rs 映射同族：会话不在 → NotFound；其余（授权
                // 拒/配置漂移/端口故障）→ Unavailable。
                if matches!(
                    error.code,
                    ApiErrorCode::SessionNotFound | ApiErrorCode::NotFound
                ) {
                    FkSessionError::NotFound(format!("DB session {db_session_id} is unavailable"))
                } else {
                    FkSessionError::Unavailable(error.to_string())
                }
            })?;
        let connection = lease
            .connection()
            .map_err(|error| FkSessionError::Unavailable(error.to_string()))?;
        let config = self
            .connections
            .get(&self.scope, session.connection_id.as_str())
            .await
            .map_err(|error| FkSessionError::Unavailable(error.to_string()))?
            .ok_or_else(|| {
                FkSessionError::NotFound("connection profile is unavailable".to_string())
            })?;
        Ok(FkSessionBinding {
            driver: Arc::clone(connection.driver()),
            handle: connection.handle().clone(),
            database: config.database.clone(),
            schema: config.schema.clone(),
            // The lease keeps the live connection borrowed while the resolver
            // reads the catalog and probes the target table.
            guard: Box::new(lease),
        })
    }
}

/// `FkNavigationError` → wire 映射（桌面 `commands/error.rs` 逐臂的 server
/// 镜像）：`grid.fk.*` 码保持消息第一词（前端按前缀分类）；会话失败不是导航
/// 规则失败，不带 grid 前缀。
fn fk_navigation_error_response(error: FkNavigationError) -> Response {
    match error {
        FkNavigationError::Coded { code, message } => api_error_response(ApiError::new(
            ApiErrorCode::InvalidArgument,
            format!("{code}: {message}"),
        )),
        FkNavigationError::Session(FkSessionError::NotFound(message)) => {
            api_error_response(ApiError::new(ApiErrorCode::NotFound, message))
        }
        FkNavigationError::Session(FkSessionError::Unavailable(message)) => {
            api_error_response(ApiError::new(ApiErrorCode::ServiceUnavailable, message))
        }
    }
}

// ── 四条 handler ───────────────────────────────────────────────────────────

/// `resolveForeignKeyTargets`（POST /api/v1/table-data/foreign-key-targets）
/// → 200 `ForeignKeyTargetResolution`（`exists:false` 是成功答案——目标表
/// 可读但引用行已不在，不是错误）。
///
/// 闸序：serde 解析（共享层 DTO camelCase）→ 信封预检（桌面
/// `fk_navigation.rs` 同文案）→ 共享借出助手 → `resolve_foreign_key_targets`
/// 原样跑（三条往返共享一次绑定）。
pub(crate) async fn handler_resolve_foreign_key_targets(
    State(state): State<TableDataState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let request: ResolveForeignKeyTargetsRequest = match serde_json::from_value(body) {
        Ok(request) => request,
        Err(source) => return invalid_argument(format!("resolveForeignKeyTargets: {source}")),
    };
    if request.db_session_id.trim().is_empty() {
        return invalid_argument("dbSessionId is required");
    }
    if request.table.trim().is_empty() {
        return invalid_argument("table is required");
    }
    let source = ServerForeignKeyNavigationSource::new(&state, identity.ctx.clone());
    match datazen::services::fk_navigation::resolve_foreign_key_targets(&source, &request).await {
        Ok(resolution) => (StatusCode::OK, Json(resolution)).into_response(),
        Err(error) => fk_navigation_error_response(error),
    }
}

/// `listInboundForeignKeys`（POST /api/v1/table-data/inbound-foreign-keys）
/// → 200 `InboundForeignKeyCatalog`（最长活句柄持有面：≤200 次 schema 读，
/// `truncated` 如实报告早停）。
///
/// 与 FK 跳转**同一个**端口**同一个**适配器（评估档 §4 表第 2 行「零边际」）。
pub(crate) async fn handler_list_inbound_foreign_keys(
    State(state): State<TableDataState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let request: ListInboundForeignKeysRequest = match serde_json::from_value(body) {
        Ok(request) => request,
        Err(source) => return invalid_argument(format!("listInboundForeignKeys: {source}")),
    };
    if request.db_session_id.trim().is_empty() {
        return invalid_argument("dbSessionId is required");
    }
    if request.table.trim().is_empty() {
        return invalid_argument("table is required");
    }
    let source = ServerForeignKeyNavigationSource::new(&state, identity.ctx.clone());
    match datazen::services::referenced_by::list_inbound_foreign_keys(&source, &request).await {
        Ok(catalog) => (StatusCode::OK, Json(catalog)).into_response(),
        Err(error) => fk_navigation_error_response(error),
    }
}

/// `searchTableValues`（POST /api/v1/table-data/value-searches）→ 200
/// `SearchTableValuesResult`。
///
/// 直接在 handler 里编排（此服务吃裸 `(driver, handle)`，无宿主端口）：
/// 借出 → 档案回退 → database 梯子（`table_read_database`——estimate 同款
/// 共享梯子，request → config → `"default"`）→ schema 梯子（`metadata_schema`）
/// → 共享 `search_table_values` 原样跑（语句构建/LIKE 转义/绑定或内联/计数
/// 解析全在共享层）。扫描节奏在前端（一表一命令），host 永不自行迭代。
pub(crate) async fn handler_search_table_values(
    State(state): State<TableDataState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let request: SearchTableValuesRequest = match serde_json::from_value(body) {
        Ok(request) => request,
        Err(source) => return invalid_argument(format!("searchTableValues: {source}")),
    };
    if request.table.trim().is_empty() {
        return invalid_argument("table is required");
    }
    if request.value.trim().is_empty() {
        return invalid_argument("value is required");
    }
    let borrowed =
        match borrow_live_for_session(&state, &identity.ctx, &request.db_session_id).await {
            Ok(borrowed) => borrowed,
            Err(response) => return response,
        };
    let (driver, handle) = match lend(borrowed.lease.connection()) {
        Ok(lent) => lent,
        Err(response) => return response,
    };
    let config = match connection_config_of(
        &state,
        &identity.ctx,
        borrowed.session.connection_id.as_str(),
    )
    .await
    {
        Ok(config) => config,
        Err(response) => return response,
    };
    // 梯子与 estimate 爬同一份共享实现（schema::application::scope）。
    let database = table_read_database(request.database.as_deref(), config.database.as_deref());
    let schema = metadata_schema(
        driver.as_ref(),
        request.schema.as_deref(),
        embedded_schema_of(&request.table),
        config.schema.as_deref(),
    );
    match datazen::services::value_search::search_table_values(
        driver.as_ref(),
        &handle,
        database,
        schema.as_deref(),
        &request,
    )
    .await
    {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(error) => api_error_response(ApiError::new(
            ApiErrorCode::InvalidArgument,
            error.to_string(),
        )),
    }
}

/// `resolveResultOrigin` 的请求体（桌面 `ResolveResultOriginRequest` 的
/// HTTP 镜像；该 DTO 是桌面命令的 IPC 类型不在共享层，server 域文件自持
/// 一份同字段反序列化结构）。
#[derive(Deserialize)]
pub(crate) struct ResolveResultOriginBody {
    /// The session the result was read from. The connection is **not** part
    /// of the request: it is resolved from the session.
    pub db_session_id: String,
    /// The database the result was read from.
    pub database: Option<String>,
    /// The schema the result was read from.
    pub schema: Option<String>,
    /// The statement the result came from.
    pub sql: String,
    /// The column names the executed result reports.
    pub result_columns: Vec<String>,
    /// The real name of the relation the result was read from, as the narrow
    /// host-side mapping check named it.
    pub table: Option<String>,
}

/// `resolveResultOrigin`（POST /api/v1/table-data/result-origins）→ 200
/// `ResultEditability`（只读裁决带 `reason` 也是 200 成功答案——verdict plus
/// a reason，never a bare no）。
///
/// 编排镜像桌面 `resolve_result_origin` 六步：借出 → 连接档案 → `sql_target`
/// 梯子（两处都过 `has_schema_level()` 过滤）→ `describe_result_origin`（驱动
/// 答不出不是错误：映射 400 + `grid.result.originUnavailable`）→ 列/主键半边
/// 走 `state.schema_cache`（None = 目录答不出，按「nothing is provable」只读
/// 处理）→ `evaluate_result_editability`。规则（origin 分析、可写性判定、码
/// 表）单家在 `datazen-driver-api::result_origin` 与 `table_editing::result_edit`，
/// 宿主只借会话与读缓存，无规则复制。
pub(crate) async fn handler_resolve_result_origin(
    State(state): State<TableDataState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let request: ResolveResultOriginBody = match serde_json::from_value(body) {
        Ok(request) => request,
        Err(source) => return invalid_argument(format!("resolveResultOrigin: {source}")),
    };
    if request.db_session_id.trim().is_empty() {
        return invalid_argument("dbSessionId is required");
    }
    if request.sql.trim().is_empty() {
        return invalid_argument("sql is required");
    }
    let borrowed =
        match borrow_live_for_session(&state, &identity.ctx, &request.db_session_id).await {
            Ok(borrowed) => borrowed,
            Err(response) => return response,
        };
    let (driver, handle) = match lend(borrowed.lease.connection()) {
        Ok(lent) => lent,
        Err(response) => return response,
    };
    let config = match connection_config_of(
        &state,
        &identity.ctx,
        borrowed.session.connection_id.as_str(),
    )
    .await
    {
        Ok(config) => config,
        Err(response) => return response,
    };
    // sql_target 梯子（桌面 `result_origin.rs:255-271` 同形）：request 赢过
    // profile；schema 两个来源都过驱动的 schema 级声明过滤（无 schema 级的
    // 驱动拒绝任何 schema 参数）。
    let database = nonblank(request.database.as_deref())
        .or_else(|| nonblank(config.database.as_deref()))
        .map(str::to_owned);
    let schema = nonblank(request.schema.as_deref())
        .filter(|_| driver.has_schema_level())
        .or_else(|| nonblank(config.schema.as_deref()).filter(|_| driver.has_schema_level()))
        .map(str::to_owned);
    let target = SqlTarget::new(database.as_deref(), schema.as_deref());
    let origin = match driver
        .describe_result_origin(&handle, target, &request.result_columns, &request.sql)
        .await
    {
        Ok(origin) => origin,
        Err(error) => {
            // A driver that cannot answer is not an error of this endpoint:
            // the verdict says "read-only" and the reason says why（桌面
            // `describe_origin` 同族文案）。
            return api_error_response(ApiError::new(
                ApiErrorCode::InvalidArgument,
                format!(
                    "{}: the driver cannot describe where this result came from ({error})",
                    datazen::table_editing::result_edit::read_only_error_code(
                        datazen::table_editing::result_edit::GridReadOnlyReason::UnknownOrigin
                    )
                ),
            ));
        }
    };
    let columns = source_table_columns(
        &state,
        borrowed.session.connection_id.as_str(),
        driver.as_ref(),
        &handle,
        &config,
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
    let verdict = datazen::table_editing::result_edit::evaluate_result_editability(
        &origin,
        &primary_keys,
        &request.result_columns,
        !config.read_only,
    );
    (StatusCode::OK, Json(verdict)).into_response()
}

/// Read the source table's columns — and with them its key — from the catalog
/// cache（桌面 `source_table_columns` 的 server 同构投影）。
///
/// `None` means the catalog could not answer. The caller treats that as
/// "nothing is provable", never as "the table has no key".
///
/// `pub(crate)`：刀 4 的 result-cell 写半边（`table_data.rs`）写前 origin
/// 重读走同一份规则（列/主键半边单一事实源，不复制）。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn source_table_columns(
    state: &TableDataState,
    connection_id: &str,
    driver: &dyn datazen_driver_api::DatabaseDriver,
    handle: &ConnectionHandle,
    config: &datazen_driver_api::ConnectionConfig,
    database: Option<&str>,
    schema: Option<&str>,
    origin: &datazen_driver_api::ResultOrigin,
    narrow_table: Option<&str>,
) -> Option<CachedColumns> {
    // The driver's origin names the relation when it can; the narrow mapping
    // check names it when it cannot. Either way the catalog is the authority
    // for the key columns.
    let table = origin_table_name(origin)
        .or_else(|| narrow_table.map(str::to_string))
        .filter(|name| !name.trim().is_empty())?;
    let database = nonblank(database).or_else(|| nonblank(config.database.as_deref()))?;
    // A driver without a schema level rejects any schema argument instead of
    // ignoring it, so the effective schema comes from the driver's own
    // declaration first.
    let schema = nonblank(schema)
        .filter(|_| driver.has_schema_level())
        .or_else(|| nonblank(config.schema.as_deref()).filter(|_| driver.has_schema_level()));
    let catalog = LegacyCatalogReader::new(driver);
    state
        .schema_cache
        .get_columns(connection_id, database, schema, &table, &catalog, handle)
        .await
        .ok()
}

/// The relation the result came from, when the driver named one（桌面
/// `origin_table_name` 同形：Determined 分支取 `detail.table.name`）。
fn origin_table_name(origin: &datazen_driver_api::ResultOrigin) -> Option<String> {
    match origin {
        datazen_driver_api::ResultOrigin::Determined(detail) => detail
            .table
            .as_ref()
            .map(|table| table.name.clone())
            .filter(|name| !name.trim().is_empty()),
        datazen_driver_api::ResultOrigin::Undeterminable(_) => None,
    }
}

/// 归一化可空字符串：去空白，空白等同缺省（桌面 `nonblank` 同形）。
/// `pub(crate)`：刀 4 的 result-cell 写半边共用同一条梯子语义。
pub(crate) fn nonblank(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
