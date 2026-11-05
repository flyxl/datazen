//! WR-D5：`EventSink` 的 SSE 适配（team-server §8 的 server 落地）。
//!
//! 端口侧（[`MemoryEventSink`]，WR-D5 前置刀）只做「事件事实的中继」；本模块
//! 承接它模块头登记的全部 SSE 层职责：
//!
//! * **streamId 订阅建立**（契约 §6.6 的「订阅建立路由随实现登记 v1.1」）：
//!   `POST /api/v1/events/subscriptions`（认证链内；写方法 → L4 CSRF 适用，
//!   这是普通 fetch 而非 `EventSource`）→ `{"streamId"}`。streamId 用
//!   CSPRNG 铸造（仿 [`crate::csrf`]），注册表进程内单例（单实例 v1），
//!   带 TTL 与容量上界（构造注入；TTL 过期 = 未知流，404）。
//! * **握手鉴权**（§8.1）：`GET /api/v1/events/{streamId}` 免 CSRF（
//!   `EventSource` 无法设头），替代证明 = 会话 cookie（L2 已做）+ Origin /
//!   Fetch-Metadata 校验 + L6 授权（挂载点在 `build_router_with_auth` 的
//!   授权链内，随主线接线）。Origin 规则逐字照 §8.1：存在时必须等值于
//!   配置的 UI origin；缺失时要求 `Sec-Fetch-Site: same-origin`；两者都缺
//!   拒绝；`cross-site` / `same-site` / `Origin: null` 不豁免。
//! * **续传翻译**（§8.2）：`Last-Event-ID` 头优先于 `?afterSequence=`（契约
//!   §6.6 明文）；十进制字符串解析（前端不 `Number()`，服务端同样按字符串
//!   收）。subscribe 的**任何 Err** → 200 `text/event-stream` 内先发
//!   `streamResetRequired` 帧再关流（端口模块头的登记裁决：缺口不是 404）。
//! * **慢消费者 / 积压上限**（§8.2）：首波（心跳前）投递超过构造上界（建议
//!   256，连接管理 §7.7 的每订阅投递上限）→ 先发 `streamResetRequired` 再
//!   关流；稳态投递无上限（事件由端口游标语义保证连续，交付缺口以迭代器
//!   `None` 终止暴露 → 同款 reset + 关流）。
//! * **心跳与周期复核**（§8.3/§8.4）：每 `keepalive_interval`（默认 15s）
//!   发 `: keepalive` 注释行（不占 sequence、不进回放缓冲），同一周期复核
//!   登录行（[`LoginSessions::peek`]——**只读**，心跳不刷新登录空闲期）与
//!   组织 `permission_version`；任一不满足 → 先发终态事件
//!   （`auth_required` / `permission_revoked`）再关流。
//!
//! 线程模型：端口的订阅产物是**阻塞迭代器**（端口模块头明文），不能进
//! async 任务——由桥接线程把迭代器搬到 `mpsc` 通道，async 任务只做 select
//! （心跳/复核 ‖ 事件转发）。客户端断开（通道关闭）即整流终止，桥接线程
//! 随 `blocking_send` 失败退出。
//!
//! wire 形态：SSE 帧为 `id: <sequence>` / `event: <payload kind>` /
//! `data: <envelope JSON>`。`EventEnvelope` 逐字段 serde（camelCase），与
//! 桌面 IPC 的信封同形；`session_handle` 是运行时值（§4.4 永不落盘），
//! wire 传输保留——归档白名单是另一条路径，注册为 E2 消费者接入时的
//! 复核项。适配层合成的 `streamResetRequired` 帧没有 `id` 行（端口不伪造
//! sequence，CM-55 的「sequence 连续」契约不受污染）。
//!
//! **挂载点（主线接线）**：`POST /api/v1/events/subscriptions` 与
//! `GET /api/v1/events/{streamId}` 挂进 `build_router_with_auth` 的认证链
//! （L2–L6），配置字段随 `config.rs` 落地。

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use rand::RngCore;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;

use datazen::execution::application::event_sink::EventSink;
use datazen::execution::application::{ConnectionEvent, ConnectionEventEnvelope};
use datazen::shared::context::RequestContext;
use datazen::shared::error::{ApiError, PortError};
use datazen::shared::id::{Counter, OrganizationId, PrincipalId, RequestId, StreamId};
use datazen_driver_api::QueryStreamEvent;

use crate::http::middleware::auth::AuthenticatedSession;
use crate::http::response::{status_for, ApiErrorResponse};
use crate::policy::PolicyService;
use crate::storage::login_sessions::LoginSessions;

/// 默认回放积压上限（连接管理 §7.7 的每订阅 256 条建议值）。
pub const DEFAULT_MAX_BACKLOG_FRAMES: usize = 256;
/// 默认心跳间隔（§8.3 的 15s 建议值；来自部署配置）。
pub const DEFAULT_KEEPALIVE: Duration = Duration::from_secs(15);
/// 默认订阅登记 TTL（握手必须在窗口内发生；来自部署配置）。
pub const DEFAULT_SUBSCRIPTION_TTL: Duration = Duration::from_secs(300);
/// 默认注册表容量（未消费的 streamId 是内存攻击面）。
pub const DEFAULT_REGISTRY_CAPACITY: usize = 4_096;

/// 服务端事件信封的运行时纪元（撤销桥与 driverGateway 流投递桥同一来源）。
pub const SERVER_RUNTIME_EPOCH: &str = "server-v1";

/// SSE 适配层配置（全部来自部署配置；§8 各建议值的默认在这里）。
#[derive(Debug, Clone)]
pub struct SseConfig {
    /// 允许的 UI origin（§8.1：Origin 存在时必须等值其一）。
    pub allowed_origins: Vec<String>,
    /// 心跳间隔（§8.3 建议值 15s）。
    pub keepalive_interval: Duration,
    /// 首波回放积压上限（超过 → `streamResetRequired` + 关流；§8.2 慢消费者）。
    pub max_backlog_frames: usize,
    /// 订阅登记 TTL。
    pub subscription_ttl: Duration,
    /// 注册表容量。
    pub registry_capacity: usize,
}

impl Default for SseConfig {
    fn default() -> Self {
        Self {
            allowed_origins: Vec::new(),
            keepalive_interval: DEFAULT_KEEPALIVE,
            max_backlog_frames: DEFAULT_MAX_BACKLOG_FRAMES,
            subscription_ttl: DEFAULT_SUBSCRIPTION_TTL,
            registry_capacity: DEFAULT_REGISTRY_CAPACITY,
        }
    }
}

/// 订阅登记项（streamId → 归属事实；握手时与 L2 的会话身份比对）。
#[derive(Debug, Clone)]
pub struct SubscriptionEntry {
    organization_id: String,
    principal_id: String,
    created_at_unix: u64,
}

#[derive(Default)]
struct RegistryInner {
    entries: HashMap<String, SubscriptionEntry>,
}

/// streamId 注册表。进程内单例（单实例 v1；重启即失效——与 OIDC 待定
/// 事务同一条登记口径）。
pub struct EventStreamRegistry {
    ttl_secs: u64,
    capacity: usize,
    inner: Mutex<RegistryInner>,
}

impl std::fmt::Debug for EventStreamRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventStreamRegistry")
            .field("ttl_secs", &self.ttl_secs)
            .field("capacity", &self.capacity)
            .field("inner", &"entries")
            .finish()
    }
}

impl EventStreamRegistry {
    pub fn new(config: &SseConfig) -> Self {
        Self {
            ttl_secs: config.subscription_ttl.as_secs(),
            capacity: config.registry_capacity,
            inner: Mutex::new(RegistryInner::default()),
        }
    }

    fn prune(inner: &mut RegistryInner, now: u64, ttl_secs: u64) {
        inner
            .entries
            .retain(|_, entry| now.saturating_sub(entry.created_at_unix) <= ttl_secs);
    }

    /// 登记一个订阅：CSPRNG streamId + 归属盖章。表满时拒绝（fail-closed，
    /// 不做逐出——未消费的 id 不是可牺牲的缓存行，逐出会悄悄吞订阅资格）。
    pub fn create(
        &self,
        organization_id: &str,
        principal_id: &str,
        now_unix: u64,
    ) -> Result<StreamId, PortError> {
        let mut inner = self.inner.lock().expect("registry lock");
        Self::prune(&mut inner, now_unix, self.ttl_secs);
        if inner.entries.len() >= self.capacity {
            return Err(PortError::QuotaExceeded(
                "event subscription registry is full".into(),
            ));
        }
        let stream_id = Self::mint_stream_id();
        inner.entries.insert(
            stream_id.as_str().to_string(),
            SubscriptionEntry {
                organization_id: organization_id.to_string(),
                principal_id: principal_id.to_string(),
                created_at_unix: now_unix,
            },
        );
        Ok(stream_id)
    }

    /// 取走一个登记（握手成功即消费：一个 streamId 一次握手——重连走新的
    /// POST 订阅 + 新 streamId，避免旧 id 的重放面）。
    pub fn take(&self, stream_id: &str, now_unix: u64) -> Option<SubscriptionEntry> {
        let mut inner = self.inner.lock().expect("registry lock");
        Self::prune(&mut inner, now_unix, self.ttl_secs);
        inner.entries.remove(stream_id)
    }

    /// CSPRNG 铸造（仿 csrf.rs 的 random 分量；128 位熵）。
    fn mint_stream_id() -> StreamId {
        let mut bytes = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        use base64::Engine as _;
        StreamId::new(format!(
            "evt-{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
        ))
    }
}

/// §8.4 的周期复核端口。心跳周期逐项核对（登录行 / 权限版本），返回
/// 终态事件的 reason；`Ok(())` = 继续投递。
#[async_trait::async_trait]
pub trait SseAccessRecheck: Send + Sync {
    async fn recheck(
        &self,
        session_id: &str,
        organization_id: &str,
        permission_version_snapshot: i64,
    ) -> Result<(), &'static str>;
}

/// 标准复核：登录行只读校验（[`LoginSessions::peek`]，不刷新空闲期）+
/// 组织权限版本比对。时钟经构造注入（测试钉得住过期路径）。
pub struct StandardRecheck {
    sessions: LoginSessions,
    policy: Arc<PolicyService>,
    clock: Arc<dyn datazen::shared::clock::Clock>,
}

impl std::fmt::Debug for StandardRecheck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StandardRecheck")
            .field("sessions", &"LoginSessions")
            .field("policy", &"PolicyService")
            .field("clock", &"Clock")
            .finish()
    }
}

impl StandardRecheck {
    pub fn new(
        sessions: LoginSessions,
        policy: Arc<PolicyService>,
        clock: Arc<dyn datazen::shared::clock::Clock>,
    ) -> Self {
        Self {
            sessions,
            policy,
            clock,
        }
    }
}

#[async_trait::async_trait]
impl SseAccessRecheck for StandardRecheck {
    async fn recheck(
        &self,
        session_id: &str,
        organization_id: &str,
        permission_version_snapshot: i64,
    ) -> Result<(), &'static str> {
        let sessions = self.sessions.clone();
        let session_id = session_id.to_string();
        let now = self.clock.now();
        let live = tokio::task::spawn_blocking(move || sessions.peek(&session_id, now))
            .await
            .map_err(|_| "session_lost")?;
        if live.is_err() {
            return Err("auth_required");
        }
        let policy = self.policy.clone();
        let organization = organization_id.to_string();
        let current = tokio::task::spawn_blocking(move || policy.permission_version(&organization))
            .await
            .map_err(|_| "permission_revoked")?
            .map_err(|_| "permission_revoked")?;
        if current != permission_version_snapshot {
            return Err("permission_revoked");
        }
        Ok(())
    }
}

/// SSE 适配层的共享状态（两个 handler 的 `from_fn_with_state` 注入物）。
#[derive(Clone)]
pub struct SseState {
    pub registry: Arc<EventStreamRegistry>,
    pub sink: Arc<dyn EventSink>,
    pub recheck: Arc<dyn SseAccessRecheck>,
    pub config: Arc<SseConfig>,
    /// unix 秒时钟（注册表 TTL 用；可注入）。
    pub now_unix: Arc<dyn Fn() -> u64 + Send + Sync>,
}

impl std::fmt::Debug for SseState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SseState")
            .field("registry", &self.registry)
            .field("sink", &"EventSink")
            .field("recheck", &"SseAccessRecheck")
            .field("config", &self.config)
            .finish()
    }
}

/// 续传参数（契约 §6.6：`Last-Event-ID` 头优先，`?afterSequence=` 其次）。
#[derive(Debug, Default, Deserialize)]
pub struct ReplayQuery {
    /// 契约 §6.6 的 `afterSequence`（camelCase wire 形态）。
    #[serde(default, rename = "afterSequence")]
    pub after_sequence: Option<String>,
}

fn request_id_of(req: &Request) -> Option<RequestId> {
    req.extensions()
        .get::<Option<RequestId>>()
        .cloned()
        .flatten()
}

fn unauthenticated(request_id: Option<RequestId>) -> Response {
    ApiErrorResponse::new(
        ApiError::new(
            datazen::shared::error::ApiErrorCode::Unauthenticated,
            "Authentication is required to subscribe to event streams.",
        )
        .with_request_id(request_id),
    )
    .with_status(StatusCode::UNAUTHORIZED)
    .into_response()
}

fn bad_request(request_id: Option<RequestId>, message: &'static str) -> Response {
    ApiErrorResponse::new(
        ApiError::new(
            datazen::shared::error::ApiErrorCode::InvalidArgument,
            message,
        )
        .with_request_id(request_id),
    )
    .with_status(StatusCode::BAD_REQUEST)
    .into_response()
}

fn forbidden(request_id: Option<RequestId>) -> Response {
    ApiErrorResponse::new(
        ApiError::new(
            datazen::shared::error::ApiErrorCode::PermissionDenied,
            "The event stream handshake did not pass the same-origin proof.",
        )
        .with_request_id(request_id),
    )
    .with_status(StatusCode::FORBIDDEN)
    .into_response()
}

fn not_found(request_id: Option<RequestId>) -> Response {
    ApiErrorResponse::new(
        ApiError::new(
            datazen::shared::error::ApiErrorCode::NotFound,
            "The event stream subscription is unknown or expired.",
        )
        .with_request_id(request_id),
    )
    .with_status(status_for(datazen::shared::error::ApiErrorCode::NotFound))
    .into_response()
}

/// §8.1 的 Origin / Fetch-Metadata 证明。返回 `Ok(())` 或拒绝原因（日志用，
/// 不进响应——脱敏纪律：拒绝细节不构成探测面）。
fn origin_proof(headers: &HeaderMap, allowed_origins: &[String]) -> Result<(), &'static str> {
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    match origin {
        Some(origin) => {
            if origin == "null" {
                // §8.1：`Origin: null` 不豁免（沙箱/重定向链的伪造面）。
                return Err("origin is null");
            }
            if allowed_origins.iter().any(|allowed| allowed == &origin) {
                return Ok(());
            }
            Err("origin does not match the configured ui origin")
        }
        None => {
            // 同源 GET 不保证带 Origin；缺失时要求 Fetch-Metadata 的同源证明。
            let fetch_site = headers
                .get("sec-fetch-site")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("");
            if fetch_site.eq_ignore_ascii_case("same-origin") {
                Ok(())
            } else {
                Err("no origin and no same-origin fetch metadata")
            }
        }
    }
}

/// 续传位点解析：`Last-Event-ID` 头优先于 `?afterSequence=`；两者都是
/// 十进制字符串（§8.2：前端不 `Number()`，服务端按字符串收）。
fn replay_position(headers: &HeaderMap, query: &ReplayQuery) -> Result<Option<u64>, &'static str> {
    let raw = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .or_else(|| query.after_sequence.clone());
    let Some(raw) = raw else { return Ok(None) };
    if raw.is_empty() {
        return Ok(None);
    }
    raw.parse::<u64>()
        .map(Some)
        .map_err(|_| "last-event-id / afterSequence is not a decimal counter")
}

/// `POST /api/v1/events/subscriptions`：登记订阅 → `{streamId}`。
/// 认证链内挂载（L2 会话身份即归属事实来源）；写方法 → L4 CSRF 适用。
pub async fn create_subscription(State(state): State<SseState>, req: Request) -> Response {
    let request_id = request_id_of(&req);
    let Some(session) = req.extensions().get::<AuthenticatedSession>().cloned() else {
        return unauthenticated(request_id);
    };
    let now_unix = (state.now_unix)();
    match state
        .registry
        .create(&session.organization_id, &session.principal_id, now_unix)
    {
        Ok(stream_id) => Json(json!({ "streamId": stream_id.as_str() })).into_response(),
        Err(PortError::QuotaExceeded(_)) => {
            let mut response = ApiErrorResponse::new(
                ApiError::new(
                    datazen::shared::error::ApiErrorCode::RateLimited,
                    "Too many pending event stream subscriptions.",
                )
                .with_request_id(request_id),
            )
            .with_status(StatusCode::TOO_MANY_REQUESTS)
            .into_response();
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("5"));
            response
        }
        Err(_) => ApiErrorResponse::new(
            ApiError::new(
                datazen::shared::error::ApiErrorCode::ServiceUnavailable,
                "The event subscription registry is unavailable.",
            )
            .with_request_id(request_id),
        )
        .with_status(StatusCode::SERVICE_UNAVAILABLE)
        .into_response(),
    }
}

/// `GET /api/v1/events/{streamId}`：SSE 握手 + 回放 + 实况。
pub async fn events(
    State(state): State<SseState>,
    Path(stream_id): Path<String>,
    Query(query): Query<ReplayQuery>,
    req: Request,
) -> Response {
    let request_id = request_id_of(&req);
    // §8.1 的 Origin / Fetch-Metadata 证明（CSRF 的握手替代）。
    if origin_proof(req.headers(), &state.config.allowed_origins).is_err() {
        return forbidden(request_id);
    }
    // streamId 登记：未知/过期 = 404（不是回放缺口——那是流内
    // streamResetRequired，见下）。
    let Some(entry) = state.registry.take(&stream_id, (state.now_unix)()) else {
        return not_found(request_id);
    };
    // 会话身份与登记归属一致（L2 会话是身份来源；不一致 = 他人票据）。
    let Some(session) = req.extensions().get::<AuthenticatedSession>().cloned() else {
        return unauthenticated(request_id);
    };
    if session.organization_id != entry.organization_id
        || session.principal_id != entry.principal_id
    {
        return forbidden(request_id);
    }
    let after = match replay_position(req.headers(), &query) {
        Ok(after) => after.map(Counter::new),
        Err(_) => return bad_request(request_id, "The replay position is not a decimal counter."),
    };

    // 通道容量 = 首波积压上界（§8.2 的每订阅投递队列语义）。
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(state.config.max_backlog_frames);
    let sink = state.sink.clone();
    let ctx = RequestContext::new(
        OrganizationId::new(entry.organization_id.clone()),
        PrincipalId::new(entry.principal_id.clone()),
        None,
        datazen::shared::id::ClientInstanceId::new("sse"),
        request_id
            .clone()
            .unwrap_or_else(|| RequestId::new("req-sse")),
        None,
    );
    let stream_id_value = StreamId::new(stream_id.clone());

    // 订阅：任何 Err → streamResetRequired + 关流（端口模块头的裁决；
    // 缺口不是 404）。未知流按「空窗口 + 等实时」成立（端口语义）。
    let mut subscription = match sink.subscribe(&ctx, stream_id_value, after).await {
        Ok(subscription) => subscription,
        Err(_) => {
            let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(1);
            let _ = tx
                .send(Ok(reset_event(
                    "the requested replay window is no longer buffered",
                )))
                .await;
            return sse_response(tokio_stream::wrappers::ReceiverStream::new(rx));
        }
    };

    // 阻塞迭代器 → 通道的桥接线程（端口模块头：订阅产物不能进 async 任务）。
    let (event_tx, mut event_rx) = mpsc::channel::<BridgeMessage>(state.config.max_backlog_frames);
    std::thread::spawn(move || {
        loop {
            match subscription.next() {
                Some(envelope) => {
                    if event_tx
                        .blocking_send(BridgeMessage::Envelope(envelope))
                        .is_err()
                    {
                        return; // 消费端断开
                    }
                }
                None => {
                    // 交付缺口（游标被淘汰出窗口）→ 通知异步侧 reset + 关流。
                    let _ = event_tx.blocking_send(BridgeMessage::Exhausted);
                    return;
                }
            }
        }
    });

    // 异步侧：心跳/复核 ‖ 事件转发 的 select 循环。
    let recheck = state.recheck.clone();
    let keepalive = state.config.keepalive_interval;
    let backlog_cap = state.config.max_backlog_frames as u64;
    let session_id = session.session_id.clone();
    let organization_id = session.organization_id.clone();
    let permission_snapshot = session.permission_version;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(keepalive);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        interval.tick().await; // 首个 tick 立即返回，跳过
        let mut delivered: u64 = 0;
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // §8.3：`: keepalive` 注释行，不占 sequence。
                    if tx.send(Ok(Event::default().comment("keepalive"))).await.is_err() {
                        return; // 客户端断开
                    }
                    // §8.4：每个心跳周期复核（登录行只读 / 权限版本）。
                    match recheck
                        .recheck(&session_id, &organization_id, permission_snapshot)
                        .await
                    {
                        Ok(()) => {}
                        Err(reason) => {
                            let _ = tx.send(Ok(terminal_event(reason))).await;
                            return; // 先发终态事件再关流
                        }
                    }
                }
                message = event_rx.recv() => {
                    match message {
                        Some(BridgeMessage::Envelope(envelope)) => {
                            match frame_of(&envelope) {
                                Ok(frame) => {
                                    if tx.send(Ok(frame)).await.is_err() {
                                        return; // 客户端断开
                                    }
                                    delivered += 1;
                                    // 首波积压超上界 → reset + 关流（§8.2 慢消费者）。
                                    if delivered >= backlog_cap {
                                        let _ = tx
                                            .send(Ok(reset_event(
                                                "the consumer fell behind the replay window",
                                            )))
                                            .await;
                                        return;
                                    }
                                }
                                Err(()) => {
                                    // 信封序列化失败：发合成 reset（端口词汇内
                                    // 最接近的终局；真实信封永不坏——这是防御面）。
                                    let _ = tx
                                        .send(Ok(reset_event("envelope serialization failed")))
                                        .await;
                                    return;
                                }
                            }
                        }
                        Some(BridgeMessage::Exhausted) | None => {
                            // 迭代器终止 = 交付缺口（None 断流语义）。
                            let _ = tx
                                .send(Ok(reset_event(
                                    "the consumer fell behind the replay window",
                                )))
                                .await;
                            return;
                        }
                    }
                }
            }
        }
    });

    sse_response(tokio_stream::wrappers::ReceiverStream::new(rx))
}

/// 桥接线程 → 异步侧的消息（信封或「迭代器终止」信号）。
enum BridgeMessage {
    Envelope(ConnectionEventEnvelope),
    Exhausted,
}

fn sse_response(
    stream: tokio_stream::wrappers::ReceiverStream<Result<Event, Infallible>>,
) -> Response {
    // 心跳由 select 循环自己驱动（保持 keepalive 与周期复核同一节拍，
    // §8.4 的「每个心跳周期复核」）；axum 的内建 KeepAlive 不启用。
    let mut response = Sse::new(stream).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

/// 信封 → SSE 帧：`id: <sequence>` / `event: <kind>` / `data: <json>`。
/// 序列化失败 = Err（防御面；端口产出的信封是合法 JSON 形状）。
///
/// 词汇映射表（W8-b 登记）：统一事件载荷 → `event:` 行字面量。
/// * `SessionChanged` → `sessionChanged`、`ExecutionChanged` → `executionChanged`、
///   `ResultChunk` → `resultChunk`（既有词汇）；
/// * `QueryStream` → 桌面 `QueryStreamEvent` 的 `type` tag 原样
///   （executionStarted / statementStart / rows / statementEnd / done）——
///   driverGateway 流投递桥按桌面词汇最小登记，映射表全貌见
///   `driver_gateway.rs` 模块头；
/// * `StreamResetRequired` → `streamResetRequired`（既有词汇）。
fn frame_of(envelope: &ConnectionEventEnvelope) -> Result<Event, ()> {
    let kind = match &envelope.payload {
        ConnectionEvent::SessionChanged { .. } => "sessionChanged",
        ConnectionEvent::ExecutionChanged { .. } => "executionChanged",
        ConnectionEvent::ResultChunk(_) => "resultChunk",
        ConnectionEvent::QueryStream(event) => query_stream_kind(event),
        ConnectionEvent::StreamResetRequired { .. } => "streamResetRequired",
    };
    let data = serde_json::to_string(envelope).map_err(|_| ())?;
    Ok(Event::default()
        .id(envelope.sequence.get().to_string())
        .event(kind)
        .data(data))
}

/// 桌面 `QueryStreamEvent` 种类 → SSE `event:` 行字面量（wire `type` tag
/// 的同名词汇，见 `datazen_driver_api::query_stream` 的 wire 测试）。
fn query_stream_kind(event: &QueryStreamEvent) -> &'static str {
    match event {
        QueryStreamEvent::ExecutionStarted { .. } => "executionStarted",
        QueryStreamEvent::StatementStart { .. } => "statementStart",
        QueryStreamEvent::Rows { .. } => "rows",
        QueryStreamEvent::StatementEnd { .. } => "statementEnd",
        QueryStreamEvent::Done { .. } => "done",
    }
}

/// 适配层合成的 `streamResetRequired` 帧（无 `id` 行——端口不伪造
/// sequence，CM-55 的「sequence 连续」契约不受污染）。
fn reset_event(reason: &str) -> Event {
    Event::default()
        .event("streamResetRequired")
        .data(json!({ "reason": reason }).to_string())
}

/// §8.4 的终态事件帧（`auth_required` / `permission_revoked` / `session_lost`）。
fn terminal_event(reason: &str) -> Event {
    Event::default()
        .event(reason)
        .data(json!({ "reason": reason }).to_string())
}
