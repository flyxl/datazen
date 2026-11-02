//! L2：认证（team-server §3.2 的 L2 行 + §4.3/§4.4 的会话与 cookie 语义）。
//!
//! 职责（§3.2）：解析 `__Host-dz_session` cookie → 查会话行并校验
//! 到期/撤销 → 校验通过后把**会话事实**注入请求扩展。失败行为：
//!
//! - 无/坏 cookie、过期、已撤销 → **401 `Unauthenticated`**，脱敏文案
//!   不说明会话是否存在或为何失效（§3.2：「不说明会话是否存在」）；
//! - 管理库不可达 → **503 `ServiceUnavailable`**（`ValidateFailure` 的
//!   分立裁决：fail-closed 不等于把基础设施故障伪装成认证失败）；
//! - native 会话出现在 cookie 通道 → 401（§4.6 的 L2 互斥：native 请求
//!   只接受 native 凭据 `Authorization: Bearer`，本刀未启用该通道）。
//!
//! 会话校验**顺带完成空闲顺延**（`LoginSessions::validate` 的 touch）：
//! §4.3 规定 `idle_expires_at` 只由入站请求顺延，L2 是唯一逐请求必经的
//! 入站校验点。被拒请求不产生写副作用（拒绝分支不 touch）。
//!
//! `/api/` 之外的路径与 `/api/v1/auth/*` 豁免（§3.3）——判定在
//! [`super::under_api`] / [`super::auth_exempt`]。

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::shared::id::{RequestId, Timestamp};

use crate::http::middleware::{auth_exempt, now_rfc3339, session_cookie, under_api, AuthChainState};
use crate::http::response::ApiErrorResponse;
use crate::storage::login_sessions::LoginSessionMode;
use crate::storage::ValidateFailure;

/// 会话 cookie 名（§4.4：`__Host-` 前缀强制 `Secure`、无 `Domain`、
/// `Path=/`；`HttpOnly` + `SameSite=Lax`）。前缀由部署侧同源 HTTPS 保证，
/// 本层只按名读取。
pub const SESSION_COOKIE: &str = "__Host-dz_session";

/// L2 校验通过后注入请求扩展的**登录会话事实**。
///
/// 命名说明：这是登录会话（§4.3 的服务端行），不是连接域的运行时
/// `SessionHandle`（那是请求体里的运行时句柄，`session` 层的提取对象）。
/// 下游（L4/L6 与未来 handler）从这里取组织与主体——INV-S1：身份只来自
/// 会话行，请求体中的 `organizationId`/`principalId` 永不生效。
#[derive(Debug, Clone)]
pub struct AuthenticatedSession {
    pub organization_id: String,
    pub principal_id: String,
    /// 不透明会话 id（CSRF 绑定与登出撤销的主键；不是 `dbSessionId`）。
    pub session_id: String,
    /// 建立会话时的组织权限版本快照（§6.2/§6.3 撤销传播比对的基准）。
    pub permission_version: i64,
    /// 撤销时自增的令牌纪元（§4.3）。
    pub token_epoch: i64,
    pub auth_mode: LoginSessionMode,
}

/// L2 中间件本体。挂载点：`build_router_with_auth`，L1b 之内、L3 之外。
pub(crate) async fn authenticate(
    State(state): State<AuthChainState>,
    mut req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_owned();
    if !under_api(&path) || auth_exempt(&path) {
        return next.run(req).await;
    }

    let request_id = req.extensions().get::<RequestId>().cloned();
    let Some(session_id) = session_cookie(req.headers(), SESSION_COOKIE) else {
        return unauthenticated(request_id);
    };

    let now = Timestamp::new(now_rfc3339());
    match state
        .sessions
        .validate(&session_id, now, state.idle_ttl_seconds)
    {
        Ok(row) => {
            // §4.6 的互斥裁决：cookie 通道只服务 cookie 会话；native 会话
            // 的凭据是 `Authorization: Bearer`（P8 切片），绝不能经 cookie
            // 复用——同一行两种通道等价于「浏览器可窃取的凭据即可登录」。
            if row.mode != LoginSessionMode::Cookie {
                return unauthenticated(request_id);
            }
            req.extensions_mut().insert(AuthenticatedSession {
                organization_id: row.organization_id,
                principal_id: row.principal_id,
                session_id: row.session_id,
                permission_version: row.permission_version,
                token_epoch: row.token_epoch,
                auth_mode: row.mode,
            });
            next.run(req).await
        }
        Err(ValidateFailure::Rejected(_)) => {
            // 拒绝原因（无行/撤销/到期）供日志与审计，不进响应——§3.2 的
            // 脱敏口径。撤销的即时性由 `LoginSessions::revoke` 保证：
            // 撤销后下一个入站请求必 401（§4.5）。
            unauthenticated(request_id)
        }
        Err(ValidateFailure::Storage(source)) => {
            tracing::warn!(%source, "login session validation failed on storage");
            service_unavailable(request_id)
        }
    }
}

/// 401 脱敏响应（三条拒绝路径共用的同一 wire 形状）。
fn unauthenticated(request_id: Option<RequestId>) -> Response {
    ApiErrorResponse::new(
        ApiError::new(ApiErrorCode::Unauthenticated, "Authentication required.")
            .with_request_id(request_id),
    )
    .into_response()
}

/// 503 响应：管理库不可达（`LoginSessions::validate` 的 Storage 分支）。
fn service_unavailable(request_id: Option<RequestId>) -> Response {
    ApiErrorResponse::new(
        ApiError::new(
            ApiErrorCode::ServiceUnavailable,
            "The service is temporarily unavailable.",
        )
        .with_request_id(request_id),
    )
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::middleware::{auth_exempt, session_cookie, under_api};
    use datazen::shared::id::Timestamp;

    /// INV-S1 的形状钉子：扩展里的身份字段取自会话行，请求体的同名字段
    /// 永不参与（本类型没有任何「来自请求」的构造路径）。
    #[test]
    fn authenticated_session_carries_row_facts_not_request_facts() {
        let session = AuthenticatedSession {
            organization_id: "org-a".into(),
            principal_id: "user-1".into(),
            session_id: "s-1".into(),
            permission_version: 3,
            token_epoch: 1,
            auth_mode: LoginSessionMode::Cookie,
        };
        assert_eq!(session.organization_id, "org-a");
        assert_eq!(session.principal_id, "user-1");
        assert_eq!(session.permission_version, 3);
        assert_eq!(session.token_epoch, 1);
        assert_eq!(session.auth_mode, LoginSessionMode::Cookie);
        let _ = Timestamp::new("2026-01-01T00:00:00.000Z");
    }

    /// cookie 解析：`__Host-` 名精确匹配、分号与空白容忍、前缀相似不误报。
    #[test]
    fn cookie_parsing_matches_the_host_prefixed_name_exactly() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            axum::http::HeaderValue::from_static(
                "__Host-dz_other= decoy; __Host-dz_session=s-42; other=1",
            ),
        );
        assert_eq!(
            session_cookie(&headers, SESSION_COOKIE).as_deref(),
            Some("s-42")
        );
        assert_eq!(session_cookie(&headers, "__Host-dz_none"), None);
        assert_eq!(session_cookie(&headers, "__Host-dz_session_extra"), None);
    }

    /// 豁免判定：探针路径只到 L0，auth 前缀部分豁免（§3.3）。
    #[test]
    fn path_gates_match_the_early_rejection_table() {
        assert!(!under_api("/healthz"));
        assert!(!under_api("/readyz"));
        assert!(under_api("/api/v1/connections"));
        assert!(under_api("/api/v1"));
        assert!(!under_api("/apiv1/connections"), "前缀必须含斜杠边界");
        assert!(auth_exempt("/api/v1/auth/login"));
        assert!(auth_exempt("/api/v1/auth/desktop/start"));
        assert!(!auth_exempt("/api/v1/connections"));
        assert!(!auth_exempt("/api/v1/authentication"), "路径段必须完整匹配");
    }

    /// 401 的 wire 形状：`Unauthenticated` → 401（§9.2 映射表的落地）。
    #[test]
    fn the_unauthenticated_response_is_a_401() {
        let response = unauthenticated(Some(RequestId::new("req-9")));
        assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
    }
}
