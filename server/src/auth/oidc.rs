//! OIDC RP（依赖方）登录流：发现 → 授权码 + PKCE(S256) → 回调 → 会话行 +
//! 会话 cookie（team-server §4.1 时序、§4.2 校验表、§4.4 cookie 属性）。
//!
//! # 组成
//!
//! - **发现**（[`OidcRelyingParty::discovery`]）：拉取
//!   `{issuer}/.well-known/openid-configuration`，TTL 内缓存、过期重取；
//!   响应的 `issuer` 与配置的 issuer **精确匹配**（字符串等值），不匹配
//!   拒绝服务（上游完整性事实，HTTP 层 503）。
//! - **发起**（[`login`]）：生成 state / nonce / PKCE(verifier, challenge
//!   = BASE64URL(SHA256(verifier)))，**只发 S256**（§4.2 禁 plain）；
//!   待定授权事务存**进程内存**（§4.6 先例口径：单进程 v1，带 TTL 与
//!   一次性消费，服务重启时失效并重新登录），302 → IdP 授权端点，并下发
//!   `__Host-dz_oidc_state` cookie（§4.4：HttpOnly + Lax + Secure +
//!   Path=/，Max-Age = state TTL）。
//! - **回调**（[`callback`]）：query `state` 与浏览器 state cookie
//!   **常量时间比较**（[`super::claims::constant_time_eq`]，§4.2 state 行）；
//!   从内存事务表**取出即焚**（一次性消费）+ TTL 检查；code + verifier
//!   到 token 端点交换；id_token 经 JWKS 校验（`kid` 缓存 + 未知 kid 触发
//!   **一次**刷新，§4.2 签名行）；claims 手工校验（§4.2 nonce/azp/sub/iat，
//!   [`super::claims`]）；membership 映射（403 = 未登记/禁用，不隐式开户）；
//!   [`crate::storage::LoginSessions::create`] 落会话行；下发与
//!   [`crate::http::middleware::auth`] 逐字一致的会话 cookie（名取
//!   `auth::SESSION_COOKIE`，§4.4 属性），清除 state cookie，302 回应用。
//!
//! # 时长与容量（部署配置，非协议常量）
//!
//! §4.1「本节所有时长都是首版建议值，来自部署配置」——state TTL、时钟偏移
//! 容差、JWKS / 发现文档缓存 TTL、HTTP 超时、会话 TTL、待定事务容量全部
//! 是 [`OidcConfig`] 的构造参数，本模块没有任何写死的协议时长。
//!
//! # 脱敏纪律（`crate::logging` 的契约）
//!
//! 授权码 / verifier / client_secret / state **不进日志**：server crate 尚
//! 未依赖 datazen-infra redaction（见 server/Cargo.toml 注记，它随真实
//! handler 切片引入），因此本模块以**结构**保证——错误变体只携带静态描述
//! 串（[`FlowRejection`]），日志语句从不输出请求参数或上游响应体；日志
//! 恢复时不存在需要回溯脱敏的字段。
//!
//! # HTTP 映射（§9.2）
//!
//! state 无效/重放/过期、缺 code → 400 `InvalidArgument`；token 交换与
//! id_token 校验失败 → 401 `Unauthenticated`；membership 未映射 → 403
//! `PermissionDenied`；待定事务表满 → 429 `QuotaExceeded`（带
//! `Retry-After`）；上游不可达/发现不匹配/存储失败 → 503
//! `ServiceUnavailable`。路由挂载与豁免（§3.3）由主线接线，见模块报告。

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Query, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::csrf::constant_time_eq;

use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::shared::id::RequestId;

use crate::http::middleware::auth::SESSION_COOKIE;
use crate::http::middleware::session_cookie;
use crate::http::response::ApiErrorResponse;
use crate::policy::PolicyService;
use crate::storage::login_sessions::{LoginSessionMode, LoginSessions, NewLoginSession};
use crate::storage::AdminStore;

use super::claims::{self, IdTokenClaims};

/// 登录事务 state 的浏览器 cookie 名（§4.4：`__Host-` 前缀强制 `Secure`、
/// 无 `Domain`、`Path=/`；`HttpOnly` + `SameSite=Lax`；10 分钟级 TTL 来自
/// 部署配置；单次消费后由回调以 `Max-Age=0` 删除）。
pub const STATE_COOKIE: &str = "__Host-dz_oidc_state";

/// 待定授权事务表满时 429 的 `Retry-After` 建议值（秒；响应头建议，不是
/// 协议常量——表满即「稍后重试」，不精确承诺可用时刻）。
const PENDING_FULL_RETRY_AFTER_SECS: u64 = 5;

/// 随机字节数：state / nonce 各 32 字节（256-bit，超过 §4.2 state 的
/// 128-bit 下限），PKCE verifier 64 字节（base64url 后 86 字符，落在
/// RFC 7636 的 43–128 字符区间）。
const STATE_BYTES: usize = 32;
const NONCE_BYTES: usize = 32;
const VERIFIER_BYTES: usize = 64;
/// 会话 id 字节数（§4.3：≥128-bit 随机、不透明、不可从 principalId 推导；
/// 256-bit 与 `crate::csrf` 的 random 分量同档）。
const SESSION_ID_BYTES: usize = 32;

// ---------------------------------------------------------------------------
// 配置（部署注入，非协议常量）
// ---------------------------------------------------------------------------

/// client_secret 的提交形态（RFC 6749 §2.3.1 的两种；PKCE 公开客户端则为
/// `None`——本服务是机密客户端形态，但 secret 可选以便对接纯 PKCE IdP）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientAuthStyle {
    /// HTTP Basic（`client_secret_basic`）。
    Basic,
    /// 请求体字段（`client_secret_post`）。
    Post,
}

/// client_secret 值 + 提交形态。`Debug` 刻意脱敏：secret 不进日志（与
/// `ServerSecret` / `CsrfSecret` 同一条纪律）。
#[derive(Clone)]
pub struct ClientSecret {
    pub value: String,
    pub auth_style: ClientAuthStyle,
}

impl std::fmt::Debug for ClientSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClientSecret(<redacted>)")
    }
}

/// RP 侧部署配置。全部字段由装配点（bootstrap / 测试）注入；本模块不读
/// 环境变量（`crate::config` 是唯一环境入口，接线时由它喂入）。
#[derive(Clone)]
pub struct OidcConfig {
    /// IdP issuer（发现文档的 issuer 必须**精确等值**，§4.2 iss 行）。
    pub issuer: String,
    /// 本服务在 IdP 登记的 client id。
    pub client_id: String,
    /// 机密客户端凭据；`None` = 纯 PKCE 公开客户端。
    pub client_secret: Option<ClientSecret>,
    /// 本服务所属组织（§4.2 membership 行「本组织」；v1 单组织部署）。
    pub organization_id: String,
    /// 对外可见 origin（构造 `redirect_uri` 用；同源 HTTPS 由部署保证）。
    pub public_origin: String,
    /// 回调路径（契约 §5：`/api/v1/auth/callback`；与 `redirect_uri`
    /// 的 path 分量必须一致）。
    pub callback_path: String,
    /// 授权请求的 scope（空格分隔，至少含 `openid`）。
    pub scope: String,
    /// 登录成功后的跳转目标（§4.1 最后一步「302 回应用」）。
    pub post_login_redirect: String,
    // —— 时长与容量（§4.1：来自部署配置，非协议常量）——
    /// 上游 HTTP 调用（发现/JWKS/token）的每次超时。
    pub http_timeout: Duration,
    /// 发现文档缓存 TTL（过期重取）。
    pub discovery_ttl: Duration,
    /// JWKS 缓存 TTL（过期重取；未知 kid 触发的刷新独立于此）。
    pub jwks_ttl: Duration,
    /// id_token 时钟偏移容差（§4.2「允许 60 秒」是建议值）。
    pub clock_skew_leeway: Duration,
    /// state 事务 TTL（§4.2「TTL 10 分钟」是建议值；cookie Max-Age 同值）。
    pub state_ttl: Duration,
    /// 待定授权事务的容量上限（内存界；满时先清过期，仍满则 429）。
    pub pending_capacity: usize,
    // —— 会话行参数（与认证链同源的部署值，§4.3）——
    /// 登录会话空闲寿命（秒；cookie 会话行的 `idle_expires_at`）。
    pub session_idle_seconds: u64,
    /// 登录会话绝对寿命（秒；`absolute_expires_at` 与会话 cookie Max-Age）。
    pub session_absolute_seconds: u64,
}

impl std::fmt::Debug for OidcConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // ClientSecret 已脱敏，但 config 整体不参与日志输出——只报形状。
        f.debug_struct("OidcConfig")
            .field("issuer", &self.issuer)
            .field("client_id", &self.client_id)
            .field("organization_id", &self.organization_id)
            .field("public_origin", &self.public_origin)
            .field("callback_path", &self.callback_path)
            .field("state_ttl", &self.state_ttl)
            .finish_non_exhaustive()
    }
}

/// 时钟源（可注入：测试用假时钟钉 state TTL / 缓存 TTL 的边界）。
pub trait OidcClock: Send + Sync {
    /// unix 秒。
    fn now_unix_secs(&self) -> u64;
}

/// 真实时钟。
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl OidcClock for SystemClock {
    fn now_unix_secs(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// RP 本体
// ---------------------------------------------------------------------------

/// 发现文档投影（只取本流消费的四个字段）。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct DiscoveryDocument {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
}

#[derive(Debug, Clone)]
struct CachedDiscovery {
    fetched_at: u64,
    doc: DiscoveryDocument,
}

/// JWKS 缓存：`kid` → 解码钥（§4.2 签名行「按 kid 缓存轮换」）。`fetched_at`
/// 驱动 `jwks_ttl` 过期重取：缓存命中但已过期同样视为 miss（钥轮换的慢路径）。
#[derive(Clone)]
struct JwksCache {
    fetched_at: u64,
    keys: HashMap<String, jsonwebtoken::DecodingKey>,
}

/// 待定授权事务（§4.6 先例口径：暂存内存，服务重启时失效并重新登录）。
#[derive(Debug, Clone)]
struct PendingAuthorization {
    nonce: String,
    pkce_verifier: String,
    created_at: u64,
}

#[derive(Default)]
struct RpInner {
    discovery: Option<CachedDiscovery>,
    jwks: Option<JwksCache>,
    pending: HashMap<String, PendingAuthorization>,
}

/// OIDC RP。一个进程一份（单进程 v1，待定事务的内存口径）。
pub struct OidcRelyingParty {
    config: OidcConfig,
    http: reqwest::Client,
    store: Arc<AdminStore>,
    sessions: LoginSessions,
    policy: Arc<PolicyService>,
    clock: Arc<dyn OidcClock>,
    inner: Mutex<RpInner>,
}

/// handler 的共享状态（`from_fn`/`get(...)` 的 `State` 注入物）。
pub type OidcState = Arc<OidcRelyingParty>;

impl OidcRelyingParty {
    /// 构造。配置非法（origin/路径/寿命窗口/容量）即拒绝——fail-closed，
    /// 与 `crate::config` 的校验纪律一致。
    pub fn new(
        config: OidcConfig,
        store: Arc<AdminStore>,
        policy: Arc<PolicyService>,
        sessions: LoginSessions,
        clock: Arc<dyn OidcClock>,
    ) -> Result<Self, String> {
        if !(config.issuer.starts_with("https://") || config.issuer.starts_with("http://")) {
            return Err("oidc: issuer must be an absolute http(s) URL".into());
        }
        if config.client_id.trim().is_empty() {
            return Err("oidc: client_id must not be blank".into());
        }
        if config.organization_id.trim().is_empty() {
            return Err("oidc: organization_id must not be blank".into());
        }
        if !config.public_origin.starts_with("https://")
            && !config.public_origin.starts_with("http://")
        {
            return Err("oidc: public_origin must be an absolute http(s) URL".into());
        }
        if !config.callback_path.starts_with('/') {
            return Err("oidc: callback_path must start with '/'".into());
        }
        if !config.scope.split(' ').any(|scope| scope == "openid") {
            return Err("oidc: scope must include `openid`".into());
        }
        if config.http_timeout.is_zero() {
            return Err("oidc: http_timeout must be positive".into());
        }
        if config.discovery_ttl.is_zero() || config.jwks_ttl.is_zero() || config.state_ttl.is_zero()
        {
            return Err("oidc: cache and state TTLs must be positive".into());
        }
        if config.pending_capacity == 0 {
            return Err("oidc: pending_capacity must be at least 1".into());
        }
        // ck_login_window 的构造期前提（与 crate::config 同一条规则）。
        if config.session_idle_seconds > config.session_absolute_seconds {
            return Err(
                "oidc: session_idle_seconds must not exceed session_absolute_seconds".into(),
            );
        }
        if config.session_absolute_seconds < 60 {
            return Err("oidc: session_absolute_seconds must be at least 60".into());
        }
        let http = reqwest::Client::builder()
            .timeout(config.http_timeout)
            .build()
            .map_err(|error| format!("oidc: http client build failed: {error}"))?;
        Ok(Self {
            config,
            http,
            store,
            sessions,
            policy,
            clock,
            inner: Mutex::new(RpInner::default()),
        })
    }

    fn lock_inner(&self) -> std::sync::MutexGuard<'_, RpInner> {
        // 中毒恢复：缓存/事务表的写操作都是「整项 insert/remove」，中途
        // panic 留下的只是少一条缓存——回退到继续服务比整体拒登更符合
        // fail-open 的缓存语义（会话行的权威在管理库，不在这里）。
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// `redirect_uri`（授权请求与 token 交换必须逐字一致）。
    pub fn redirect_uri(&self) -> String {
        format!(
            "{}{}",
            self.config.public_origin.trim_end_matches('/'),
            self.config.callback_path
        )
    }

    // ── 发起（§4.1 的「生成 state / nonce / PKCE」步） ──

    /// 发起登录：登记待定事务并构造授权跳转。**发起即写内存事务**——
    /// 容量先清过期再检查，仍满则 429（§9.2 的额度口径，见模块报告）。
    pub async fn begin_login(&self) -> Result<BeginLogin, FlowRejection> {
        let doc = self.discovery().await?;
        let now = self.clock.now_unix_secs();
        let state = random_b64url(STATE_BYTES);
        let nonce = random_b64url(NONCE_BYTES);
        let mut verifier_bytes = [0u8; VERIFIER_BYTES];
        RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut verifier_bytes);
        let (verifier, challenge) = pkce_pair(verifier_bytes);

        {
            let mut inner = self.lock_inner();
            prune_pending(&mut inner.pending, self.config.state_ttl.as_secs(), now);
            if inner.pending.len() >= self.config.pending_capacity {
                tracing::warn!("oidc: pending authorization table is full");
                return Err(FlowRejection::PendingTableFull);
            }
            inner.pending.insert(
                state.clone(),
                PendingAuthorization {
                    nonce: nonce.clone(),
                    pkce_verifier: verifier,
                    created_at: now,
                },
            );
        }

        let url = self.authorization_url(&doc, &state, &nonce, &challenge);
        Ok(BeginLogin {
            authorization_url: url,
            state_cookie: cookie_value(STATE_COOKIE, &state, self.config.state_ttl.as_secs()),
        })
    }

    /// 授权端点 URL（§4.2：只发 `code_challenge_method=S256`）。
    fn authorization_url(
        &self,
        doc: &DiscoveryDocument,
        state: &str,
        nonce: &str,
        challenge: &str,
    ) -> String {
        format!(
            "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&nonce={}&code_challenge={}&code_challenge_method=S256",
            doc.authorization_endpoint,
            percent_encode(&self.config.client_id),
            percent_encode(&self.redirect_uri()),
            percent_encode(&self.config.scope),
            percent_encode(state),
            percent_encode(nonce),
            percent_encode(challenge),
        )
    }

    // ── 回调（§4.1 的「常量时间比较 state → 交换 → 校验 → 会话」链） ──

    /// 完成登录。成功产出会话 cookie 与跳转目标；失败按 §9.2 映射（见
    /// [`FlowRejection`]）。`user_agent` 是 §4.3 client_binding 的输入。
    pub async fn complete_login(
        &self,
        query: &CallbackQuery,
        cookie_state: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<LoginOutcome, FlowRejection> {
        // IdP 侧错误（如 access_denied）：透传为 401，描述不回显（脱敏）。
        if query.error.is_some() {
            tracing::warn!("oidc: authorization endpoint returned an error");
            return Err(FlowRejection::UpstreamRejected(
                "the authorization request was denied by the provider",
            ));
        }

        // state 三重校验（§4.2 state 行）：cookie 与 query 常量时间相等 →
        // 内存事务取出即焚（一次性）→ TTL 内。
        let Some(query_state) = query.state.as_deref() else {
            return Err(FlowRejection::StateInvalid);
        };
        let Some(cookie_state) = cookie_state else {
            return Err(FlowRejection::StateInvalid);
        };
        if !constant_time_eq(query_state.as_bytes(), cookie_state.as_bytes()) {
            tracing::warn!("oidc: state cookie and query parameter disagree");
            return Err(FlowRejection::StateInvalid);
        }
        let now = self.clock.now_unix_secs();
        let pending = {
            let mut inner = self.lock_inner();
            inner.pending.remove(query_state)
        };
        let Some(pending) = pending else {
            // 未知/已消费（重放）——「单次消费，不重放」。
            tracing::warn!("oidc: unknown or already consumed login transaction");
            return Err(FlowRejection::StateInvalid);
        };
        if now.saturating_sub(pending.created_at) > self.config.state_ttl.as_secs() {
            tracing::warn!("oidc: login transaction expired");
            return Err(FlowRejection::StateInvalid);
        }
        let Some(code) = query.code.as_deref() else {
            return Err(FlowRejection::CallbackMissingCode);
        };

        // 发现文档（缓存）→ token 端点交换。
        let doc = self.discovery().await?;
        let token = self
            .exchange_code(&doc.token_endpoint, code, &pending.pkce_verifier)
            .await?;
        let Some(id_token) = token.id_token.as_deref() else {
            return Err(FlowRejection::UpstreamRejected(
                "the token response did not include an id_token",
            ));
        };

        // id_token 校验（JWKS/签名/iss/aud/exp + nonce/azp/sub/iat）。
        let token_claims = self
            .validate_id_token(id_token, &doc.jwks_uri, &pending.nonce)
            .await?;

        // membership 映射（§4.2 membership 行：不隐式开户）。
        let identity = claims::resolve_membership(
            &self.store,
            &self.config.organization_id,
            &token_claims.sub,
        )
        .map_err(|_| FlowRejection::Storage)?;
        let Some(identity) = identity else {
            tracing::warn!(
                organization = %self.config.organization_id,
                "oidc: subject has no enabled membership; login refused"
            );
            return Err(FlowRejection::MembershipUnresolved);
        };

        // 会话行（§4.3）：权限版本快照 → 铸 session id → 落行。
        let permission_version = self
            .policy
            .permission_version(&self.config.organization_id)
            .map_err(|_| FlowRejection::Storage)?;
        let session_id = random_b64url(SESSION_ID_BYTES);
        let issued_at = DateTime::<Utc>::from_timestamp(now as i64, 0)
            .ok_or(FlowRejection::UpstreamUnavailable("clock is out of range"))?
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let absolute_expires_at = DateTime::<Utc>::from_timestamp(
            now as i64 + self.config.session_absolute_seconds as i64,
            0,
        )
        .ok_or(FlowRejection::UpstreamUnavailable("clock is out of range"))?
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let idle_expires_at = DateTime::<Utc>::from_timestamp(
            now as i64 + self.config.session_idle_seconds as i64,
            0,
        )
        .ok_or(FlowRejection::UpstreamUnavailable("clock is out of range"))?
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let auth_time = token_claims
            .auth_time
            .and_then(|auth_time| DateTime::<Utc>::from_timestamp(auth_time, 0))
            .unwrap_or_else(|| {
                DateTime::parse_from_rfc3339(&issued_at)
                    .expect("issued_at")
                    .into()
            })
            .with_timezone(&Utc)
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        self.sessions
            .create(NewLoginSession {
                organization_id: self.config.organization_id.clone(),
                session_id: session_id.clone(),
                principal_id: identity.principal_id,
                auth_time,
                issued_at,
                absolute_expires_at,
                idle_expires_at,
                permission_version,
                client_binding: claims::user_agent_digest(user_agent),
                mode: LoginSessionMode::Cookie,
                native_secret_hash: None,
            })
            .map_err(|_| FlowRejection::Storage)?;

        tracing::info!(
            organization = %identity.organization_id,
            role = %identity.role.as_str(),
            "oidc: login session established"
        );
        Ok(LoginOutcome {
            session_cookie: cookie_value(
                SESSION_COOKIE,
                &session_id,
                self.config.session_absolute_seconds,
            ),
            redirect_to: self.config.post_login_redirect.clone(),
        })
    }

    // ── 发现文档（TTL + 过期重取 + issuer 精确匹配） ──

    async fn discovery(&self) -> Result<DiscoveryDocument, FlowRejection> {
        let now = self.clock.now_unix_secs();
        let ttl = self.config.discovery_ttl.as_secs();
        if let Some(doc) = {
            let inner = self.lock_inner();
            inner
                .discovery
                .as_ref()
                .filter(|cached| now.saturating_sub(cached.fetched_at) <= ttl)
                .map(|cached| cached.doc.clone())
        } {
            return Ok(doc);
        }
        let url = format!(
            "{}/.well-known/openid-configuration",
            self.config.issuer.trim_end_matches('/')
        );
        let response =
            self.http.get(&url).send().await.map_err(|_| {
                FlowRejection::UpstreamUnavailable("discovery document is unreachable")
            })?;
        if !response.status().is_success() {
            tracing::warn!(status = %response.status(), "oidc: discovery document request failed");
            return Err(FlowRejection::UpstreamUnavailable(
                "discovery document is unreachable",
            ));
        }
        let doc: DiscoveryDocument = response.json().await.map_err(|_| {
            FlowRejection::UpstreamUnavailable("discovery document is not parseable")
        })?;
        // issuer 精确匹配（§4.2 iss 行的发现侧前提：配置与文档必须逐字
        // 等值；混合配置会被这里挡下，而不是在 token 校验时才炸）。
        if !constant_time_eq(doc.issuer.as_bytes(), self.config.issuer.as_bytes()) {
            tracing::warn!("oidc: discovery issuer does not match the configured issuer");
            return Err(FlowRejection::UpstreamUnavailable(
                "discovery issuer does not match the configuration",
            ));
        }
        {
            let mut inner = self.lock_inner();
            inner.discovery = Some(CachedDiscovery {
                fetched_at: now,
                doc: doc.clone(),
            });
        }
        Ok(doc)
    }

    // ── token 交换（code + verifier；secret 按 Basic/Post 形态提交） ──

    async fn exchange_code(
        &self,
        token_endpoint: &str,
        code: &str,
        verifier: &str,
    ) -> Result<TokenResponse, FlowRejection> {
        let mut client_secret: Option<&str> = None;
        if let Some(secret) = &self.config.client_secret {
            if secret.auth_style == ClientAuthStyle::Basic {
                // Basic：凭据进 Authorization 头，不进请求体。
            } else {
                client_secret = Some(secret.value.as_str());
            }
        }
        let form = TokenRequestForm {
            grant_type: "authorization_code",
            code,
            redirect_uri: &self.redirect_uri(),
            client_id: &self.config.client_id,
            code_verifier: verifier,
            client_secret,
        };
        let mut request = self.http.post(token_endpoint);
        if let Some(secret) = &self.config.client_secret {
            if secret.auth_style == ClientAuthStyle::Basic {
                request = request.basic_auth(&self.config.client_id, Some(&secret.value));
            }
        }
        let response = request
            .form(&form)
            .send()
            .await
            .map_err(|_| FlowRejection::UpstreamUnavailable("token endpoint is unreachable"))?;
        if !response.status().is_success() {
            // IdP 的错误体可能回显参数，绝不进日志——只记状态码。
            tracing::warn!(status = %response.status(), "oidc: token endpoint rejected the exchange");
            return Err(FlowRejection::UpstreamRejected(
                "the token endpoint rejected the authorization code exchange",
            ));
        }
        response
            .json()
            .await
            .map_err(|_| FlowRejection::UpstreamRejected("the token response is not parseable"))
    }

    // ── id_token 校验（JWKS 按 kid 缓存 + 未知 kid 一次刷新） ──

    async fn validate_id_token(
        &self,
        token: &str,
        jwks_uri: &str,
        expected_nonce: &str,
    ) -> Result<IdTokenClaims, FlowRejection> {
        let token_header = jsonwebtoken::decode_header(token)
            .map_err(|_| FlowRejection::UpstreamRejected("the id_token header is not parseable"))?;
        // kid 是缓存索引（§4.2 签名行）；无 kid 的 token 无法走缓存轮换，
        // 一律拒绝。
        let Some(kid) = token_header.kid.as_deref() else {
            return Err(FlowRejection::UpstreamRejected("the id_token has no kid"));
        };
        let decoding_key = self.decoding_key(jwks_uri, kid).await?;

        // crate 侧校验：签名算法（RS256）、iss 精确等值、aud 包含本服务
        // client id、exp 带偏移容差（§4.2 的 iss/aud/exp 行）。
        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
        validation.set_issuer(&[self.config.issuer.clone()]);
        validation.aud = Some(HashSet::from([self.config.client_id.clone()]));
        validation.leeway = self.config.clock_skew_leeway.as_secs();
        validation.validate_exp = true;
        validation.validate_nbf = false;
        let decoded = jsonwebtoken::decode::<IdTokenClaims>(token, &decoding_key, &validation)
            .map_err(|_| FlowRejection::UpstreamRejected("id_token validation failed"))?;

        // 手工收尾（§4.2 的 nonce/azp/sub/iat 行）。
        let now = self.clock.now_unix_secs();
        claims::verify_nonce_azp_subject_and_iat(
            &decoded.claims,
            expected_nonce,
            &self.config.client_id,
            now,
            self.config.clock_skew_leeway.as_secs(),
        )
        .map_err(|rejection| {
            tracing::warn!(?rejection, "oidc: id_token claims checks failed");
            FlowRejection::UpstreamRejected("id_token validation failed")
        })?;
        Ok(decoded.claims)
    }

    /// `kid` → 解码钥。命中即用；未命中**触发一次**刷新（§4.2 签名行的
    /// 轮换路径——IdP 已换钥而缓存新鲜的场景靠它完成轮换），刷新后再看
    /// 一眼，仍无则拒绝。每次校验至多一次网络刷新。
    async fn decoding_key(
        &self,
        jwks_uri: &str,
        kid: &str,
    ) -> Result<jsonwebtoken::DecodingKey, FlowRejection> {
        let now = self.clock.now_unix_secs();
        let cached = {
            let inner = self.lock_inner();
            inner.jwks.as_ref().and_then(|cache| {
                // TTL 过期的缓存命中等价于 miss（jwks_ttl 驱动轮换）。
                let fresh = now.saturating_sub(cache.fetched_at) <= self.config.jwks_ttl.as_secs();
                if fresh { cache.keys.get(kid) } else { None }.cloned()
            })
        };
        if let Some(key) = cached {
            return Ok(key);
        }
        let fresh = self.fetch_jwks(jwks_uri, now).await?;
        let key = fresh.keys.get(kid).cloned();
        {
            let mut inner = self.lock_inner();
            inner.jwks = Some(fresh);
        }
        key.ok_or(FlowRejection::UpstreamRejected(
            "the id_token kid is not published",
        ))
    }

    async fn fetch_jwks(&self, jwks_uri: &str, now: u64) -> Result<JwksCache, FlowRejection> {
        let response = self
            .http
            .get(jwks_uri)
            .send()
            .await
            .map_err(|_| FlowRejection::UpstreamUnavailable("jwks is unreachable"))?;
        if !response.status().is_success() {
            tracing::warn!(status = %response.status(), "oidc: jwks request failed");
            return Err(FlowRejection::UpstreamUnavailable("jwks is unreachable"));
        }
        let jwks: jsonwebtoken::jwk::JwkSet = response
            .json()
            .await
            .map_err(|_| FlowRejection::UpstreamUnavailable("jwks is not parseable"))?;
        let mut keys = HashMap::new();
        for jwk in &jwks.keys {
            let Some(kid) = jwk.common.key_id.clone() else {
                continue; // 无 kid 的钥无法索引缓存，跳过
            };
            if let Ok(key) = jsonwebtoken::DecodingKey::from_jwk(jwk) {
                keys.insert(kid, key);
            } else {
                tracing::warn!("oidc: a jwks key could not be converted to a decoding key");
            }
        }
        Ok(JwksCache {
            fetched_at: now,
            keys,
        })
    }
}

fn prune_pending(pending: &mut HashMap<String, PendingAuthorization>, ttl_secs: u64, now: u64) {
    pending.retain(|_, txn| now.saturating_sub(txn.created_at) <= ttl_secs);
}

// ---------------------------------------------------------------------------
// handler 侧类型
// ---------------------------------------------------------------------------

/// 发起登录的产物：302 目标 + state cookie。
pub struct BeginLogin {
    pub authorization_url: String,
    pub state_cookie: HeaderValue,
}

/// 回调成功的产物：会话 cookie（§4.4）+ 跳转目标。
pub struct LoginOutcome {
    pub session_cookie: HeaderValue,
    pub redirect_to: String,
}

/// 回调的 query 投影（`Query::try_from_uri` 反序列化；缺失字段 = `None`）。
#[derive(Debug, Default, Deserialize)]
pub struct CallbackQuery {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub error_description: Option<String>,
}

#[derive(Serialize)]
struct TokenRequestForm<'a> {
    grant_type: &'a str,
    code: &'a str,
    redirect_uri: &'a str,
    client_id: &'a str,
    code_verifier: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_secret: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    #[serde(default)]
    id_token: Option<String>,
    /// RP 首版只用 id_token；access_token 解析但不用（保留字段以校验
    /// token 端点响应形状完整）。
    #[serde(default)]
    #[allow(dead_code)]
    access_token: Option<String>,
}

/// 登录流的拒绝面。变体携带**静态描述**（脱敏纪律：授权码/verifier/
/// secret/state/上游响应体不进任何变体），HTTP 映射见模块文档。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowRejection {
    /// state 缺失/不匹配/过期/重放（§4.2 state 行 → 400）。
    StateInvalid,
    /// 回调缺 `code`（400）。
    CallbackMissingCode,
    /// IdP 拒绝（token 交换失败 / id_token 校验失败 → 401）。
    UpstreamRejected(&'static str),
    /// subject 无启用 membership（§4.2 membership 行 → 403）。
    MembershipUnresolved,
    /// 上游（发现/JWKS/token）不可达或完整性不匹配（503）。
    UpstreamUnavailable(&'static str),
    /// 管理库失败（503——与 L2 的 Storage 分支同款处置）。
    Storage,
    /// 待定事务表满（429 + Retry-After）。
    PendingTableFull,
}

impl FlowRejection {
    fn api_code(self) -> (ApiErrorCode, &'static str) {
        match self {
            Self::StateInvalid => (
                ApiErrorCode::InvalidArgument,
                "The login transaction is invalid, expired, or was already used.",
            ),
            Self::CallbackMissingCode => (
                ApiErrorCode::InvalidArgument,
                "The authorization response is missing the code parameter.",
            ),
            Self::UpstreamRejected(_) => (
                ApiErrorCode::Unauthenticated,
                "Sign-in could not be completed.",
            ),
            Self::MembershipUnresolved => (
                ApiErrorCode::PermissionDenied,
                "This account is not enabled for this organization.",
            ),
            Self::UpstreamUnavailable(_) => (
                ApiErrorCode::ServiceUnavailable,
                "The sign-in provider is temporarily unavailable.",
            ),
            Self::Storage => (
                ApiErrorCode::ServiceUnavailable,
                "The service is temporarily unavailable.",
            ),
            Self::PendingTableFull => (
                ApiErrorCode::QuotaExceeded,
                "Too many pending sign-ins; retry after the stated interval.",
            ),
        }
    }

    fn into_response(self, request_id: Option<RequestId>) -> Response {
        let (code, message) = self.api_code();
        let mut response =
            ApiErrorResponse::new(ApiError::new(code, message).with_request_id(request_id))
                .into_response();
        if self == Self::PendingTableFull {
            if let Ok(value) = HeaderValue::from_str(&PENDING_FULL_RETRY_AFTER_SECS.to_string()) {
                response.headers_mut().insert(header::RETRY_AFTER, value);
            }
        }
        response
    }
}

fn request_id_of(req: &Request) -> Option<RequestId> {
    req.extensions().get::<RequestId>().cloned()
}

/// `GET /api/v1/auth/login`（契约 §5）：发起登录 → 302 IdP + state cookie。
pub async fn login(State(rp): State<OidcState>, req: Request) -> Response {
    let request_id = request_id_of(&req);
    match rp.begin_login().await {
        Ok(begin) => {
            let Ok(location) = HeaderValue::from_str(&begin.authorization_url) else {
                return FlowRejection::UpstreamUnavailable(
                    "authorization endpoint is not a valid URL",
                )
                .into_response(request_id);
            };
            Response::builder()
                .status(StatusCode::FOUND)
                .header(header::LOCATION, location)
                .header(header::SET_COOKIE, begin.state_cookie)
                .body(axum::body::Body::empty())
                .unwrap_or_else(|_| internal_error(request_id))
        }
        Err(rejection) => rejection.into_response(request_id),
    }
}

/// `GET /api/v1/auth/callback`（契约 §5）：完成登录 → 302 回应 +
/// 会话 cookie + 清除 state cookie。
pub async fn callback(State(rp): State<OidcState>, req: Request) -> Response {
    let request_id = request_id_of(&req);
    let query = Query::<CallbackQuery>::try_from_uri(req.uri())
        .map(|parsed| parsed.0)
        .unwrap_or_default();
    let cookie_state = session_cookie(req.headers(), STATE_COOKIE);
    let user_agent = req
        .headers()
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok());

    match rp
        .complete_login(&query, cookie_state.as_deref(), user_agent)
        .await
    {
        Ok(outcome) => {
            let Ok(location) = HeaderValue::from_str(&outcome.redirect_to) else {
                return FlowRejection::UpstreamUnavailable("redirect target is not a valid URL")
                    .into_response(request_id);
            };
            Response::builder()
                .status(StatusCode::FOUND)
                .header(header::LOCATION, location)
                .header(header::SET_COOKIE, outcome.session_cookie)
                .header(header::SET_COOKIE, clear_cookie(STATE_COOKIE))
                .body(axum::body::Body::empty())
                .unwrap_or_else(|_| internal_error(request_id))
        }
        Err(rejection) => {
            // 失败同样清掉 state cookie：旧事务已死，不留作重放原料。
            let mut response = rejection.into_response(request_id);
            response
                .headers_mut()
                .append(header::SET_COOKIE, clear_cookie(STATE_COOKIE));
            response
        }
    }
}

fn internal_error(request_id: Option<RequestId>) -> Response {
    ApiErrorResponse::new(
        ApiError::new(
            ApiErrorCode::ServiceUnavailable,
            "The service is temporarily unavailable.",
        )
        .with_request_id(request_id),
    )
    .into_response()
}

// ---------------------------------------------------------------------------
// 原语（cookie 形态 / PKCE / 随机 / 百分号编码）
// ---------------------------------------------------------------------------

/// §4.4 cookie 形态（逐字）：`__Host-` 名 + `HttpOnly; SameSite=Lax;
/// Secure; Path=/; Max-Age=N`——无 `Domain`。会话与 state 两个 cookie 共用
/// 本形态（state 的 Max-Age = state TTL，会话的 = 会话绝对寿命）。
fn cookie_value(name: &str, value: &str, max_age_secs: u64) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{name}={value}; HttpOnly; SameSite=Lax; Secure; Path=/; Max-Age={max_age_secs}"
    ))
    .expect("cookie attributes are always valid header bytes")
}

/// 单次消费后的删除形态（§4.4：state cookie「单次消费后立即删除」）。
pub fn clear_cookie(name: &str) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{name}=; HttpOnly; SameSite=Lax; Secure; Path=/; Max-Age=0"
    ))
    .expect("cookie attributes are always valid header bytes")
}

/// PKCE 对（§4.2 PKCE 行逐字）：`challenge = BASE64URL(SHA256(verifier))`。
/// verifier 由 64 字节 CSPRNG 经 base64url 生成（86 字符，RFC 7636 的
/// 43–128 区间内，字符集是其 unreserved 白名单的子集）。
pub(crate) fn pkce_pair(random_bytes: [u8; VERIFIER_BYTES]) -> (String, String) {
    let verifier = URL_SAFE_NO_PAD.encode(random_bytes);
    let digest = Sha256::digest(verifier.as_bytes());
    (verifier, URL_SAFE_NO_PAD.encode(digest))
}

/// CSPRNG 字节 → base64url（state / nonce / session id 的铸造）。
pub(crate) fn random_b64url(n_bytes: usize) -> String {
    let mut bytes = vec![0u8; n_bytes];
    RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// query 值的百分号编码（RFC 3986 unreserved 白名单；base64url 的
/// `-` `_` 与 `.` 都在白名单内，state/nonce/challenge 不被转义）。
pub(crate) fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_matches_the_rfc7636_appendix_b_vector() {
        // RFC 7636 Appendix B 的权威向量：S256 challenge 必须逐字一致。
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        // 权威向量直接钉 S256 公式（challenge = BASE64URL(SHA256(verifier))；
        // RFC 向量的 verifier 解码只有 32 字节，而本实现生成 64 字节熵的
        // verifier——公式同一条路径，不经过 pkce_pair 的随机源）。
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        // pkce_pair 产出的 challenge 与同一公式逐字一致（64 字节随机源）。
        let random = [7u8; VERIFIER_BYTES];
        let (produced_verifier, challenge) = pkce_pair(random);
        assert_eq!(
            challenge,
            URL_SAFE_NO_PAD.encode(Sha256::digest(produced_verifier.as_bytes()))
        );
    }

    #[test]
    fn authorization_urls_carry_s256_only_and_encode_every_parameter() {
        let doc = DiscoveryDocument {
            issuer: "https://idp.example".into(),
            authorization_endpoint: "https://idp.example/authorize".into(),
            token_endpoint: "https://idp.example/token".into(),
            jwks_uri: "https://idp.example/jwks.json".into(),
        };
        let config = test_config("https://idp.example");
        let rp = OidcRelyingParty::new(
            config,
            test_store(),
            test_policy(),
            test_sessions(),
            Arc::new(SystemClock),
        )
        .expect("rp");
        let url = rp.authorization_url(&doc, "st/ate=x", "no/nce", "chal=enge");
        assert!(url.starts_with("https://idp.example/authorize?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("code_challenge_method=S256"), "只发 S256");
        assert!(!url.contains("plain"), "禁 plain（§4.2）");
        // 非 unreserved 字节必须被编码（state/nonce/challenge 里的 = / 等）。
        assert!(url.contains("state=st%2Fate%3Dx"));
        assert!(url.contains("nonce=no%2Fnce"));
        assert!(url.contains("code_challenge=chal%3Denge"));
        assert!(url.contains("redirect_uri=https%3A%2F%2Frp.test%2Fapi%2Fv1%2Fauth%2Fcallback"));
    }

    #[test]
    fn pending_transactions_are_consumed_exactly_once() {
        let rp = test_rp(600, 8);
        let mut inner = rp.lock_inner();
        inner.pending.insert(
            "st-1".into(),
            PendingAuthorization {
                nonce: "n".into(),
                pkce_verifier: "v".into(),
                created_at: 1_000,
            },
        );
        // 取出即焚：第二次 take 拿不到（§4.2「单次消费」）。
        assert!(inner.pending.remove("st-1").is_some());
        assert!(inner.pending.remove("st-1").is_none());
    }

    #[test]
    fn expired_pending_transactions_are_pruned() {
        prune_pending(&mut HashMap::new(), 600, 1_000);
        let mut pending = HashMap::new();
        pending.insert(
            "fresh".into(),
            PendingAuthorization {
                nonce: String::new(),
                pkce_verifier: String::new(),
                created_at: 500,
            },
        );
        pending.insert(
            "stale".into(),
            PendingAuthorization {
                nonce: String::new(),
                pkce_verifier: String::new(),
                created_at: 399,
            },
        );
        prune_pending(&mut pending, 600, 1_000);
        // TTL 边界（含）内存活：1000-500=600 ≤ 600；399+600=999 < 1000 出界。
        assert!(pending.contains_key("fresh"));
        assert!(!pending.contains_key("stale"));
    }

    #[test]
    fn pending_capacity_semantics_prune_first_then_count() {
        // begin_login 的容量裁决（2 槽）：先清过期再计数——过期事务让位，
        // 新事务可入；仍满则拒绝（429，见 FlowRejection::PendingTableFull）。
        let rp = test_rp(600, 2);
        let now = rp.clock.now_unix_secs();
        {
            let mut inner = rp.lock_inner();
            inner.pending.insert(
                "stale".into(),
                PendingAuthorization {
                    nonce: String::new(),
                    pkce_verifier: String::new(),
                    created_at: now.saturating_sub(601),
                },
            );
            inner.pending.insert(
                "fresh-1".into(),
                PendingAuthorization {
                    nonce: String::new(),
                    pkce_verifier: String::new(),
                    created_at: now,
                },
            );
            inner.pending.insert(
                "fresh-2".into(),
                PendingAuthorization {
                    nonce: String::new(),
                    pkce_verifier: String::new(),
                    created_at: now,
                },
            );
            prune_pending(&mut inner.pending, rp.config.state_ttl.as_secs(), now);
            assert_eq!(inner.pending.len(), 2, "过期让位后恰好满容量");
        }
    }

    #[test]
    fn cookie_strings_match_the_host_prefix_attributes_exactly() {
        // §4.4 逐字形态：HttpOnly + SameSite=Lax + Secure + Path=/ + Max-Age，无 Domain。
        let session = cookie_value(SESSION_COOKIE, "sid-1", 3_600);
        assert_eq!(
            session.to_str().unwrap(),
            "__Host-dz_session=sid-1; HttpOnly; SameSite=Lax; Secure; Path=/; Max-Age=3600"
        );
        let state = cookie_value(STATE_COOKIE, "st-1", 600);
        assert_eq!(
            state.to_str().unwrap(),
            "__Host-dz_oidc_state=st-1; HttpOnly; SameSite=Lax; Secure; Path=/; Max-Age=600"
        );
        let cleared = clear_cookie(STATE_COOKIE);
        assert_eq!(
            cleared.to_str().unwrap(),
            "__Host-dz_oidc_state=; HttpOnly; SameSite=Lax; Secure; Path=/; Max-Age=0"
        );
    }

    #[test]
    fn session_ids_and_nonces_are_high_entropy_and_fresh() {
        let first = random_b64url(SESSION_ID_BYTES);
        let second = random_b64url(SESSION_ID_BYTES);
        assert_eq!(first.len(), 43, "32 字节 → 43 个 base64url 字符（256-bit）");
        assert_ne!(first, second, "每次铸造必须新鲜");
        assert!(
            first
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "铸造值必须是 base64url 字符集（cookie 安全）"
        );
        let nonce = random_b64url(NONCE_BYTES);
        assert_eq!(nonce.len(), 43);
    }

    #[test]
    fn config_validation_rejects_broken_origins_and_windows() {
        let mut config = test_config("https://idp.example");
        config.issuer = "idp.example".into();
        assert!(
            OidcRelyingParty::new(
                config.clone(),
                test_store(),
                test_policy(),
                test_sessions(),
                Arc::new(SystemClock)
            )
            .is_err(),
            "issuer 必须是绝对 URL"
        );
        let mut config = test_config("https://idp.example");
        config.callback_path = "api/v1/auth/callback".into();
        assert!(
            OidcRelyingParty::new(
                config.clone(),
                test_store(),
                test_policy(),
                test_sessions(),
                Arc::new(SystemClock)
            )
            .is_err(),
            "callback_path 必须以 / 开头"
        );
        let mut config = test_config("https://idp.example");
        config.session_idle_seconds = 9_000;
        config.session_absolute_seconds = 3_600;
        assert!(
            OidcRelyingParty::new(
                config.clone(),
                test_store(),
                test_policy(),
                test_sessions(),
                Arc::new(SystemClock)
            )
            .is_err(),
            "空闲窗不得越过绝对窗（ck_login_window 前提）"
        );
        let mut config = test_config("https://idp.example");
        config.scope = "profile email".into();
        assert!(
            OidcRelyingParty::new(
                config,
                test_store(),
                test_policy(),
                test_sessions(),
                Arc::new(SystemClock)
            )
            .is_err(),
            "scope 必须含 openid"
        );
        let good = test_config("https://idp.example");
        assert!(OidcRelyingParty::new(
            good,
            test_store(),
            test_policy(),
            test_sessions(),
            Arc::new(SystemClock)
        )
        .is_ok());
    }

    #[test]
    fn percent_encode_keeps_unreserved_bytes_only() {
        assert_eq!(percent_encode("abcXYZ019-._~"), "abcXYZ019-._~");
        assert_eq!(percent_encode("a b"), "a%20b");
        assert_eq!(percent_encode("q=1&r=2"), "q%3D1%26r%3D2");
        assert_eq!(percent_encode("é"), "%C3%A9");
    }

    #[test]
    fn the_client_secret_debug_output_is_redacted() {
        let rendered = format!(
            "{:?}",
            ClientSecret {
                value: "hunter2-secret".into(),
                auth_style: ClientAuthStyle::Basic
            }
        );
        assert_eq!(rendered, "ClientSecret(<redacted>)");
    }

    // —— 测试装配（无网络路径；假 IdP 的全流程钉子在 `super::oidc_flow_tests`） ——

    fn test_config(issuer: &str) -> OidcConfig {
        OidcConfig {
            issuer: issuer.into(),
            client_id: "datazen-web".into(),
            client_secret: Some(ClientSecret {
                value: "hunter2-secret".into(),
                auth_style: ClientAuthStyle::Post,
            }),
            organization_id: "org-a".into(),
            public_origin: "https://rp.test".into(),
            callback_path: "/api/v1/auth/callback".into(),
            scope: "openid profile".into(),
            post_login_redirect: "/".into(),
            http_timeout: Duration::from_secs(5),
            discovery_ttl: Duration::from_secs(300),
            jwks_ttl: Duration::from_secs(300),
            clock_skew_leeway: Duration::from_secs(60),
            state_ttl: Duration::from_secs(600),
            pending_capacity: 8,
            session_idle_seconds: 1_800,
            session_absolute_seconds: 3_600,
        }
    }

    fn test_store() -> Arc<AdminStore> {
        let store = AdminStore::open_in_memory().expect("store");
        store.migrate().expect("migrate");
        Arc::new(store)
    }

    fn test_policy() -> Arc<PolicyService> {
        Arc::new(
            PolicyService::new(
                test_store(),
                b"test-server-secret-32-bytes-minimum!!".to_vec(),
            )
            .expect("policy"),
        )
    }

    fn test_sessions() -> LoginSessions {
        LoginSessions::new(test_store())
    }

    fn test_rp(state_ttl_secs: u64, capacity: usize) -> OidcRelyingParty {
        let mut config = test_config("https://idp.example");
        config.state_ttl = Duration::from_secs(state_ttl_secs);
        config.pending_capacity = capacity;
        OidcRelyingParty::new(
            config,
            test_store(),
            test_policy(),
            test_sessions(),
            Arc::new(SystemClock),
        )
        .expect("rp")
    }
}
