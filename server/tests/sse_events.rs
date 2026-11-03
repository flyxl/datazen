//! WR-D5 SSE 适配的集成钉子（oneshot 驱动，进程内假体）。
//!
//! 覆盖：订阅建立（归属盖章/认证/表满）、握手证明（§8.1 的 Origin /
//! Fetch-Metadata 全分支）、续传翻译（Last-Event-ID 优先于 `?afterSequence=`，
//! §8.2/契约 §6.6）、订阅缺口 → `streamResetRequired`（端口的 Err 不是
//! 404）、首波积压上界 → reset + 关流（§8.2 慢消费者）、心跳注释行与
//! §8.4 周期复核的终态事件、streamId 一次性消费。
//!
//! L2 在真实装配里注入 `AuthenticatedSession`；本文件以 `Extension` 层
//! 等价注入（同一扩展类型，装配语义由主线集成腿复核）。

use std::convert::Infallible;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, Request as HttpRequest, StatusCode};
use axum::routing::{get, post};
use axum::Router;
use tower::{Service, ServiceExt};

use datazen::execution::application::event_sink::EventSink;
use datazen::execution::application::event_sink_memory::MemoryEventSink;
use datazen::execution::application::{ConnectionEvent, ConnectionEventEnvelope, EventEnvelope};
use datazen::shared::context::RequestContext;
use datazen::shared::id::{
    ClientInstanceId, Counter, OrganizationId, PrincipalId, RequestId, RuntimeEpoch, StreamId,
};

use datazen_server::http::middleware::auth::AuthenticatedSession;
use datazen_server::http::sse::{
    EventStreamRegistry, ReplayQuery, SseAccessRecheck, SseConfig, SseState,
};

fn context() -> RequestContext {
    RequestContext::new(
        OrganizationId::new("org-1"),
        PrincipalId::new("user-1"),
        None,
        ClientInstanceId::new("client-1"),
        RequestId::new("req-1"),
        None,
    )
}

fn session() -> AuthenticatedSession {
    AuthenticatedSession {
        organization_id: "org-1".into(),
        principal_id: "user-1".into(),
        session_id: "sess-1".into(),
        permission_version: 1,
        token_epoch: 0,
        auth_mode: datazen_server::storage::login_sessions::LoginSessionMode::Cookie,
    }
}

fn session_of(org: &str, principal: &str) -> AuthenticatedSession {
    AuthenticatedSession {
        organization_id: org.into(),
        principal_id: principal.into(),
        session_id: "sess-1".into(),
        permission_version: 1,
        token_epoch: 0,
        auth_mode: datazen_server::storage::login_sessions::LoginSessionMode::Cookie,
    }
}

/// 恒定复刻（复核永远通过——心跳路径的行为钉子用它隔离登录事实）。
struct AllowRecheck;

#[async_trait::async_trait]
impl SseAccessRecheck for AllowRecheck {
    async fn recheck(
        &self,
        _session_id: &str,
        _organization_id: &str,
        _snapshot: i64,
    ) -> Result<(), &'static str> {
        Ok(())
    }
}

/// 恒拒复刻（§8.4 终态事件钉子）。
struct DenyRecheck(&'static str);

#[async_trait::async_trait]
impl SseAccessRecheck for DenyRecheck {
    async fn recheck(
        &self,
        _session_id: &str,
        _organization_id: &str,
        _snapshot: i64,
    ) -> Result<(), &'static str> {
        Err(self.0)
    }
}

fn config(origins: &[&str], backlog: usize, keepalive: Duration) -> SseConfig {
    SseConfig {
        allowed_origins: origins.iter().map(|o| o.to_string()).collect(),
        keepalive_interval: keepalive,
        max_backlog_frames: backlog,
        subscription_ttl: Duration::from_secs(300),
        registry_capacity: 16,
    }
}

fn state(config: SseConfig, recheck: Arc<dyn SseAccessRecheck>) -> SseState {
    SseState {
        registry: Arc::new(EventStreamRegistry::new(&config)),
        sink: Arc::new(MemoryEventSink::new()),
        recheck,
        config: Arc::new(config),
        now_unix: Arc::new(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        }),
    }
}

fn app(state: SseState) -> Router {
    Router::new()
        .route(
            "/api/v1/events/subscriptions",
            post(datazen_server::http::sse::create_subscription),
        )
        .route(
            "/api/v1/events/{streamId}",
            get(datazen_server::http::sse::events),
        )
        .layer(ExtensionInjector(Some(RequestId::new("req-1"))))
        .with_state(state)
}

/// 以 layer 形态注入扩展的最小件（axum 0.8 的 `Extension::new` 已不在
/// 公共面；`from_extractor_with_state` 需要完整状态——这里用一个两行的
/// tower Layer 直接写请求扩展，等价 L2 的注入点）。
#[derive(Clone)]
struct ExtensionInjector<T>(Option<T>);

impl<S, T> tower::Layer<S> for ExtensionInjector<T>
where
    S: Service<Request>,
    T: Clone,
{
    type Service = InjectExtension<S, T>;

    fn layer(&self, inner: S) -> Self::Service {
        InjectExtension {
            inner,
            extension: self.0.clone(),
        }
    }
}

struct InjectExtension<S, T> {
    inner: S,
    extension: Option<T>,
}

impl<S: Clone, T: Clone> Clone for InjectExtension<S, T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            extension: self.extension.clone(),
        }
    }
}

impl<S, T> Service<Request> for InjectExtension<S, T>
where
    S: Service<Request>,
    T: Clone + Send + Sync + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request) -> Self::Future {
        if let Some(extension) = self.extension.clone() {
            request.extensions_mut().insert(extension);
        }
        self.inner.call(request)
    }
}

async fn post_subscription(app: &mut Router, authed: bool) -> Result<String, StatusCode> {
    let mut request = HttpRequest::builder()
        .method("POST")
        .uri("/api/v1/events/subscriptions")
        .body(Body::empty())
        .expect("request");
    if authed {
        request.extensions_mut().insert(session());
    }
    let response = app.oneshot(request).await.expect("call");
    if response.status() != StatusCode::OK {
        return Err(response.status());
    }
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body");
    let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    Ok(value["streamId"].as_str().expect("streamId").to_string())
}

async fn get_stream(
    app: &mut Router,
    stream_id: &str,
    headers: &[(&str, &str)],
    query: &str,
) -> axum::http::Response<Body> {
    get_stream_as(app, stream_id, headers, query, &session()).await
}

async fn get_stream_as(
    app: &mut Router,
    stream_id: &str,
    headers: &[(&str, &str)],
    query: &str,
    authed: &AuthenticatedSession,
) -> axum::http::Response<Body> {
    let mut builder = HttpRequest::builder()
        .method("GET")
        .uri(format!("/api/v1/events/{stream_id}{query}"));
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let mut request = builder.body(Body::empty()).expect("request");
    request.extensions_mut().insert(authed.clone());
    app.oneshot(request).await.expect("call")
}

/// 从 SSE 体里读到 `count` 个事件帧（`\n\n` 终止）或超时。
async fn read_frames(body: Body, count: usize) -> Vec<String> {
    let mut stream = body.into_data_stream();
    let mut buffer = String::new();
    let mut frames = Vec::new();
    while frames.len() < count {
        let chunk = tokio::time::timeout(Duration::from_secs(3), async {
            use futures_util::StreamExt;
            stream.next().await
        })
        .await;
        match chunk {
            Ok(Some(Ok(bytes))) => buffer.push_str(&String::from_utf8_lossy(&bytes)),
            Ok(Some(Err(_))) | Ok(None) | Err(_) => break,
            Ok(None) | Err(_) => break,
        }
        while let Some(position) = buffer.find("\n\n") {
            let frame: String = buffer.drain(..position + 2).collect();
            frames.push(frame);
            if frames.len() >= count {
                break;
            }
        }
    }
    frames
}

fn envelope(stream: &str, sequence: u64) -> ConnectionEventEnvelope {
    EventEnvelope::new(
        StreamId::new(stream.to_string()),
        Counter::new(sequence),
        RuntimeEpoch::new("epoch-1"),
        ConnectionEvent::StreamResetRequired {
            reason: "test".into(),
        },
    )
}

async fn publish(sink: &MemoryEventSink, stream: &str, sequence: u64) {
    sink.publish(&context(), envelope(stream, sequence))
        .await
        .expect("publish");
}

// ----------------------------------------------------------------- 钉子 ----

#[tokio::test]
async fn subscription_creation_returns_a_csp_rng_stream_id() {
    let mut app = app(state(
        config(&["https://ui.test"], 256, Duration::from_secs(15)),
        Arc::new(AllowRecheck),
    ));
    let stream_id = post_subscription(&mut app, true).await.expect("subscribe");
    assert!(stream_id.starts_with("evt-"), "streamId 形态：{stream_id}");
    // 两次铸造不同（CSPRNG）。
    let again = post_subscription(&mut app, true).await.expect("subscribe");
    assert_ne!(stream_id, again);
}

#[tokio::test]
async fn subscription_requires_an_authenticated_session() {
    let mut app = app(state(
        config(&[], 256, Duration::from_secs(15)),
        Arc::new(AllowRecheck),
    ));
    let status = post_subscription(&mut app, false).await.err().expect("401");
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn handshake_rejects_requests_without_any_origin_proof() {
    let mut app = app(state(
        config(&["https://ui.test"], 256, Duration::from_secs(15)),
        Arc::new(AllowRecheck),
    ));
    let stream_id = post_subscription(&mut app, true).await.expect("subscribe");
    let response = get_stream(&mut app, &stream_id, &[], "").await;
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "§8.1：两者都缺拒绝"
    );
}

#[tokio::test]
async fn handshake_rejects_mismatched_and_null_origins() {
    let mut app = app(state(
        config(&["https://ui.test"], 256, Duration::from_secs(15)),
        Arc::new(AllowRecheck),
    ));
    let stream_id = post_subscription(&mut app, true).await.expect("subscribe");
    let response = get_stream(&mut app, &stream_id, &[("Origin", "https://evil.test")], "").await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    // Origin: null 不豁免（§8.1 明文）。
    let stream_id = post_subscription(&mut app, true).await.expect("subscribe");
    let response = get_stream(&mut app, &stream_id, &[("Origin", "null")], "").await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn handshake_accepts_the_configured_origin() {
    let mut app = app(state(
        config(&["https://ui.test"], 256, Duration::from_secs(15)),
        Arc::new(AllowRecheck),
    ));
    let stream_id = post_subscription(&mut app, true).await.expect("subscribe");
    let response = get_stream(&mut app, &stream_id, &[("Origin", "https://ui.test")], "").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("content-type"),
        "text/event-stream"
    );
}

#[tokio::test]
async fn handshake_accepts_same_origin_fetch_metadata_without_an_origin_header() {
    let mut app = app(state(
        config(&["https://ui.test"], 256, Duration::from_secs(15)),
        Arc::new(AllowRecheck),
    ));
    let stream_id = post_subscription(&mut app, true).await.expect("subscribe");
    let response = get_stream(
        &mut app,
        &stream_id,
        &[("Sec-Fetch-Site", "same-origin")],
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    // cross-site 不豁免（§8.1 明文）。
    let stream_id = post_subscription(&mut app, true).await.expect("subscribe");
    let response = get_stream(
        &mut app,
        &stream_id,
        &[("Sec-Fetch-Site", "cross-site")],
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn unknown_stream_id_is_404_not_a_replay_gap() {
    let mut app = app(state(
        config(&["https://ui.test"], 256, Duration::from_secs(15)),
        Arc::new(AllowRecheck),
    ));
    let response = get_stream(
        &mut app,
        "evt-does-not-exist",
        &[("Origin", "https://ui.test")],
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn stream_id_is_single_use_at_handshake() {
    let mut app = app(state(
        config(&["https://ui.test"], 256, Duration::from_secs(15)),
        Arc::new(AllowRecheck),
    ));
    let stream_id = post_subscription(&mut app, true).await.expect("subscribe");
    let first = get_stream(&mut app, &stream_id, &[("Origin", "https://ui.test")], "").await;
    assert_eq!(first.status(), StatusCode::OK);
    let second = get_stream(&mut app, &stream_id, &[("Origin", "https://ui.test")], "").await;
    assert_eq!(
        second.status(),
        StatusCode::NOT_FOUND,
        "一个 streamId 一次握手"
    );
}

#[tokio::test]
async fn handshake_rejects_a_foreign_identity() {
    let mut app = app(state(
        config(&["https://ui.test"], 256, Duration::from_secs(15)),
        Arc::new(AllowRecheck),
    ));
    let stream_id = post_subscription(&mut app, true)
        .await
        .expect("subscribe as user-1");
    // 换一个主体的会话来握手（他人票据）。
    let mut foreign = app.clone();
    let response = get_stream_as(
        &mut foreign,
        &stream_id,
        &[("Origin", "https://ui.test")],
        "",
        &session_of("org-1", "intruder"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn last_event_id_header_has_priority_over_after_sequence() {
    let sink = Arc::new(MemoryEventSink::new());
    let cfg = config(&["https://ui.test"], 256, Duration::from_secs(15));
    let st = SseState {
        registry: Arc::new(EventStreamRegistry::new(&cfg)),
        sink: sink.clone(),
        recheck: Arc::new(AllowRecheck),
        config: Arc::new(cfg),
        now_unix: Arc::new(|| 0),
    };
    let mut app = app(st);
    let stream_id = post_subscription(&mut app, true).await.expect("subscribe");
    for sequence in 1..=3 {
        publish(sink.as_ref(), &stream_id, sequence).await;
    }
    let response = get_stream(
        &mut app,
        &stream_id,
        &[("Origin", "https://ui.test"), ("Last-Event-ID", "1")],
        "?afterSequence=0",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let frames = read_frames(response.into_body(), 2).await;
    // 头优先：从 lastEventId+1 = 2 起回放（不是 1）。
    assert!(frames[0].contains("id: 2"), "头优先于参数：{frames:?}");
    assert!(frames[1].contains("id: 3"));
}

#[tokio::test]
async fn after_sequence_param_replays_from_the_next_sequence() {
    let sink = Arc::new(MemoryEventSink::new());
    let cfg = config(&["https://ui.test"], 256, Duration::from_secs(15));
    let st = SseState {
        registry: Arc::new(EventStreamRegistry::new(&cfg)),
        sink: sink.clone(),
        recheck: Arc::new(AllowRecheck),
        config: Arc::new(cfg),
        now_unix: Arc::new(|| 0),
    };
    let mut app = app(st);
    let stream_id = post_subscription(&mut app, true).await.expect("subscribe");
    for sequence in 1..=2 {
        publish(sink.as_ref(), &stream_id, sequence).await;
    }
    let response = get_stream(
        &mut app,
        &stream_id,
        &[("Origin", "https://ui.test")],
        "?afterSequence=1",
    )
    .await;
    let frames = read_frames(response.into_body(), 1).await;
    assert!(frames[0].contains("id: 2"), "{frames:?}");
    assert!(
        frames[0].contains("event: streamResetRequired"),
        "event 行用 payload kind：{frames:?}"
    );
}

#[tokio::test]
async fn non_decimal_replay_position_is_a_bad_request() {
    let mut app = app(state(
        config(&["https://ui.test"], 256, Duration::from_secs(15)),
        Arc::new(AllowRecheck),
    ));
    let stream_id = post_subscription(&mut app, true).await.expect("subscribe");
    let response = get_stream(
        &mut app,
        &stream_id,
        &[("Origin", "https://ui.test"), ("Last-Event-ID", "12abc")],
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn subscription_gap_emits_stream_reset_required_then_closes() {
    // Last-Event-ID 超前于已发布最大序号（订阅时缺口）→ 端口 Err → 流内
    // reset（不是 404，端口模块头的裁决）。
    let sink = Arc::new(MemoryEventSink::new());
    let cfg = config(&["https://ui.test"], 256, Duration::from_secs(15));
    let st = SseState {
        registry: Arc::new(EventStreamRegistry::new(&cfg)),
        sink: sink.clone(),
        recheck: Arc::new(AllowRecheck),
        config: Arc::new(cfg),
        now_unix: Arc::new(|| 0),
    };
    let mut app = app(st);
    let stream_id = post_subscription(&mut app, true).await.expect("subscribe");
    publish(sink.as_ref(), &stream_id, 1).await;
    // Last-Event-ID=7 超前于已发布最大序号 1 → 超前缺口（端口语义）。
    let response = get_stream(
        &mut app,
        &stream_id,
        &[("Origin", "https://ui.test"), ("Last-Event-ID", "7")],
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK, "缺口走 200 流内 reset");
    let frames = read_frames(response.into_body(), 1).await;
    assert!(
        frames[0].contains("event: streamResetRequired"),
        "{frames:?}"
    );
}

#[tokio::test]
async fn first_wave_backlog_overflow_resets_and_closes() {
    let sink = Arc::new(MemoryEventSink::new());
    let cfg = config(&["https://ui.test"], 2, Duration::from_secs(15));
    let st = SseState {
        registry: Arc::new(EventStreamRegistry::new(&cfg)),
        sink: sink.clone(),
        recheck: Arc::new(AllowRecheck),
        config: Arc::new(cfg),
        now_unix: Arc::new(|| 0),
    };
    let mut app = app(st);
    let stream_id = post_subscription(&mut app, true).await.expect("subscribe");
    for sequence in 1..=4 {
        publish(sink.as_ref(), &stream_id, sequence).await;
    }
    let response = get_stream(&mut app, &stream_id, &[("Origin", "https://ui.test")], "").await;
    assert_eq!(response.status(), StatusCode::OK);
    let frames = read_frames(response.into_body(), 3).await;
    assert!(frames[0].contains("id: 1"));
    assert!(frames[1].contains("id: 2"));
    assert!(
        frames[2].contains("event: streamResetRequired"),
        "积压超上界 → reset：{frames:?}"
    );
}

#[tokio::test]
async fn keepalive_comments_flow_and_recheck_failures_emit_terminal_events() {
    // 短心跳间隔 + 恒拒复核 → keepalive 注释行后紧跟终态事件。
    let cfg = config(&["https://ui.test"], 256, Duration::from_millis(40));
    let st = SseState {
        registry: Arc::new(EventStreamRegistry::new(&cfg)),
        sink: Arc::new(MemoryEventSink::new()),
        recheck: Arc::new(DenyRecheck("auth_required")),
        config: Arc::new(cfg),
        now_unix: Arc::new(|| 0),
    };
    let mut app = app(st);
    let stream_id = post_subscription(&mut app, true).await.expect("subscribe");
    let response = get_stream(&mut app, &stream_id, &[("Origin", "https://ui.test")], "").await;
    let frames = read_frames(response.into_body(), 2).await;
    assert!(frames[0].contains(": keepalive"), "心跳注释行：{frames:?}");
    assert!(
        frames[1].contains("event: auth_required"),
        "§8.4 终态事件：{frames:?}"
    );
}

#[tokio::test]
async fn live_events_flow_after_the_handshake() {
    let cfg = config(&["https://ui.test"], 256, Duration::from_secs(60));
    let sink = Arc::new(MemoryEventSink::new());
    let st = SseState {
        registry: Arc::new(EventStreamRegistry::new(&cfg)),
        sink: sink.clone() as Arc<dyn EventSink>,
        recheck: Arc::new(AllowRecheck),
        config: Arc::new(cfg),
        now_unix: Arc::new(|| 0),
    };
    let mut app = app(st);
    let stream_id = post_subscription(&mut app, true).await.expect("subscribe");
    // 无回放直接连接（空窗口 + 等实时——端口的竞态语义）。
    let response = get_stream(&mut app, &stream_id, &[("Origin", "https://ui.test")], "").await;
    assert_eq!(response.status(), StatusCode::OK);
    // 握手后发布（同一 streamId）→ 实况帧到达。
    let body = response.into_body();
    let reader = tokio::spawn(async move { read_frames(body, 1).await });
    tokio::time::sleep(Duration::from_millis(150)).await;
    publish(sink.as_ref(), &stream_id, 1).await;
    let frames = reader.await.expect("reader");
    assert!(frames[0].contains("id: 1"), "实况帧：{frames:?}");
}

#[tokio::test]
async fn registry_capacity_is_fail_closed() {
    let cfg = config(&[], 256, Duration::from_secs(15));
    let registry = EventStreamRegistry::new(&cfg);
    for _ in 0..cfg.registry_capacity {
        registry.create("org-1", "user-1", 0).expect("slot");
    }
    let full = registry.create("org-1", "user-1", 0);
    assert!(matches!(
        full,
        Err(datazen::shared::error::PortError::QuotaExceeded(_))
    ));
}

#[tokio::test]
async fn expired_registrations_answer_404() {
    let cfg = SseConfig {
        subscription_ttl: Duration::from_secs(10),
        ..config(&["https://ui.test"], 256, Duration::from_secs(15))
    };
    let registry = EventStreamRegistry::new(&cfg);
    let stream_id = registry.create("org-1", "user-1", 1_000).expect("create");
    assert!(
        registry.take(stream_id.as_str(), 1_011).is_none(),
        "TTL 过期 = 未知流"
    );
}

// ── WR-G2 冻结验收：SSE Origin 校验的故障注入补钉 ─────────────────────
// （真实故障源 = 真 HTTP 请求头组合，不是 mock——每个变体都是浏览器/
// 代理在跨源/重定向场景实际发出的头形。权威：team-server §8.1 的
// 「存在必须等值、缺失要求 same-origin、null/cross-site 不豁免」。）

/// §8.1「存在必须**等值**于配置 origin」的边界钉：尾斜杠、大小写变体、
/// 端口变体都不是等值——精确字符串比较，不做规范化（规范化即放宽）。
#[tokio::test]
async fn origin_variants_are_not_equal_to_the_configured_origin() {
    let mut app = app(state(
        config(&["https://ui.test"], 256, Duration::from_secs(15)),
        Arc::new(AllowRecheck),
    ));
    for (label, origin) in [
        ("尾斜杠", "https://ui.test/"),
        ("host 大小写变体", "https://UI.test"),
        ("scheme 大小写变体", "HTTPS://ui.test"),
        ("端口变体", "https://ui.test:8443"),
        ("子域", "https://cdn.ui.test"),
    ] {
        let stream_id = post_subscription(&mut app, true).await.expect("subscribe");
        let response = get_stream(&mut app, &stream_id, &[("Origin", origin)], "").await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{label}: {origin} 不是配置 origin 的等值"
        );
    }
}

/// §8.1「缺失时要求 same-origin」：same-site / cross-site 不构成同源证明
/// （站点级 ≠ 源级——同站不同源仍可被 CSRF 面利用）。
#[tokio::test]
async fn fetch_metadata_site_variants_do_not_satisfy_the_same_origin_proof() {
    let mut app = app(state(
        config(&["https://ui.test"], 256, Duration::from_secs(15)),
        Arc::new(AllowRecheck),
    ));
    for site in ["same-site", "cross-site", "none", ""] {
        let stream_id = post_subscription(&mut app, true).await.expect("subscribe");
        let response = get_stream(
            &mut app,
            &stream_id,
            &[("Sec-Fetch-Site", site)],
            "",
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "Sec-Fetch-Site: '{site}' 不是同源证明"
        );
    }
}

/// Origin 与 Fetch-Metadata 同在时：Origin 存在即按「等值」裁决——
/// Fetch-Metadata 不构成第二证明通道（origin_proof 的 match 语义：Origin
/// 分支独占）。跨源 Origin + same-origin 元数据 = 拒。
#[tokio::test]
async fn a_present_origin_is_judged_alone_even_with_same_origin_metadata() {
    let mut app = app(state(
        config(&["https://ui.test"], 256, Duration::from_secs(15)),
        Arc::new(AllowRecheck),
    ));
    let stream_id = post_subscription(&mut app, true).await.expect("subscribe");
    let response = get_stream(
        &mut app,
        &stream_id,
        &[
            ("Origin", "https://evil.test"),
            ("Sec-Fetch-Site", "same-origin"),
        ],
        "",
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "Origin 在场即独占裁决：不因 same-origin 元数据放行"
    );
}

/// 重复 Origin 头：`HeaderMap::get` 取首值——首值为恶意源时拒绝。
/// （头注入/代理拼接的故障面：多值 Origin 的合法场景不存在，浏览器只发一个。）
#[tokio::test]
async fn duplicate_origin_headers_are_judged_by_the_first_value() {
    let mut app = app(state(
        config(&["https://ui.test"], 256, Duration::from_secs(15)),
        Arc::new(AllowRecheck),
    ));
    let stream_id = post_subscription(&mut app, true).await.expect("subscribe");
    let mut request = HttpRequest::builder()
        .method("GET")
        .uri(format!("/api/v1/events/{stream_id}"))
        .header(header::ORIGIN, "https://evil.test")
        .header(header::ORIGIN, "https://ui.test")
        .body(Body::empty())
        .expect("request");
    let response = app.oneshot(request).await.expect("call");
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "首值 evil.test 不等值 → 拒（多值 Origin 无合法场景）"
    );
}
