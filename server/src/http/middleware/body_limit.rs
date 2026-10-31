//! L1（前半）：`Content-Length` 字节闸（team-server §3.1 的「Content-Length +
//! 流式」中的 Content-Length 半边）。
//!
//! 带显式长度的请求在进 handler **之前**按 413 拒绝，wire 体走
//! `ApiError`（`payloadTooLarge` / `never`）。流式（无 Content-Length）的
//! 半边由 tower-http 的 `RequestBodyLimitLayer` 兜底（装配在
//! `router::build_router`），那一半在传输层拒绝、暂不带 JSON 体——统一
//! 错误映射中间件（WR-D3）上线后把两条路都折成同一形状。
//!
//! 上限由装配点以参数传入（`router::build_router` 传 `MAX_BODY_BYTES`）：
//! 闸门逻辑与首版数值是两个变更轴，测试可以注入不同数值验证边界。

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use datazen::shared::error::{ApiError, ApiErrorCode};

use crate::http::response::ApiErrorResponse;

/// L1 前半本体：`Content-Length` 超限即 413，否则放行。
pub async fn reject_oversized_content_length(req: Request, next: Next, max: usize) -> Response {
    if let Some(length) = req.headers().get(axum::http::header::CONTENT_LENGTH) {
        if let Some(bytes) = length.to_str().ok().and_then(|raw| raw.parse::<usize>().ok()) {
            if bytes > max {
                return ApiErrorResponse::new(ApiError::new(
                    ApiErrorCode::PayloadTooLarge,
                    "Request body exceeds the physical limit.",
                ))
                .with_status(StatusCode::PAYLOAD_TOO_LARGE)
                .into_response();
            }
        }
        // 非数字的 Content-Length 不在本层处置：传输栈自会拒绝畸形请求。
    }
    next.run(req).await
}
