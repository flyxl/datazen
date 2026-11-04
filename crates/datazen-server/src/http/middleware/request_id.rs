//! L0：分配 / 透传 `requestId`（team-server §3.1 的最外层）。
//!
//! 职责（§3.1）：请求进来先拿关联 id——上游带了 `x-request-id` 就透传
//! （网关链路保持关联），没带就铸造 UUID v4；id 注入请求扩展（后续层的
//! handler 用 [`axum::Extension`] 取，进 `ApiError.requestId`）并回写
//! 响应头。**最外层**是裁决：413、500、panic 恢复这些没走到 handler 的
//! 响应也必须带 id，否则报障对不上日志。
//!
//! D1 不接 tracing span：`request_id = …` 的 span 字段接线随 WR-D3 的
//! trace 传播一并做（届时认证层要往同一 span 写 principal）。半个 span
//! 比没有 span 更难推理，这里只做扩展注入 + 响应头两件完整的事。

use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;
use datazen::shared::id::RequestId;

/// 响应头名（小写；HTTP/1 与 HTTP/2 都合法）。
pub const HEADER: &str = "x-request-id";

/// L0 中间件本体。挂载点：`router::build_router` 最外层。
pub async fn assign(mut req: Request, next: Next) -> Response {
    let request_id = match req.headers().get(HEADER).and_then(|v| v.to_str().ok()) {
        // 透传上游 id：非空即接受（格式不设限——网关生态里 id 的形状不归
        // 我们定，拒绝会破坏链路关联）。空串视同缺失。
        Some(upstream) if !upstream.is_empty() => RequestId::new(upstream),
        _ => RequestId::new(uuid::Uuid::new_v4().to_string()),
    };

    req.extensions_mut().insert(request_id.clone());
    let mut response = next.run(req).await;

    if let Ok(value) = HeaderValue::from_str(request_id.as_str()) {
        response.headers_mut().insert(HEADER, value);
    }
    response
}
