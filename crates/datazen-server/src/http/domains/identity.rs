//! identity 域路由（契约 §6.2 的 HTTP 薄绑定）。
//!
//! | 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
//! | --- | --- | --- | --- | --- |
//! | `getPlatformIdentity` | `GET /api/v1/identity` | — | 200 | — → `{ clientInstanceId, organizationId, principalId }` |
//!
//! `POST /api/v1/submission-tokens`（同节的令牌签发行）已在
//! `http::router` 点亮（`handler_issue_token`），本文件**不**重复挂载。
//!
//! ## §6.2 的两形态语义裁决（登记）
//!
//! 契约 §6.2 明文：「桌面形态的 `getPlatformIdentity` 返回本地单机身份；
//! web 形态返回登录主体——**同一方法、两形态语义已在域客户端设计时裁
//! 决**，HTTP 侧不再分叉」。本宿主是 web 形态，因此：
//!
//! - `organizationId` / `principalId` = L2 认证链的会话行事实
//!   （[`SessionIdentity`] 的 ctx；INV-S1：请求体/查询串永不生效）；
//! - `clientInstanceId` = 服务端实例的 client 实例 id，v1 固定常量
//!   `"http"`——与 [`SessionIdentity`] 组装 `RequestContext` 时盖章的
//!   `ClientInstanceId` **同字面**（单值来源：router.rs 的提取器）。
//!   服务端是多主体宿主，不存在桌面那种「每安装一个实例 id」的语义；
//!   若后续接入多实例登记（web 导出流立项时），改动点只有这一处常量
//!   与提取器，wire 形状不变。

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use crate::http::router::SessionIdentity;

/// 服务端实例的 client 实例 id（§6.2 web 形态裁决；与 `SessionIdentity`
/// 盖章的 `ClientInstanceId` 同字面——两处常量必须同步修改）。
pub const SERVER_CLIENT_INSTANCE_ID: &str = "http";

/// identity 域 Router 片段：`GET /api/v1/identity`。handler 无状态依赖
/// （身份即全部事实），不需要 State。
pub fn identity_router() -> Router {
    Router::new().route("/api/v1/identity", get(handler_get_identity))
}

/// `getPlatformIdentity`（§6.2）：认证链三元组的回显。未认证请求在
/// [`SessionIdentity`] 提取处已被拒（401 `unauthenticated`）。
async fn handler_get_identity(identity: SessionIdentity) -> Response {
    (
        StatusCode::OK,
        Json(json!({
            "clientInstanceId": identity.ctx.client_instance_id.as_str(),
            "organizationId": identity.ctx.organization_id.as_str(),
            "principalId": identity.ctx.principal_id.as_str(),
        })),
    )
        .into_response()
}
