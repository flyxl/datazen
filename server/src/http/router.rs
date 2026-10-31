//! v1 路由表（team-server §2.3 的 M/N 步）。
//!
//! D1 骨架只挂机器面（`/healthz` / `/readyz`）；`/api/v1` 的域路由按
//! WR-C2 冻结的契约表（`docs/architecture/platform/http-api-contract.md`）
//! 随 WR-D2（仓储 + 逐域 handler）、WR-D4（执行接线）、WR-D5（SSE）点亮。
//! 在此之前，一切 `/api/` 路径统一回答 501 `capabilityUnsupported`
//! （契约 §7：`not-implemented` 类 → 501），非 `/api/` 的未知路径回答
//! 404 `notFound`。**没有 catch-all 之外的任何域端点**——契约的「多余
//! 端点为零」验收在 D1 就成立。
//!
//! 中间件链的装配顺序见 [`middleware`]；请求体上限取 team-server §9.1
//! 的首版权威值 8 MiB（L1 物理闸）。

use axum::extract::Request;
use axum::http::{StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::limit::RequestBodyLimitLayer;

use crate::http::middleware;
use crate::http::response::{status_for, ApiErrorResponse};
use datazen::shared::error::{ApiError, ApiErrorCode};

/// 请求体字节上限：team-server §9.1 首版权威值（8 MiB，L1 物理闸 → 413）。
pub const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

/// L1a 的 from_fn 形状适配：上限取本模块常量，逻辑在 `middleware::body_limit`。
async fn content_length_gate(req: Request, next: Next) -> Response {
    middleware::body_limit::reject_oversized_content_length(req, next, MAX_BODY_BYTES).await
}

/// 组装完整 Router（机器面 + `/api/v1` 骨架 + 中间件链）。
///
/// 幂等：不读环境、不建连接，可以反复调用（集成测试就依赖这一点）。
pub fn build_router() -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .fallback(api_or_not_found)
        // 装配顺序（外 → 内）：requestId → L1a Content-Length 闸 →
        // catch-panic → L1b 流式体闸 → handler。`Router::layer` 后加的在外层；
        // L0 必须最外，保证 413/500 也带 requestId。
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .layer(CatchPanicLayer::new())
        .layer(axum::middleware::from_fn(content_length_gate))
        .layer(axum::middleware::from_fn(middleware::request_id::assign))
}

/// 存活探针：进程在服务即 200（team-server §2.4 / §5）。
async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

/// 就绪探针：composition root 走完即 200。WR-D2 引入管理库后，这里改为
/// 探测管理库可达（§2.4 的 drain 语义随 D6 补齐）。
async fn readyz() -> impl IntoResponse {
    (StatusCode::OK, "ready")
}

/// 一切未挂路由路径的统一出口。
///
/// `/api/` 前缀 → 501 `capabilityUnsupported`（本 build 尚未点亮任何域
/// 路由；点亮后本函数对已挂路由不再可达，只剩真正未实现的新路径）；
/// 其余 → 404 `notFound`。两条都走 [`ApiErrorResponse`]，wire 形状与
/// 契约一致。
async fn api_or_not_found(uri: Uri) -> Response {
    let error = if uri.path().starts_with("/api/") {
        ApiError::new(
            ApiErrorCode::CapabilityUnsupported,
            "This build serves no /api/v1 domain routes yet; they arrive with WR-D2 and later slices.",
        )
    } else {
        ApiError::new(ApiErrorCode::NotFound, "No such route.")
    };
    // 显式落一遍状态码再交给 ApiErrorResponse：让本函数对 501/404 的选择
    // 可读、可测，而不是藏在 IntoResponse 里。
    let status = status_for(error.code);
    ApiErrorResponse::new(error).with_status(status).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_limit_is_the_documented_first_version_value() {
        assert_eq!(MAX_BODY_BYTES, 8 * 1024 * 1024);
    }
}
