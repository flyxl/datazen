//! `oidc` 全流程钉子（假 IdP = 进程内 axum，127.0.0.1:0；网络面只到 loopback，
//! 无外呼）。覆盖 §4.1/§4.2 的可代码化裁决：发起/回调双腿、PKCE S256 在 IdP
//! 侧的真实校验、state 三重校验（cookie/query 常量时间相等 → 一次性取出即焚 →
//! TTL）、nonce/aud/exp 校验、JWKS `kid` 轮换（未知 kid 触发一次刷新）、
//! membership 映射拒绝（不隐式开户）、待定事务容量、发现文档 issuer 精确匹配、
//! token 端点失败映射。
//!
//! 会话 cookie 的 L2 级验收（middleware/auth.rs 逐字一致）在主线接线时由
//! 集成腿覆盖；本文件在 RP API 层断言会话行与 cookie 形状。

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Form, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use datazen::shared::id::Timestamp;

use super::oidc::{
    CallbackQuery, ClientAuthStyle, ClientSecret, FlowRejection, OidcClock, OidcConfig,
    OidcRelyingParty, STATE_COOKIE,
};
use crate::policy::{PolicyService, Role};
use crate::storage::{AdminStore, LoginSessions};

/// 测试专用 RSA-2048 私钥（PKCS#8；仅存在于本文件，不对应任何真实部署）。
const TEST_RSA_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQDXBTWWzVLrg/rH\nBkEbYh7G0476TbC8uiJYswDKrM1b5yVpcbZTMRjUz5pb/eh+7E2LYbW9yNPlnf5A\nZpFmQ6v1GBkvvsWaeTvaqBWCio6qzZujKgx4TK/w+/eNgip5Q8/jMnmJ5ZARjgUy\nU17jCPex1n/JqS7WF7DXe0xaeBVfrAl8+983FJcd6efrTcqitk7OKe/45J1zqKaV\nh8xtJTWadKJiPsjIeEWEECfAKTTosW4Bv2Xbu3kvzCCb14hLd/SOtZ76Nlo8SqA7\ng9VddBfPt2en+2Jnqwb0M19Q1RFBjR8TIpKprRx8wAedB+S1E2K5f65am75SEOOl\ni/PmEV73AgMBAAECggEALq+BXrBPCDbcPNlVSpJH12i42w0CSj/p1DNtgRP69LDf\n1SCs8ywuVYqL29xp3iEFhzVcZgwUQf9HZIlAmaEo1Wr5kP3NXshi4AttvC3EYz6I\nQ+mN0pLoAp1atx1jhnfcS9q6O+Ado0ogQ7iJWicozLKjhVAYDMEREwBA6EW5ck98\nbTBfzA2z8YZGmy/nfJnS1u5tKqwPN3nHaKx0unb6J1aGf1rJ90AWLEArMRWCYt5Z\nTrKqmf7qz5HTv8H97b8qX7Rcl9r4wGd0fDk+0zo34fIp4nTMdQbEJT0KqQTqxQiN\nv6qSfZccScJqXQGxeV7+4mNMKFJz7JdAWSLo3aczSQKBgQD/8nhZ1KWLOXif7lAW\nUxy07e1CxpybmM0SCm99J5F5qhgvLA4fUAbnfO69C5HCMNyC6uZi3QZLsivCHGuj\nSy22GIhqSeAXIC8H0z7tn8RPuVug1xxH3XomsKwT4dfgcdIi8bZ4dwYeop5LLTnM\n2DxJZ2CUHwlfXReUjYwLq9cuqQKBgQDXEJNjoEbx7XOy2GEsHI32TI4HdqRo81WT\n8QY0JKHa0syNu3XyoRwuqTiwXsfoJ7rvuuNjEmX3vAZoPPxyfEWZpnpF5KKRc/P/\n5WiycZgDhqWvlCGbsui4ij83ZAWx80Pxl84PM9saqF2W2Kwgf0Cuqhopxkv6PmHT\n29xbd2LEnwKBgQCSl62WxdSLCT+tWjFEmBW5Cwz5UggglkPeMVvpst//bwyq9/Ce\nvpc1vrh+TkINly61TRTnSvBwKnwlkWGVBVoJEcQQHH/cvvIw6S0BrKAjiWaRmKL6\nBkAQKOBmAtRCGO7iVM7N8F3w7fL728tfJ/pfkHYgYCZeGFOJIgk0Jhf5AQKBgHaQ\nOtE3sST6xUgZpiiNbW4Vck+HBsidLdarj8H7WAetIvwYFsQ2MbU/sXL6Lbc8///d\nxjH+GJzk2Ztp07rB7BTzBIG3tC///spajMejfCyldH5Bae3OSiJChOtihwk9lX2B\nNSaIbghaPY5F0U7xGJWyTU4udgB1l7UWhnsebCL9AoGAZCOd2X0j/ePz0XHnm5Ao\ny+0HCCWIvBnVSDRQoZG2bz2/8p1lcoMnwWU2RF5imzvCJBzVng2Zbw29ZU6QHJ17\nhB514dLdt/nU3lUB6ej6/1V/b1OTKgpVjVGFSdoR8nlU3H5It5xSX3x7R8LRf2yJ\ntDDK1iZ+0apiz3TrDTDRqMI=\n-----END PRIVATE KEY-----\n";
/// 上式公钥的 RSA 模数（base64url，无填充）——JWKS 的 `n`。
const TEST_RSA_N: &str = "1wU1ls1S64P6xwZBG2IextOO-k2wvLoiWLMAyqzNW-claXG2UzEY1M-aW_3ofuxNi2G1vcjT5Z3-QGaRZkOr9RgZL77Fmnk72qgVgoqOqs2boyoMeEyv8Pv3jYIqeUPP4zJ5ieWQEY4FMlNe4wj3sdZ_yaku1hew13tMWngVX6wJfPvfNxSXHenn603KorZOzinv-OSdc6imlYfMbSU1mnSiYj7IyHhFhBAnwCk06LFuAb9l27t5L8wgm9eIS3f0jrWe-jZaPEqgO4PVXXQXz7dnp_tiZ6sG9DNfUNURQY0fEyKSqa0cfMAHnQfktRNiuX-uWpu-UhDjpYvz5hFe9w";
const TEST_RSA_E: &str = "AQAB";

// ---------------------------------------------------------------- fake IdP --

#[derive(Clone)]
struct IdpState {
    inner: Arc<IdpInner>,
}

struct IdpInner {
    /// 绑定端口后才知道自己的 base URL，故用锁（发现文档与 iss 都读它）。
    issuer: Mutex<String>,
    client_id: String,
    client_secret: String,
    /// 测试逐例改写：id_token 的 claims 模板（nonce/aud/exp/sub 由用例注入）。
    claims: Mutex<Value>,
    /// token 响应用哪个 kid 签名（轮换用例改写）。
    kid: Mutex<String>,
    /// JWKS 发布面（kid → JWK JSON；轮换用例增补）。
    jwks: Mutex<HashMap<String, Value>>,
    /// /jwks 被拉取的次数（「未知 kid 触发**一次**刷新」钉子的观测点）。
    jwks_fetches: AtomicU64,
    /// 已消费的授权码（单次使用）。
    used_codes: Mutex<HashSet<String>>,
    /// IdP 侧期望的 PKCE challenge（S256(verifier) 的 base64url）。
    challenge: Mutex<Option<String>>,
    /// verifier 校验通过的观测点。
    pkce_ok: AtomicBool,
    /// true → /token 一律 500（上游故障映射用例）。
    fail_token: AtomicBool,
    /// 最近一次签发的 id_token（诊断与断言观测点）。
    last_token: Mutex<Option<String>>,
}

#[derive(Deserialize)]
struct TokenForm {
    grant_type: String,
    code: String,
    #[serde(default)]
    code_verifier: Option<String>,
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    client_secret: Option<String>,
}

async fn discovery(State(st): State<IdpState>) -> Json<Value> {
    let issuer = st.inner.issuer.lock().unwrap().clone();
    Json(json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/authorize"),
        "token_endpoint": format!("{issuer}/token"),
        "jwks_uri": format!("{issuer}/jwks"),
    }))
}

async fn jwks(State(st): State<IdpState>) -> Json<Value> {
    st.inner.jwks_fetches.fetch_add(1, Ordering::SeqCst);
    let keys: Vec<Value> = st.inner.jwks.lock().unwrap().values().cloned().collect();
    Json(json!({ "keys": keys }))
}

async fn token(State(st): State<IdpState>, Form(form): Form<TokenForm>) -> impl IntoResponse {
    if st.inner.fail_token.load(Ordering::SeqCst) {
        return (StatusCode::INTERNAL_SERVER_ERROR, "boom").into_response();
    }
    if form.grant_type != "authorization_code" {
        return (StatusCode::BAD_REQUEST, "bad grant_type").into_response();
    }
    // 授权码单次使用（ IdP 侧纪律）。
    if !st
        .inner
        .used_codes
        .lock()
        .unwrap()
        .insert(form.code.clone())
    {
        return (StatusCode::BAD_REQUEST, "code replayed").into_response();
    }
    // PKCE S256：BASE64URL(SHA256(verifier)) 与发起时登记的 challenge 相等。
    let expected = st.inner.challenge.lock().unwrap().clone();
    let verifier = form.code_verifier.clone().unwrap_or_default();
    let digest = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    if expected.as_deref() == Some(digest.as_str()) {
        st.inner.pkce_ok.store(true, Ordering::SeqCst);
    } else {
        return (StatusCode::BAD_REQUEST, "pkce mismatch").into_response();
    }
    if form.client_id.as_deref() != Some(st.inner.client_id.as_str())
        || form.client_secret.as_deref() != Some(st.inner.client_secret.as_str())
    {
        return (StatusCode::UNAUTHORIZED, "client auth failed").into_response();
    }
    let kid = st.inner.kid.lock().unwrap().clone();
    let claims = st.inner.claims.lock().unwrap().clone();
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some(kid);
    let key = jsonwebtoken::EncodingKey::from_rsa_pem(TEST_RSA_PEM.as_bytes())
        .expect("test rsa pem parses");
    let id_token = jsonwebtoken::encode(&header, &claims, &key).expect("test id_token signs");
    *st.inner.last_token.lock().unwrap() = Some(id_token.clone());
    (
        StatusCode::OK,
        Json(json!({ "id_token": id_token, "access_token": "at-test" })),
    )
        .into_response()
}

fn idp_router(st: IdpState) -> Router {
    Router::new()
        .route("/.well-known/openid-configuration", get(discovery))
        .route("/jwks", get(jwks))
        .route("/token", post(token))
        .with_state(st)
}

async fn spawn_idp(st: IdpState) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, idp_router(st)).await.expect("serve") });
    format!("http://{addr}")
}

// ------------------------------------------------------------- 测试装配 ----

struct FakeClock(Arc<AtomicU64>);

impl OidcClock for FakeClock {
    fn now_unix_secs(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

fn base_jwk(kid: &str) -> Value {
    json!({
        "kty": "RSA",
        "kid": kid,
        "alg": "RS256",
        "use": "sig",
        "n": TEST_RSA_N,
        "e": TEST_RSA_E,
    })
}

struct Rig {
    rp: Arc<OidcRelyingParty>,
    sessions: LoginSessions,
    idp_base: String,
    state: IdpState,
    clock: Arc<AtomicU64>,
}

/// 一套完整的 RP + 假 IdP。`sub` 有启用 membership；`config_issuer` 允许
/// 与假 IdP 自报 issuer 错开（发现不匹配用例）。
async fn rig(config_issuer: Option<&str>, capacity: usize, with_membership: bool) -> Rig {
    let state_for_spawn = IdpState {
        inner: Arc::new(IdpInner {
            issuer: Mutex::new(String::new()),
            client_id: "datazen-web".into(),
            client_secret: "hunter2-secret".into(),
            claims: Mutex::new(json!({
                "iss": "https://idp.test",
                "sub": "user-1",
                "aud": "datazen-web",
                "azp": "datazen-web",
                "exp": 0,
                "iat": 0,
                "nonce": "",
            })),
            kid: Mutex::new("kid-a".into()),
            jwks: Mutex::new(HashMap::from([("kid-a".into(), base_jwk("kid-a"))])),
            jwks_fetches: AtomicU64::new(0),
            used_codes: Mutex::new(HashSet::new()),
            challenge: Mutex::new(None),
            pkce_ok: AtomicBool::new(false),
            fail_token: AtomicBool::new(false),
            last_token: Mutex::new(None),
        }),
    };
    let idp_base = spawn_idp(state_for_spawn.clone()).await;
    let inner = state_for_spawn.inner.clone();
    // OIDC 语义：issuer 就是发现 URL 的根——测试里即假 IdP 的实际地址。
    *inner.issuer.lock().unwrap() = idp_base.clone();
    inner.claims.lock().unwrap()["iss"] = json!(idp_base);
    let state = IdpState { inner };

    let clock = Arc::new(AtomicU64::new(1_700_000_000));
    let store = Arc::new(AdminStore::open_in_memory().expect("store"));
    store.migrate().expect("migrate");
    let policy = Arc::new(
        PolicyService::new(
            store.clone(),
            b"test-server-secret-32-bytes-minimum!!".to_vec(),
        )
        .expect("policy"),
    );
    if with_membership {
        policy
            .create_membership("org-a", "principal-1", Role::OrgMember, "user-1")
            .expect("membership");
    }
    let sessions = LoginSessions::new(store.clone());
    let config = OidcConfig {
        issuer: config_issuer
            .map(str::to_string)
            .unwrap_or_else(|| idp_base.clone()),
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
        pending_capacity: capacity,
        session_idle_seconds: 1_800,
        session_absolute_seconds: 3_600,
    };
    let rp = Arc::new(
        OidcRelyingParty::new(
            config,
            store.clone(),
            policy,
            sessions.clone(),
            Arc::new(FakeClock(clock.clone())),
        )
        .expect("rp"),
    );
    Rig {
        rp,
        sessions,
        idp_base,
        state,
        clock,
    }
}

/// 会话行校验的时刻也必须相对 RP 的假时钟（行内到期字段由它铸造）。
fn rfc3339_at(unix_secs: u64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(unix_secs as i64, 0)
        .expect("unix secs in range")
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// jsonwebtoken 的 exp 校验走真实墙钟（不感知测试假时钟）；id_token 的
/// iat/exp 必须相对真实时刻取值，假时钟只驱动 RP 侧的 state/缓存 TTL。
fn real_now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock")
        .as_secs()
}

/// 从授权 URL 取 query 参数（本流参数全是 base64url，无百分号编码歧义）。
fn query_param(url: &str, key: &str) -> String {
    url.split('?')
        .nth(1)
        .expect("url has query")
        .split('&')
        .find_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            (k == key).then(|| v.to_string())
        })
        .expect("param present")
}

/// `Set-Cookie` 头里取 `name=` 的值（首个分号前）。
fn cookie_value(header_value: &HeaderValue, name: &str) -> String {
    let rendered = header_value.to_str().expect("cookie is ascii");
    let pair = rendered.split(';').next().expect("cookie has a pair");
    pair.strip_prefix(&format!("{name}="))
        .expect("cookie name matches")
        .to_string()
}

/// 发起并把 IdP 的 claims 模板对齐本次事务（nonce/iat/exp）。
async fn arm(rp: &OidcRelyingParty, state: &IdpState, clock: &AtomicU64) -> (String, String) {
    let begun = rp.begin_login().await.expect("begin");
    let state_value = query_param(&begun.authorization_url, "state");
    let nonce = query_param(&begun.authorization_url, "nonce");
    {
        let mut claims = state.inner.claims.lock().unwrap();
        claims["nonce"] = json!(nonce);
        // iat 相对 RP 的注入时钟（§4.2 的手工「不得来自未来」校验感知它），
        // exp 相对真实墙钟（jsonwebtoken 的 exp 校验感知它）。
        claims["iat"] = json!(clock.load(Ordering::SeqCst));
        claims["exp"] = json!(real_now_secs() + 300);
    }
    let verifier_hint = query_param(&begun.authorization_url, "code_challenge");
    *state.inner.challenge.lock().unwrap() = Some(verifier_hint);
    let cookie = cookie_value(&begun.state_cookie, STATE_COOKIE);
    (state_value, cookie)
}

const ONE_TIME_CODE: &str = "one-time-code";

// ----------------------------------------------------------------- 钉子 ----

#[tokio::test]
async fn full_login_flow_establishes_a_cookie_session() {
    let rig = rig(None, 8, true).await;
    let (state_value, cookie) = arm(&rig.rp, &rig.state, &rig.clock).await;
    let outcome = rig
        .rp
        .complete_login(
            &CallbackQuery {
                code: Some(ONE_TIME_CODE.into()),
                state: Some(state_value),
                ..Default::default()
            },
            Some(&cookie),
            Some("DataZenTest/1.0"),
        )
        .await
        .expect("login completes");
    // PKCE 在 IdP 侧真实通过（S256(verifier) 与发起的 challenge 相等）。
    assert!(
        rig.state.inner.pkce_ok.load(Ordering::SeqCst),
        "idp verified pkce"
    );
    // 302 目标与 cookie 形状（§4.4：HttpOnly + SameSite=Lax + Secure）。
    assert_eq!(outcome.redirect_to, "/");
    let rendered = outcome
        .session_cookie
        .to_str()
        .expect("cookie ascii")
        .to_string();
    assert!(
        rendered.starts_with("__Host-dz_session="),
        "host-prefixed: {rendered}"
    );
    assert!(
        rendered.contains("HttpOnly")
            && rendered.contains("SameSite=Lax")
            && rendered.contains("Secure")
    );
    // 会话行可被 §4.3 语义校验（principal/角色来自 membership 映射）。
    let session_id = cookie_value(&outcome.session_cookie, "__Host-dz_session");
    let unix_now = rig.clock.load(Ordering::SeqCst);
    let row = rig
        .sessions
        .validate(
            &session_id,
            Timestamp::new(rfc3339_at(unix_now + 60)),
            1_800,
        )
        .expect("session row validates");
    assert_eq!(row.principal_id, "principal-1");
    assert_eq!(row.organization_id, "org-a");
}

#[tokio::test]
async fn state_is_single_use_once_consumed() {
    let rig = rig(None, 8, true).await;
    let (state_value, cookie) = arm(&rig.rp, &rig.state, &rig.clock).await;
    let query = CallbackQuery {
        code: Some(ONE_TIME_CODE.into()),
        state: Some(state_value.clone()),
        ..Default::default()
    };
    rig.rp
        .complete_login(&query, Some(&cookie), None)
        .await
        .expect("first login completes");
    let second = rig.rp.complete_login(&query, Some(&cookie), None).await;
    assert_eq!(
        second.err(),
        Some(FlowRejection::StateInvalid),
        "replayed state"
    );
}

#[tokio::test]
async fn state_cookie_and_query_disagreement_is_rejected() {
    let rig = rig(None, 8, true).await;
    let (state_value, _cookie) = arm(&rig.rp, &rig.state, &rig.clock).await;
    let outcome = rig
        .rp
        .complete_login(
            &CallbackQuery {
                code: Some(ONE_TIME_CODE.into()),
                state: Some(state_value),
                ..Default::default()
            },
            Some("a-different-state"),
            None,
        )
        .await;
    assert_eq!(outcome.err(), Some(FlowRejection::StateInvalid));
}

#[tokio::test]
async fn state_past_ttl_is_rejected() {
    let rig = rig(None, 8, true).await;
    let (state_value, cookie) = arm(&rig.rp, &rig.state, &rig.clock).await;
    // state_ttl = 600s：跨过 601s 后回调。
    rig.clock.fetch_add(601, Ordering::SeqCst);
    let outcome = rig
        .rp
        .complete_login(
            &CallbackQuery {
                code: Some(ONE_TIME_CODE.into()),
                state: Some(state_value),
                ..Default::default()
            },
            Some(&cookie),
            None,
        )
        .await;
    assert_eq!(outcome.err(), Some(FlowRejection::StateInvalid));
}

#[tokio::test]
async fn nonce_mismatch_is_rejected() {
    let rig = rig(None, 8, true).await;
    let (state_value, cookie) = arm(&rig.rp, &rig.state, &rig.clock).await;
    rig.state.inner.claims.lock().unwrap()["nonce"] = json!("a-nonce-from-elsewhere");
    let outcome = rig
        .rp
        .complete_login(
            &CallbackQuery {
                code: Some(ONE_TIME_CODE.into()),
                state: Some(state_value),
                ..Default::default()
            },
            Some(&cookie),
            None,
        )
        .await;
    assert!(
        matches!(outcome.err(), Some(FlowRejection::UpstreamRejected(_))),
        "nonce mismatch maps to upstream rejection"
    );
}

#[tokio::test]
async fn wrong_audience_is_rejected() {
    let rig = rig(None, 8, true).await;
    let (state_value, cookie) = arm(&rig.rp, &rig.state, &rig.clock).await;
    rig.state.inner.claims.lock().unwrap()["aud"] = json!("someone-else");
    rig.state.inner.claims.lock().unwrap()["azp"] = json!("someone-else");
    let outcome = rig
        .rp
        .complete_login(
            &CallbackQuery {
                code: Some(ONE_TIME_CODE.into()),
                state: Some(state_value),
                ..Default::default()
            },
            Some(&cookie),
            None,
        )
        .await;
    assert!(matches!(
        outcome.err(),
        Some(FlowRejection::UpstreamRejected(_))
    ));
}

#[tokio::test]
async fn expired_id_token_is_rejected() {
    let rig = rig(None, 8, true).await;
    let (state_value, cookie) = arm(&rig.rp, &rig.state, &rig.clock).await;
    // exp 远早于真实 now（leeway 60s 盖不住 3600s）。
    rig.state.inner.claims.lock().unwrap()["exp"] = json!(real_now_secs() as i64 - 3_600);
    let outcome = rig
        .rp
        .complete_login(
            &CallbackQuery {
                code: Some(ONE_TIME_CODE.into()),
                state: Some(state_value),
                ..Default::default()
            },
            Some(&cookie),
            None,
        )
        .await;
    assert!(matches!(
        outcome.err(),
        Some(FlowRejection::UpstreamRejected(_))
    ));
}

#[tokio::test]
async fn unknown_kid_triggers_one_refresh_and_rotation_succeeds() {
    let rig = rig(None, 8, true).await;
    // 第一跳：IdP 只发布 kid-a，签名用 kid-b → 刷新一次仍缺 → 拒绝。
    let (state_a, cookie_a) = arm(&rig.rp, &rig.state, &rig.clock).await;
    *rig.state.inner.kid.lock().unwrap() = "kid-b".into();
    let rejected = rig
        .rp
        .complete_login(
            &CallbackQuery {
                code: Some("code-attempt-a".into()),
                state: Some(state_a),
                ..Default::default()
            },
            Some(&cookie_a),
            None,
        )
        .await;
    assert!(matches!(
        rejected.err(),
        Some(FlowRejection::UpstreamRejected(_))
    ));
    let after_first = rig.state.inner.jwks_fetches.load(Ordering::SeqCst);
    assert_eq!(
        after_first, 1,
        "exactly one jwks refresh for the unknown kid"
    );
    // 第二跳（全新事务）：IdP 轮换发布 kid-b → 未知 kid 触发的刷新拿到它。
    let (state_b, cookie_b) = arm(&rig.rp, &rig.state, &rig.clock).await;
    rig.state
        .inner
        .jwks
        .lock()
        .unwrap()
        .insert("kid-b".into(), base_jwk("kid-b"));
    let rotated = rig
        .rp
        .complete_login(
            &CallbackQuery {
                code: Some("code-attempt-b".into()),
                state: Some(state_b),
                ..Default::default()
            },
            Some(&cookie_b),
            None,
        )
        .await;
    assert!(rotated.is_ok(), "rotation completes the login");
    assert_eq!(rig.state.inner.jwks_fetches.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn login_without_enabled_membership_is_refused() {
    let rig = rig(None, 8, false).await;
    let (state_value, cookie) = arm(&rig.rp, &rig.state, &rig.clock).await;
    let outcome = rig
        .rp
        .complete_login(
            &CallbackQuery {
                code: Some(ONE_TIME_CODE.into()),
                state: Some(state_value),
                ..Default::default()
            },
            Some(&cookie),
            None,
        )
        .await;
    assert_eq!(outcome.err(), Some(FlowRejection::MembershipUnresolved));
}

#[tokio::test]
async fn pending_capacity_is_enforced() {
    let rig = rig(None, 1, true).await;
    rig.rp.begin_login().await.expect("first begin fits");
    let second = rig.rp.begin_login().await;
    assert_eq!(second.err(), Some(FlowRejection::PendingTableFull));
}

#[tokio::test]
async fn discovery_issuer_mismatch_is_rejected() {
    // 配置的 issuer 与假 IdP 自报的 issuer（其实际地址）逐字不等。
    let rig = rig(Some("https://idp.example"), 8, true).await;
    let outcome = rig.rp.begin_login().await;
    assert!(matches!(
        outcome.err(),
        Some(FlowRejection::UpstreamUnavailable(_))
    ));
}

#[tokio::test]
async fn idp_side_pkce_enforcement_surfaces_as_upstream_rejection() {
    let rig = rig(None, 8, true).await;
    let (state_value, cookie) = arm(&rig.rp, &rig.state, &rig.clock).await;
    // IdP 侧期望的 challenge 被改坏 → token 端点 400 → RP 上游拒绝。
    *rig.state.inner.challenge.lock().unwrap() =
        Some(URL_SAFE_NO_PAD.encode(Sha256::digest(b"a-different-verifier")));
    let outcome = rig
        .rp
        .complete_login(
            &CallbackQuery {
                code: Some(ONE_TIME_CODE.into()),
                state: Some(state_value),
                ..Default::default()
            },
            Some(&cookie),
            None,
        )
        .await;
    assert!(matches!(
        outcome.err(),
        Some(FlowRejection::UpstreamRejected(_))
    ));
    assert!(!rig.state.inner.pkce_ok.load(Ordering::SeqCst));
}

#[tokio::test]
async fn token_endpoint_failure_maps_to_upstream_rejection() {
    let rig = rig(None, 8, true).await;
    let (state_value, cookie) = arm(&rig.rp, &rig.state, &rig.clock).await;
    rig.state.inner.fail_token.store(true, Ordering::SeqCst);
    let outcome = rig
        .rp
        .complete_login(
            &CallbackQuery {
                code: Some(ONE_TIME_CODE.into()),
                state: Some(state_value),
                ..Default::default()
            },
            Some(&cookie),
            None,
        )
        .await;
    assert!(matches!(
        outcome.err(),
        Some(FlowRejection::UpstreamRejected(_))
    ));
}

#[tokio::test]
async fn session_row_permissions_snapshot_comes_from_the_policy_version() {
    let rig = rig(None, 8, true).await;
    let (state_value, cookie) = arm(&rig.rp, &rig.state, &rig.clock).await;
    let outcome = rig
        .rp
        .complete_login(
            &CallbackQuery {
                code: Some(ONE_TIME_CODE.into()),
                state: Some(state_value),
                ..Default::default()
            },
            Some(&cookie),
            None,
        )
        .await
        .expect("login completes");
    let session_id = cookie_value(&outcome.session_cookie, "__Host-dz_session");
    let unix_now = rig.clock.load(Ordering::SeqCst);
    let row = rig
        .sessions
        .validate(
            &session_id,
            Timestamp::new(rfc3339_at(unix_now + 60)),
            1_800,
        )
        .expect("session row validates");
    // permission_version 快照 = policy 当前版本（新组织初始 1）。
    assert_eq!(row.permission_version, 1);
    assert_eq!(
        row.mode,
        crate::storage::login_sessions::LoginSessionMode::Cookie
    );
    assert!(row.native_secret_hash.is_none());
}
