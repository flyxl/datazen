//! WR-D1 骨架的行为钉子。
//!
//! 这些断言是契约的机器形态：501/404 的 wire 体逐字段、requestId 的
//! 透传与铸造、健康探针的存在。WR-D2+ 逐域点亮路由时，这些测试**不许
//! 改语义**——只能增行，不能改断言（改断言 = 改契约 = 走 /api/v2）。

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use datazen_server::http::router::build_router;
use tower::ServiceExt;

fn app() -> Router {
    build_router()
}

/// GET 一个 URI，返回（状态码，原始响应体，requestId 响应头）。
async fn get(app: Router, uri: &str) -> (StatusCode, Vec<u8>, Option<String>) {
    let response = app
        .oneshot(Request::get(uri).body(Body::empty()).expect("request body is empty"))
        .await
        .expect("oneshot dispatch");
    let status = response.status();
    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    (status, bytes.to_vec(), request_id)
}

/// 解析 `ApiError` JSON 体（调用方已断言状态码）。
fn api_error_json(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes).expect("error body is ApiError JSON")
}

#[tokio::test]
async fn healthz_and_readyz_are_live() {
    let (status, body, _) = get(app(), "/healthz").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"ok", "plain text body, no JSON contract");

    let (status, _, _) = get(app(), "/readyz").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn api_v1_answers_501_capability_unsupported_until_domains_land() {
    let (status, body, request_id) = get(app(), "/api/v1/connections").await;
    assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    let json = api_error_json(&body);
    assert_eq!(json["code"], "capabilityUnsupported");
    assert_eq!(json["retryDisposition"], "never");
    assert!(
        json["message"].as_str().is_some_and(|m| !m.is_empty()),
        "message is present and non-empty"
    );
    assert!(
        request_id.as_deref().is_some_and(|id| !id.is_empty()),
        "every response carries x-request-id"
    );
    // requestId 与 wire 体：D1 骨架的 fallback 不注入 ApiError.requestId
    // （那是 L0 扩展 + 错误中间件的接线，随 WR-D3 的统一错误映射上线），
    // 响应头先钉住存在性。
}

#[tokio::test]
async fn unknown_non_api_path_is_404_not_found() {
    let (status, body, _) = get(app(), "/definitely/not/here").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let json = api_error_json(&body);
    assert_eq!(json["code"], "notFound");
    assert_eq!(json["retryDisposition"], "never");
}

#[tokio::test]
async fn request_id_is_passed_through_when_upstream_sends_one() {
    let response = app()
        .oneshot(
            Request::get("/healthz")
                .header("x-request-id", "gateway-trace-42")
                .body(Body::empty())
                .expect("request body is empty"),
        )
        .await
        .expect("oneshot dispatch");
    assert_eq!(
        response
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok()),
        Some("gateway-trace-42"),
        "upstream id is forwarded verbatim, not regenerated"
    );
}

#[tokio::test]
async fn body_over_the_limit_is_rejected_before_handlers() {
    // 8 MiB 上限（team-server §9.1）+ 1 字节 = 413。Content-Length 路径
    // （L1a）：自研闸在进 handler 之前拒绝，wire 体是 ApiError。
    let oversized = vec![0u8; 8 * 1024 * 1024 + 1];
    let response = app()
        .oneshot(
            Request::post("/api/v1/connections")
                .header("content-length", oversized.len().to_string())
                .body(Body::from(oversized))
                .expect("request body is a byte vector"),
        )
        .await
        .expect("oneshot dispatch");
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    // L0 最外层裁决：413 也必须带 requestId。
    assert!(
        response.headers().get("x-request-id").is_some(),
        "even the body-limit rejection carries x-request-id"
    );
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    let body: serde_json::Value = serde_json::from_slice(&bytes).expect("413 body is ApiError JSON");
    assert_eq!(body["code"], "payloadTooLarge");
    assert_eq!(body["retryDisposition"], "never");
}

#[tokio::test]
async fn body_at_exactly_the_limit_is_not_rejected_by_the_content_length_gate() {
    // 边界值本身不拒绝（`>` 而非 `>=`）：上限内的请求继续走链。
    let at_limit = vec![0u8; 8 * 1024 * 1024];
    let response = app()
        .oneshot(
            Request::post("/api/v1/connections")
                .header("content-length", at_limit.len().to_string())
                .body(Body::from(at_limit))
                .expect("request body is a byte vector"),
        )
        .await
        .expect("oneshot dispatch");
    // 在上限内 → 放行到 fallback → 501（本 build 无域路由）。
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
}
