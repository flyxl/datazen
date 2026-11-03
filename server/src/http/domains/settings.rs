//! settings 域路由（契约 §6.1 的 HTTP 薄绑定）。
//!
//! | 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
//! | --- | --- | --- | --- | --- |
//! | `getSettings` | `GET /api/v1/settings` | — | 200 | — → `SharedSettings` |
//! | `saveSettings` | `PUT /api/v1/settings` | — | 200 | `{ settings }` → `SharedSettings` |
//!
//! 域裁决（契约 §6.1 全文逐字对照）：
//!
//! - **只服务共享契约**：请求/响应类型是 `datazen::settings::SharedSettings`
//!   ——Web 形态只服务 `WEB_SETTINGS_SECTIONS`（general / appearance /
//!   dataBrowsing / editor / behavior）对应的共享字段；桌面专属分节
//!   （updater / logPath / grid 外观等宿主字段）**不上 HTTP 面**。`SharedSettings`
//!   的 `deny_unknown_fields` 是这条边界的第一道闸：携带宿主分节的 PUT
//!   在反序列化处被拒（400 `invalidArgument`），不做静默剥离——剥离会把
//!   「客户端以为保存了」和「存储里没有」变成静默分歧。
//! - **不要求幂等令牌**（§6.1 明文）：整体覆盖写（last-write-wins），
//!   handler 不读 `submissionToken`。
//! - **错误 wire 的登记**：settings 域没有 application 契约码（
//!   `SettingsError` 无 code）；本宿主用共享错误词汇落地——校验拒绝 →
//!   `invalidArgument`（400），存储故障 → `serviceUnavailable`（503）。
//!   与 WR-C3 契约腿（`tests/use_case_contracts.rs`）的 `{code: null}`
//!   装配裁决相对照：那条腿 pin 的是它自建 router 的形态，本文件是
//!   真实宿主的统一 `ApiError` wire（`http::response` 单处定义）。
//! - **组织边界**：`SettingsRepository` 端口无 scope 参数（known-hosts
//!   同款「每组织一个实例」口径），service 按 `SessionIdentity` 的组织
//!   逐请求装配（INV-01：组织只来自认证链，不来自请求体）。

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use axum::{Json, Router};
use serde_json::Value;

use crate::storage::ServerSettingsStore;
use datazen::settings::{SettingsError, SettingsService, SharedSettings};

use crate::http::router::{api_error_response, invalid_argument, SessionIdentity};
use crate::storage::AdminStore;

/// settings 域的装配状态：管理库句柄 + 每请求按会话组织装配 service。
#[derive(Clone)]
pub struct SettingsState {
    /// 管理库（`server_settings` 表，v8 迁移；调用方负责先 `migrate`）。
    pub store: Arc<AdminStore>,
}

impl SettingsState {
    /// 按会话组织的 `SettingsService` 装配（端口「每组织一个仓储实例」
    /// 口径；组织来自认证链事实）。
    fn service_for(&self, organization_id: &str) -> SettingsService {
        SettingsService::new(Arc::new(ServerSettingsStore::scoped(
            organization_id,
            self.store.clone(),
        )))
    }
}

/// settings 域 Router 片段：`GET/PUT /api/v1/settings`。主线把它 merge 进
/// `build_router_with_auth` 的认证链（L2–L6 之内）。
pub fn settings_router(state: SettingsState) -> Router {
    Router::new()
        .route(
            "/api/v1/settings",
            get(handler_get_settings).put(handler_put_settings),
        )
        .with_state(state)
}

/// 本域的 `SettingsError` → wire 映射（见模块头的错误 wire 登记）。
fn settings_error_response(error: SettingsError) -> Response {
    match error {
        SettingsError::Validation(message) => invalid_argument(message),
        SettingsError::Repository(source) => api_error_response(
            datazen::shared::error::ApiError::new(
                datazen::shared::error::ApiErrorCode::ServiceUnavailable,
                source.to_string(),
            ),
        ),
    }
}

/// `getSettings`（§6.1：GET /api/v1/settings）→ 200 `SharedSettings`。
///
/// 空库按缺省文档回答（fresh install 语义，`storage::settings` 的缺省
/// 物化；桌面专属分节不在文档里，也就不出现在响应里）。
async fn handler_get_settings(State(state): State<SettingsState>, identity: SessionIdentity) -> Response {
    let service = state.service_for(identity.ctx.organization_id.as_str());
    match service.read().await {
        Ok(settings) => (StatusCode::OK, Json(settings)).into_response(),
        Err(error) => settings_error_response(error),
    }
}

/// `saveSettings`（§6.1：PUT /api/v1/settings，体 `{ settings }`）→ 200。
///
/// 保存返回 normalize 后的文档（application 语义：越界池尺寸被夹回、
/// 非法枚举在落库前被拒）；响应即存储事实，客户端回读零分歧。
async fn handler_put_settings(
    State(state): State<SettingsState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let Some(raw) = body.get("settings").cloned() else {
        return invalid_argument("settings is required.");
    };
    let draft: SharedSettings = match serde_json::from_value(raw) {
        Ok(draft) => draft,
        Err(source) => {
            // deny_unknown_fields 的拒绝文案点名冒分的分节键——桌面专属
            // 分节在这条路径上被拒（见模块头）。
            return invalid_argument(format!("settings: {source}"));
        }
    };
    let service = state.service_for(identity.ctx.organization_id.as_str());
    match service.save(draft).await {
        Ok(settings) => (StatusCode::OK, Json(settings)).into_response(),
        Err(error) => settings_error_response(error),
    }
}
