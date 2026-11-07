//! L5：令牌桶限流（team-server §3.2 的 L5 行 + §9.1 的「速率」行）。
//!
//! # 键（§9.1「按 principal + 路由类的令牌桶」）
//!
//! - **认证后**：principal（会话 subject）——L2 注入的
//!   [`AuthenticatedSession`]（组织 + 主体二元组，INV-S1：身份只来自
//!   会话行）。本层在 L4 之内、L6 之外，认证扩展必然存在（存在即用）。
//! - **未认证回退**：客户端 IP（`ConnectInfo<SocketAddr>`，装配点经
//!   `into_make_service_with_connect_info` 提供；不可用时归入同一个
//!   `unattributed` 桶——探针与无连接信息的测试驱动，见模块报告）。
//!   `/api/v1/auth/*` 虽豁免 L2，但**不豁免 L5**：login/callback 未认证，
//!   落 IP 桶（§3.3 的「最小到达层」不减 L5）。
//!
//! # 桶（进程内，仿 `crate::csrf` 的 `Mutex` 形态）
//!
//! `std::sync::Mutex<HashMap<键, 桶>>`。经典令牌桶：容量 + 恒定补充速率，
//! 突发吃存量、长期吃速率。**容量/速率按路由类构造注入**（§9.1 的
//! 「严格档」= 令牌签发/开会话/提交执行/取消；其余默认档），**时钟可注入**
//! （测试用假时钟钉补充曲线）。桶表有 `max_entries` 上界（未认证 IP 键是
//! 攻击面，不做上界等于把内存交给攻击者）；满时逐出「最后活跃最早」的桶。
//!
//! # 失败行为（§9.2 的 429 行）
//!
//! 超限 → **429 `RateLimited`** + `Retry-After` 头（下一次可用令牌的整秒
//! 上取整）+ `ApiError` JSON 体（`crate::http::response` 的既有词汇——
//! `RateLimited` 在 `shared::error` 已登记，无映射缺口）。`retryDisposition
//! = never`（crate 对该 code 的保守取值，见 shared::error 的规格缺口注记）。
//!
//! # 本刀范围
//!
//! 令牌桶 429。L5 的另外两件事——**反序列化失败语义**与**分页/取块上限**
//! （§3.2 L5 行、§9.1 的 400 行）——随域 handler 切片落地（没有 handler
//! 就没有可反序列化的请求体），已登记为推迟项。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{header, HeaderValue, Method};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::shared::id::RequestId;

use crate::http::middleware::auth::AuthenticatedSession;
use crate::http::middleware::under_api;
use crate::http::response::ApiErrorResponse;

/// 一个路由类的桶参数（全部构造注入，§9.1「来自部署配置」）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BucketParams {
    /// 桶容量（可突发消耗的令牌数；≥ 1）。
    pub capacity: u64,
    /// 恒定补充速率（令牌/秒；> 0）。
    pub refill_per_second: f64,
}

/// 路由类（§9.1：令牌签发/开会话/提交执行/取消为严格档，其余默认档）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RouteClass {
    /// 常规读写路由。
    Default,
    /// 严格档：幂等令牌签发、开会话、提交执行、取消。
    Strict,
}

/// 各路由类的桶参数 + 桶表上界（全部构造注入）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RateLimitConfig {
    pub default: BucketParams,
    pub strict: BucketParams,
    /// 桶表条目上界（内存界；满时逐出最久未活跃的桶）。
    pub max_entries: usize,
}

/// 限流键（§9.1：principal 优先，未认证回退 IP）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RateKey {
    /// 认证后：会话行的组织 + 主体（INV-S1 的二元组）。
    Principal {
        organization_id: String,
        principal_id: String,
    },
    /// 未认证回退：客户端 IP（`ConnectInfo` 提供者；缺失归 `unattributed`）。
    ClientIp(String),
}

/// 时钟源（可注入：测试用假时钟驱动补充曲线）。
pub trait RateClock: Send + Sync {
    /// unix 秒（亚秒精度即可：f64）。
    fn now_secs(&self) -> f64;
}

/// 真实时钟。
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl RateClock for SystemClock {
    fn now_secs(&self) -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs_f64())
            .unwrap_or(0.0)
    }
}

/// 一次桶检查的裁决。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RateDecision {
    Allowed,
    /// 拒绝 + `Retry-After` 建议值（秒，≥ 1；到下一次可用令牌的整秒上取整）。
    Rejected {
        retry_after_secs: u64,
    },
}

#[derive(Debug, Clone, Copy)]
struct BucketState {
    tokens: f64,
    last_seen: f64,
}

/// 令牌桶限流器。一进程一份，桶表进程内共享。
pub struct RateLimiter {
    config: RateLimitConfig,
    clock: Arc<dyn RateClock>,
    buckets: Mutex<HashMap<(RouteClass, RateKey), BucketState>>,
}

impl RateLimiter {
    /// 构造。参数非法（容量 0、速率非正、表上界 0）即拒绝——fail-closed。
    pub fn new(config: RateLimitConfig, clock: Arc<dyn RateClock>) -> Result<Self, String> {
        for (name, params) in [("default", &config.default), ("strict", &config.strict)] {
            if params.capacity == 0 {
                return Err(format!("ratelimit: {name} capacity must be at least 1"));
            }
            if !(params.refill_per_second > 0.0) || !params.refill_per_second.is_finite() {
                return Err(format!(
                    "ratelimit: {name} refill_per_second must be positive and finite"
                ));
            }
        }
        if config.max_entries == 0 {
            return Err("ratelimit: max_entries must be at least 1".into());
        }
        Ok(Self {
            config,
            clock,
            buckets: Mutex::new(HashMap::new()),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<(RouteClass, RateKey), BucketState>> {
        // 中毒恢复：桶是纯数值缓存，一次 panic 顶多少扣一枚令牌——继续
        // 服务比把全体请求打成 500 更符合限流层的职责（见模块报告）。
        self.buckets
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 路由类判定（§9.1 严格档清单；W8 刀1对账：契约 §6.10 补行后执行域
    /// 提交路径入本表——`POST …/executions`（executeAtTarget /
    /// executeInSession）与 aiHost 目标执行；过渡域 driver-gateway 的
    /// `/executions` 词尾同语义。stream-executions 不是本词尾（`-` 前缀
    /// 不匹配），维持缺省档）。
    pub fn classify(method: &Method, path: &str) -> RouteClass {
        match (method.as_str(), path) {
            // 幂等令牌签发（契约 §6.2）与开会话（契约 §6.4）。
            ("POST", "/api/v1/submission-tokens") | ("POST", "/api/v1/sessions") => {
                RouteClass::Strict
            }
            // 提交执行（契约 §6.10：/api/v1/executions、
            // /api/v1/sessions/{id}/executions；driver-gateway 过渡域同词尾
            // 同语义）。
            ("POST", path) if path.ends_with("/executions") => RouteClass::Strict,
            // aiHost 目标执行（WR-F1#3 点亮后的提交面）。
            ("POST", "/api/v1/ai-host/execute-target") => RouteClass::Strict,
            // 取消族（§5.2 注记的 `POST /cancel` 语义：执行/Job 的取消端点
            // 以 /cancel 结尾）。
            ("POST", path) if path.ends_with("/cancel") => RouteClass::Strict,
            _ => RouteClass::Default,
        }
    }

    /// 桶检查（补充 → 判定；拒绝**不扣令牌**）。同一键的并发放大至多消耗
    /// 表内现有令牌——检查在锁内完成，是原子的一次裁决。
    pub fn check(&self, key: &RateKey, class: RouteClass) -> RateDecision {
        let params = match class {
            RouteClass::Default => &self.config.default,
            RouteClass::Strict => &self.config.strict,
        };
        let now = self.clock.now_secs();
        let capacity = params.capacity as f64;
        let mut buckets = self.lock();

        if !buckets.contains_key(&(class, key.clone())) && buckets.len() >= self.config.max_entries
        {
            // 表满：逐出最久未活跃的桶（容量水位不变——full 桶很快会回来）。
            if let Some(oldest) = buckets
                .iter()
                .min_by(|a, b| a.1.last_seen.total_cmp(&b.1.last_seen))
                .map(|(key, _)| key.clone())
            {
                buckets.remove(&oldest);
            }
        }

        // 桶按 (类, 键) 分表：严格档的消耗不得吃掉默认档余量（两档参数
        // 独立是构造契约；共享一张桶会让 429 跨类串扰）。
        let entry = buckets.entry((class, key.clone())).or_insert(BucketState {
            tokens: capacity,
            last_seen: now,
        });
        let elapsed = (now - entry.last_seen).max(0.0);
        entry.tokens = (entry.tokens + elapsed * params.refill_per_second).min(capacity);
        entry.last_seen = now;

        if entry.tokens >= 1.0 {
            entry.tokens -= 1.0;
            RateDecision::Allowed
        } else {
            // 到下一枚令牌的整秒上取整（下限 1：refill 有限，永远 ≥ 1 秒后
            // 才可能补足——避免 0 秒的 Retry-After 诱发忙等）。
            let wait = (1.0 - entry.tokens) / params.refill_per_second;
            let retry_after_secs = (wait.ceil() as u64).max(1);
            RateDecision::Rejected { retry_after_secs }
        }
    }
}

/// L5 的共享状态（`from_fn_with_state` 的注入物；与 [`crate::http::middleware::AuthChainState`]
/// 分立——装配点各自构造，见模块报告的插入点）。
#[derive(Clone)]
pub struct RateLimitState {
    pub limiter: Arc<RateLimiter>,
}

/// L5 中间件本体。挂载点：`build_router_with_auth`，L4 之内、L6 之外。
pub(crate) async fn guard(
    State(state): State<RateLimitState>,
    req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path();
    // §3.3：/healthz、/readyz 只到 L0——探针不限流（它们是限流退避时
    // 存活判定的来源，被 429 会本末倒置）。
    if !under_api(path) {
        return next.run(req).await;
    }

    let key = rate_key(&req);
    let class = RateLimiter::classify(req.method(), path);
    if let RateDecision::Rejected { retry_after_secs } = state.limiter.check(&key, class) {
        // 键不进日志：IP 是个人数据，principal 组合无害但也无必要——路由类
        // 与退避秒数足够定位容量问题。
        tracing::warn!(class = ?class, retry_after_secs, "rate limit exceeded");
        return too_many_requests(
            req.extensions().get::<RequestId>().cloned(),
            retry_after_secs,
        );
    }
    next.run(req).await
}

/// §9.1 的键序：principal（会话行事实）→ IP（ConnectInfo）→ 归并桶。
fn rate_key(req: &Request) -> RateKey {
    if let Some(session) = req.extensions().get::<AuthenticatedSession>() {
        return RateKey::Principal {
            organization_id: session.organization_id.clone(),
            principal_id: session.principal_id.clone(),
        };
    }
    RateKey::ClientIp(
        req.extensions()
            .get::<ConnectInfo<std::net::SocketAddr>>()
            .map(|connect_info| connect_info.0.ip().to_string())
            .unwrap_or_else(|| "unattributed".into()),
    )
}

/// 429 + `Retry-After` + `ApiError` JSON 体（§9.2 429 行：必须带头）。
fn too_many_requests(request_id: Option<RequestId>, retry_after_secs: u64) -> Response {
    let mut response = ApiErrorResponse::new(
        ApiError::new(
            ApiErrorCode::RateLimited,
            "Too many requests; retry after the stated interval.",
        )
        .with_request_id(request_id),
    )
    .into_response();
    if let Ok(value) = HeaderValue::from_str(&retry_after_secs.to_string()) {
        response.headers_mut().insert(header::RETRY_AFTER, value);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::routing::{get, post};
    use axum::Router;
    use std::sync::Mutex as StdMutex;
    use tower::ServiceExt;

    /// 假时钟（测试驱动补充曲线；advance 只进不退）。
    #[derive(Clone, Default)]
    struct FakeClock(Arc<StdMutex<f64>>);

    impl FakeClock {
        fn advance(&self, secs: f64) {
            *self.0.lock().unwrap() += secs;
        }
    }

    impl RateClock for FakeClock {
        fn now_secs(&self) -> f64 {
            *self.0.lock().unwrap()
        }
    }

    fn params(capacity: u64, refill: f64) -> BucketParams {
        BucketParams {
            capacity,
            refill_per_second: refill,
        }
    }

    fn config(default: BucketParams, strict: BucketParams) -> RateLimitConfig {
        RateLimitConfig {
            default,
            strict,
            max_entries: 1_024,
        }
    }

    fn limiter(clock: FakeClock, default: BucketParams, strict: BucketParams) -> RateLimiter {
        RateLimiter::new(config(default, strict), Arc::new(clock)).expect("limiter")
    }

    fn app_with(state: RateLimitState) -> Router {
        Router::new()
            .route("/api/v1/ping", get(|| async { "ok" }))
            .route("/healthz", get(|| async { "ok" }))
            .route("/api/v1/auth/login", get(|| async { "ok" }))
            .route("/api/v1/submission-tokens", post(|| async { "ok" }))
            .route("/api/v1/sessions", post(|| async { "ok" }))
            .route("/api/v1/executions/{id}/cancel", post(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(state, guard))
    }

    async fn send(app: &Router, uri: &str) -> Response {
        app.clone()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("infallible")
    }

    async fn send_post(app: &Router, uri: &str) -> Response {
        app.clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(uri)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("infallible")
    }

    async fn send_with_principal(app: &Router, uri: &str, principal: &str) -> Response {
        let mut request = Request::builder()
            .uri(uri)
            .body(Body::empty())
            .expect("request");
        request.extensions_mut().insert(AuthenticatedSession {
            organization_id: "org-a".into(),
            principal_id: principal.into(),
            session_id: "sid".into(),
            permission_version: 1,
            token_epoch: 1,
            auth_mode: crate::storage::login_sessions::LoginSessionMode::Cookie,
        });
        app.clone().oneshot(request).await.expect("infallible")
    }

    async fn send_with_ip(app: &Router, uri: &str, ip: &str) -> Response {
        let mut request = Request::builder()
            .uri(uri)
            .body(Body::empty())
            .expect("request");
        let addr: std::net::SocketAddr = format!("{ip}:44444").parse().expect("addr");
        request.extensions_mut().insert(ConnectInfo(addr));
        app.clone().oneshot(request).await.expect("infallible")
    }

    #[tokio::test]
    async fn burst_beyond_capacity_returns_429() {
        let clock = FakeClock::default();
        let state = RateLimitState {
            limiter: Arc::new(limiter(clock.clone(), params(2, 0.001), params(2, 0.001))),
        };
        let app = app_with(state);
        assert_eq!(send(&app, "/api/v1/ping").await.status(), 200);
        assert_eq!(
            send(&app, "/api/v1/ping").await.status(),
            200,
            "突发吃满容量"
        );
        assert_eq!(
            send(&app, "/api/v1/ping").await.status(),
            429,
            "第 3 发超限"
        );
        // 补充速率极低且时钟未动：立刻重试仍是 429。
        assert_eq!(send(&app, "/api/v1/ping").await.status(), 429);
    }

    #[tokio::test]
    async fn rejected_responses_carry_retry_after_and_the_api_error_body() {
        let clock = FakeClock::default();
        let state = RateLimitState {
            limiter: Arc::new(limiter(clock.clone(), params(1, 0.5), params(1, 0.5))),
        };
        let app = app_with(state);
        assert_eq!(send(&app, "/api/v1/ping").await.status(), 200);
        let rejected = send(&app, "/api/v1/ping").await;
        assert_eq!(rejected.status(), 429);
        let retry_after = rejected
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .expect("Retry-After must exist (§9.2 429 行)")
            .to_string();
        let seconds: u64 = retry_after.parse().expect("integer seconds");
        assert!(
            seconds >= 1,
            "Retry-After 下限 1 秒（0 秒会诱发忙等）: {retry_after}"
        );
        // 2 枚余量的缺口 / 0.5 每秒 = 2 秒。
        assert_eq!(seconds, 2);
        let body = axum::body::to_bytes(rejected.into_body(), 64 * 1024)
            .await
            .expect("body");
        let parsed: serde_json::Value = serde_json::from_slice(&body).expect("ApiError JSON");
        assert_eq!(parsed["code"], "rateLimited");
        assert_eq!(parsed["retryDisposition"], "never");
        assert!(parsed["message"].is_string());
    }

    #[tokio::test]
    async fn tokens_refill_over_the_injected_clock_and_cap_at_capacity() {
        let clock = FakeClock::default();
        let state = RateLimitState {
            limiter: Arc::new(limiter(clock.clone(), params(2, 1.0), params(2, 1.0))),
        };
        let app = app_with(state);
        assert_eq!(send(&app, "/api/v1/ping").await.status(), 200);
        assert_eq!(send(&app, "/api/v1/ping").await.status(), 200);
        assert_eq!(send(&app, "/api/v1/ping").await.status(), 429);
        clock.advance(1.0); // +1 枚令牌（速率 1/秒）
        assert_eq!(send(&app, "/api/v1/ping").await.status(), 200);
        assert_eq!(send(&app, "/api/v1/ping").await.status(), 429);
        clock.advance(60.0); // 长闲期：补充封顶在容量（2），不是无限累积
        assert_eq!(send(&app, "/api/v1/ping").await.status(), 200);
        assert_eq!(send(&app, "/api/v1/ping").await.status(), 200);
        assert_eq!(send(&app, "/api/v1/ping").await.status(), 429);
    }

    #[tokio::test]
    async fn principals_get_isolated_buckets() {
        let clock = FakeClock::default();
        let state = RateLimitState {
            limiter: Arc::new(limiter(clock.clone(), params(1, 0.001), params(1, 0.001))),
        };
        let app = app_with(state);
        // user-1 打满自己的桶。
        assert_eq!(
            send_with_principal(&app, "/api/v1/ping", "user-1")
                .await
                .status(),
            200
        );
        assert_eq!(
            send_with_principal(&app, "/api/v1/ping", "user-1")
                .await
                .status(),
            429
        );
        // user-2 的桶独立（§9.1：按 principal 限流，不连坐）。
        assert_eq!(
            send_with_principal(&app, "/api/v1/ping", "user-2")
                .await
                .status(),
            200
        );
        assert_eq!(
            send_with_principal(&app, "/api/v1/ping", "user-2")
                .await
                .status(),
            429
        );
    }

    #[tokio::test]
    async fn unauthenticated_requests_fall_back_to_client_ip_keys() {
        let clock = FakeClock::default();
        let state = RateLimitState {
            limiter: Arc::new(limiter(clock.clone(), params(1, 0.001), params(1, 0.001))),
        };
        let app = app_with(state);
        assert_eq!(
            send_with_ip(&app, "/api/v1/auth/login", "203.0.113.7")
                .await
                .status(),
            200
        );
        assert_eq!(
            send_with_ip(&app, "/api/v1/auth/login", "203.0.113.7")
                .await
                .status(),
            429,
            "同 IP 的未认证请求共用一个桶"
        );
        assert_eq!(
            send_with_ip(&app, "/api/v1/auth/login", "203.0.113.8")
                .await
                .status(),
            200,
            "另一 IP 独立"
        );
        // 无 ConnectInfo 的驱动归并进 unattributed 桶（显式可预期，不 panic）。
        assert_eq!(send(&app, "/api/v1/auth/login").await.status(), 200);
    }

    #[tokio::test]
    async fn strict_routes_use_the_strict_bucket() {
        let clock = FakeClock::default();
        let state = RateLimitState {
            limiter: Arc::new(limiter(
                clock.clone(),
                params(10, 0.001), // 默认档宽松
                params(1, 0.001),  // 严格档紧
            )),
        };
        let app = app_with(state);
        // 严格档路径（契约 §6.2 的令牌签发）只放 1 发。
        assert_eq!(
            send_post(&app, "/api/v1/submission-tokens").await.status(),
            200
        );
        assert_eq!(
            send_post(&app, "/api/v1/submission-tokens").await.status(),
            429
        );
        // 开会话同属严格档。
        assert_eq!(send_post(&app, "/api/v1/sessions").await.status(), 429);
        // 默认档路径不受严格档余量影响。
        assert_eq!(send(&app, "/api/v1/ping").await.status(), 200);
        // 取消族后缀（§5.2 的 POST /cancel 语义）进严格档。
        assert_eq!(
            send_post(&app, "/api/v1/executions/ex-1/cancel")
                .await
                .status(),
            429
        );
    }

    #[tokio::test]
    async fn non_api_paths_are_not_limited() {
        let clock = FakeClock::default();
        let state = RateLimitState {
            limiter: Arc::new(limiter(clock.clone(), params(1, 0.001), params(1, 0.001))),
        };
        let app = app_with(state);
        // §3.3：/healthz 只到 L0——探针不限流。
        assert_eq!(send(&app, "/healthz").await.status(), 200);
        assert_eq!(send(&app, "/healthz").await.status(), 200);
        assert_eq!(send(&app, "/healthz").await.status(), 200);
    }

    #[tokio::test]
    async fn the_bucket_table_evicts_the_least_recently_active_entry_at_capacity() {
        let clock = FakeClock::default();
        let mut config = config(params(1, 0.001), params(1, 0.001));
        config.max_entries = 1;
        let state = RateLimitState {
            limiter: Arc::new(RateLimiter::new(config, Arc::new(clock.clone())).expect("limiter")),
        };
        let app = app_with(state);
        // 两个 principal 争 1 个槽：旧桶被逐出，新桶可用——表长不越界。
        assert_eq!(
            send_with_principal(&app, "/api/v1/ping", "user-1")
                .await
                .status(),
            200
        );
        assert_eq!(
            send_with_principal(&app, "/api/v1/ping", "user-2")
                .await
                .status(),
            200
        );
        // user-1 的桶已被逐出 → 重新从满桶开始（再放 1 发）。
        assert_eq!(
            send_with_principal(&app, "/api/v1/ping", "user-1")
                .await
                .status(),
            200
        );
        assert_eq!(
            send_with_principal(&app, "/api/v1/ping", "user-1")
                .await
                .status(),
            429
        );
    }

    #[test]
    fn limiter_construction_rejects_invalid_params() {
        let clock: Arc<dyn RateClock> = Arc::new(FakeClock::default());
        assert!(RateLimiter::new(config(params(0, 1.0), params(1, 1.0)), clock.clone()).is_err());
        assert!(RateLimiter::new(config(params(1, 0.0), params(1, 1.0)), clock.clone()).is_err());
        assert!(
            RateLimiter::new(config(params(1, f64::NAN), params(1, 1.0)), clock.clone()).is_err()
        );
        let mut broken = config(params(1, 1.0), params(1, 1.0));
        broken.max_entries = 0;
        assert!(RateLimiter::new(broken, clock).is_err());
    }

    #[test]
    fn classify_pins_the_documented_strict_set() {
        // §9.1 严格档清单的现行可判定子集（契约 §6 已列路由）。
        assert_eq!(
            RateLimiter::classify(&Method::POST, "/api/v1/submission-tokens"),
            RouteClass::Strict
        );
        assert_eq!(
            RateLimiter::classify(&Method::POST, "/api/v1/sessions"),
            RouteClass::Strict
        );
        assert_eq!(
            RateLimiter::classify(&Method::POST, "/api/v1/executions/ex-1/cancel"),
            RouteClass::Strict
        );
        // W8 刀1对账：执行域提交路径（契约 §6.10 补行）。
        assert_eq!(
            RateLimiter::classify(&Method::POST, "/api/v1/executions"),
            RouteClass::Strict
        );
        assert_eq!(
            RateLimiter::classify(&Method::POST, "/api/v1/sessions/sess-1/executions"),
            RouteClass::Strict
        );
        assert_eq!(
            RateLimiter::classify(&Method::POST, "/api/v1/ai-host/execute-target"),
            RouteClass::Strict
        );
        // 过渡域 driver-gateway 的提交执行同词尾同语义。
        assert_eq!(
            RateLimiter::classify(&Method::POST, "/api/v1/driver-gateway/executions"),
            RouteClass::Strict
        );
        // 流式命令不是提交执行词尾（`-` 前缀不匹配 `/executions`）。
        assert_eq!(
            RateLimiter::classify(&Method::POST, "/api/v1/driver-gateway/stream-executions"),
            RouteClass::Default
        );
        // 读、集合类与其他方法 → 默认档。
        assert_eq!(
            RateLimiter::classify(&Method::GET, "/api/v1/submission-tokens"),
            RouteClass::Default
        );
        assert_eq!(
            RateLimiter::classify(&Method::GET, "/api/v1/connections"),
            RouteClass::Default
        );
        assert_eq!(
            RateLimiter::classify(&Method::POST, "/api/v1/auth/login"),
            RouteClass::Default
        );
    }
}
