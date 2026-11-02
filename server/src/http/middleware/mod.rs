//! 请求中间件链（team-server §3.1 的进入顺序）。
//!
//! 完整目标链（外 → 内）与各层的落地切片：
//!
//! | 层 | 职责 | 状态 |
//! | --- | --- | --- |
//! | L0 [`request_id`] | 分配/透传 `requestId`，注入扩展 | **已上线（D1）** |
//! | L1a [`body_limit`] | `Content-Length` 显式长度闸（413 + `ApiError` 体） | **已上线（D1）** |
//! | L1b 请求体流式闸 | 无显式长度的流式体（tower-http） | **已上线（D1）** |
//! | L2 [`auth`] | 登录会话 cookie → 查会话行校验 → 注入会话扩展 | **已上线（D3 第一刀）** |
//! | L3 [`session`] | 只提取会话句柄 + runtimeEpoch 参数 | **已上线（D3 第一刀，探针形态）** |
//! | L4 [`csrf`] | 会话绑定令牌校验（写方法）+ `csrf_rejected` 审计 | **已上线（D3 第一刀）** |
//! | L5 [`ratelimit`] | 令牌桶（principal/IP 键，429 + `Retry-After`） | **已落地（本刀）** |
//! | L6 [`authorize`] | `PolicyService.authorize`（可见性先于动作） | **已上线（D3 第一刀，探针形态）** |
//! | 统一错误映射 | handler 错误 → `ApiError` | 状态码表已上线（`http::response`），错误中间件随 D4 |
//! | 审计 | `Audit` 仓储落账 | CSRF 拒绝已落账（L4，§5.3）；全量审计中间件随 D4 |
//!
//! 链顺序是 §3.1 的裁决，不是实现细节。装配点：
//! [`router::build_router_with_auth`](crate::http::router::build_router_with_auth)
//! （认证链）与 [`router::build_router`](crate::http::router::build_router)
//! （D1 骨架，L0/L1，行为被 `tests/skeleton.rs` 钉住，不得改动）。
//!
//! ## 层间共享的小工具
//!
//! [`under_api`] / [`auth_exempt`] / [`session_cookie`] 是 §3.3 早期拒绝表
//! 与 §4.4 cookie 解析的落点，四个自研层共用同一份判定——各层独立发明
//! 「哪些路径受保护」迟早漂移。
//!
//! panic 安全（CatchPanicLayer）与 L1b 由 tower-http 提供，装配在
//! `build_router*` 里；本目录只放自研层。

pub mod auth;
pub mod authorize;
pub mod body_limit;
pub mod csrf;
pub mod ratelimit;
pub mod request_id;
pub mod session;

use std::sync::Arc;

use axum::http::HeaderMap;

use crate::csrf::CsrfSecret;
use crate::policy::PolicyService;
use crate::storage::LoginSessions;

/// 认证链各层共享的状态（`from_fn_with_state` 的注入物）。
///
/// `audit` 面向端口 trait（`Arc<dyn AuditSink>`）：L4 只依赖「追加写」
/// 能力，具体落库形态（服务端 `ServerAudit`、测试替身）由装配点决定。
#[derive(Clone)]
pub(crate) struct AuthChainState {
    pub sessions: LoginSessions,
    pub policy: Arc<PolicyService>,
    pub csrf: Arc<CsrfSecret>,
    pub audit: Arc<dyn datazen::shared::audit::AuditSink>,
    /// 空闲寿命（秒；§4.3 默认 8 小时，来自部署配置）。
    pub idle_ttl_seconds: u64,
}

/// 受保护面判定（§3.3 早期拒绝表的适用范围）：`/api/` 前缀。
///
/// `/healthz`、`/readyz` 与未来的静态资源面**只到 L0**——认证链的四个
/// 层都用本函数短路，探针因此不携带会话、CSRF 或授权语义。
pub(crate) fn under_api(path: &str) -> bool {
    path.starts_with("/api/")
}

/// `/api/v1/auth/*` 的 L2 部分豁免（§3.3：login/callback 自身依赖 state
/// 校验）。
///
/// 本刀推迟了 OIDC IdP 端点（login/callback/logout、desktop start/exchange
/// 全家，见模块报告），路径尚无 handler；豁免先行落位使这些路径继续走
/// 骨架的 501 `capabilityUnsupported`（契约 §7 的 not-implemented 映射），
/// 而不是被会话闸拦成 401。OIDC 切片点亮路由时，login/callback 维持本
/// 豁免、logout 改为要求会话（届时把豁免收窄到 login/callback 两个路径）。
pub(crate) fn auth_exempt(path: &str) -> bool {
    path == "/api/v1/auth" || path.starts_with("/api/v1/auth/")
}

/// 从 `Cookie` 头取第一个名为 `name` 的值（§4.4 的三个 `__Host-` cookie
/// 都经这里解析）。多个同名 cookie 时取第一个——`__Host-` 前缀使浏览器
/// 不会重复种同名 cookie，重复只可能来自伪造请求，取第一个与浏览器语义
/// 一致。
pub(crate) fn session_cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<String> {
    for value in headers.get_all(axum::http::header::COOKIE) {
        let Ok(raw) = value.to_str() else {
            continue;
        };
        for pair in raw.split(';') {
            if let Some((key, val)) = pair.trim().split_once('=') {
                if key == name {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

/// 请求时刻的定宽 RFC3339 UTC（毫秒）——会话表的时刻列与比较口径
/// （`storage::login_sessions` 的规范化纪律）。
pub(crate) fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
