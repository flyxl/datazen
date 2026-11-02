//! L4：CSRF 校验（team-server §5 的中间件半边；纯逻辑在 [`crate::csrf`]）。
//!
//! 适用范围（§5.2）：**cookie 认证的** `POST` / `PUT` / `PATCH` / `DELETE`。
//! 豁免：
//!
//! - `GET` / `HEAD`（安全读取）与本链不服务的其他方法；
//! - `/api/v1/events/{streamId}` 的 `GET`（SSE——EventSource 无法设自定义
//!   头，握手改用 Origin/同源证明，§8.1；该路由随 WR-D5 点亮）；
//! - `/api/v1/auth/*`（与 L2 同一豁免口径，`super::auth_exempt`）；
//! - P8 native 已认证 API 不使用 cookie CSRF（§5.2）——native 通道未启用，
//!   无豁免面。
//!
//! 失败行为（§5.3）：403 `PermissionDenied`（`retryDisposition = never`），
//! 响应体不新增区分字段；**审计事件 `csrf_rejected`** 带 `requestId`、
//! 来源 `Origin` 与路由（白名单键 `payload.request`，§5.3 的显式要求）。
//! 审计写失败不吞掉业务响应，但打告警日志（§3.2 A 行的宿主处置）。
//!
//! 校验三件事（§5.1）在 [`crate::csrf::verify`] 里闭合；本层提供的输入：
//! cookie 半边、header 半边、当前登录会话 id（L2 注入的扩展）、会话存活
//! 标记（L2 已在**本请求**校验过会话行，`true` 是事实而非假设）。

use axum::extract::{Request, State};
use axum::http::{header, Method};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use datazen::shared::audit::{AuditEvent, AuditOutcome, AuditPayload, AuditPrincipalKind, AuditRequestContext};
use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::shared::id::{OrganizationId, PrincipalId, RequestId, Timestamp};

use crate::csrf::{self, CsrfRejection, CSRF_COOKIE, CSRF_HEADER};
use crate::http::middleware::auth::AuthenticatedSession;
use crate::http::middleware::{auth_exempt, now_rfc3339, session_cookie, under_api, AuthChainState};
use crate::http::response::ApiErrorResponse;

/// 审计动作字面量（team-server §5.3 逐字：`csrf_rejected`）。
pub const AUDIT_ACTION: &str = "csrf_rejected";

/// L4 中间件本体。挂载点：`build_router_with_auth`，L3 之内、L6 之外。
pub(crate) async fn guard(
    State(state): State<AuthChainState>,
    req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_owned();
    let method = req.method().clone();
    if !under_api(&path) || auth_exempt(&path) || is_safe(&method) {
        return next.run(req).await;
    }

    // L2 已放行才可能到这里；扩展缺失说明装配错误（L4 在 L2 内层却拿到
    // 了无会话的请求）——fail-closed 拒绝，不猜测身份。
    let Some(session) = req
        .extensions()
        .get::<AuthenticatedSession>()
        .cloned()
    else {
        return forbidden(None);
    };

    let cookie_token = session_cookie(req.headers(), CSRF_COOKIE);
    let header_token = req
        .headers()
        .get(CSRF_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let request_id = req.extensions().get::<RequestId>().cloned();

    let verification = match (cookie_token.as_deref(), header_token.as_deref()) {
        (Some(cookie_token), Some(header_token)) => {
            // 会话存活标记：L2 在**本请求**刚校验过会话行（撤销/过期的
            // 会话到不了这里），`true` 是已验证的事实（§5.1 校验项 3 的
            // 层序实现，见 `crate::csrf` 模块头）。
            csrf::verify(&state.csrf, &session.session_id, cookie_token, header_token, true)
        }
        // 任一半边缺失 = 双提交不成立（§5.1 校验项 1 的入口检查）。
        _ => Err(CsrfRejection::Malformed),
    };

    if let Err(rejection) = verification {
        record_csrf_rejected(&state, &session, request_id.clone(), &path, origin.as_deref());
        tracing::debug!(?rejection, route = %path, "csrf rejected");
        return forbidden(request_id);
    }

    next.run(req).await
}

/// 安全方法（§5.2 的第一行）：不携带 CSRF 令牌，但仍受 L2/L6 约束。
fn is_safe(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS | Method::TRACE)
}

/// 403 响应（§5.3：`PermissionDenied` / never，响应体不新增区分字段——
/// 拒绝原因只进日志与审计）。
fn forbidden(request_id: Option<RequestId>) -> Response {
    ApiErrorResponse::new(
        ApiError::new(ApiErrorCode::PermissionDenied, "The request was rejected.")
            .with_request_id(request_id),
    )
    .into_response()
}

/// `csrf_rejected` 审计（§5.3）。写失败不吞业务响应，但必须告警（§3.2）。
fn record_csrf_rejected(
    state: &AuthChainState,
    session: &AuthenticatedSession,
    request_id: Option<RequestId>,
    route: &str,
    origin: Option<&str>,
) {
    let event = AuditEvent {
        principal_id: Some(PrincipalId::new(&session.principal_id)),
        request_id,
        payload: AuditPayload {
            request: Some(AuditRequestContext {
                route: route.to_owned(),
                origin: origin.map(str::to_owned),
            }),
            ..AuditPayload::empty()
        },
        ..AuditEvent::new(
            OrganizationId::new(&session.organization_id),
            AuditPrincipalKind::User,
            AUDIT_ACTION,
            AuditOutcome::Denied,
            Timestamp::new(now_rfc3339()),
        )
    };
    if let Err(error) = state.audit.record(event) {
        // §3.2 A 行：审计写失败不吞掉业务响应，但触发告警指标。
        tracing::warn!(%error, "csrf_rejected audit append failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 适用范围钉子：安全方法不校验 CSRF（§5.2），写方法全部在内。
    #[test]
    fn only_unsafe_methods_are_csrf_gated() {
        assert!(is_safe(&Method::GET));
        assert!(is_safe(&Method::HEAD));
        assert!(is_safe(&Method::OPTIONS));
        assert!(is_safe(&Method::TRACE));
        assert!(!is_safe(&Method::POST));
        assert!(!is_safe(&Method::PUT));
        assert!(!is_safe(&Method::PATCH));
        assert!(!is_safe(&Method::DELETE));
    }

    /// 403 的 wire 形状：`PermissionDenied` → 403（§9.2 映射表）。
    #[test]
    fn the_forbidden_response_is_a_403() {
        let response = forbidden(Some(RequestId::new("req-9")));
        assert_eq!(response.status(), axum::http::StatusCode::FORBIDDEN);
    }
}
