//! CSRF：会话绑定的**签名双提交令牌**（team-server §5.1 的纯逻辑半边）。
//!
//! 令牌结构（§5.1 逐字）：
//!
//! ```text
//! base64url(random) || "." || base64url(HMAC(secret, login_session_id || ":" || random))
//! ```
//!
//! 同时写入 `__Host-dz_csrf` cookie 与响应体/前端缓存；校验三件事：
//!
//! 1. cookie 与 header 提交值的 `random` 分量**常量时间相等**（[`subtle`]
//!    的 `ct_eq`——§5.1 明写常量时间，手写 XOR 折叠不进安全面）；
//! 2. HMAC 在**当前请求解析出的登录会话 id** 下验证通过（换会话即失效）；
//! 3. 令牌所属会话行未被撤销。
//!
//! 第 3 件事由**层序**与参数共同保证：L2（认证）先于 L4（CSRF）执行，
//! 已撤销会话在 L2 就被 401 拒绝、到不了本模块；[`verify`] 仍显式接收
//! `session_active` 参数，让三条校验在**一个可测的纯函数里**闭合，而不是
//! 靠「上游一定挡住」的口头约定（中间件层的组合钉子见
//! `http::middleware::csrf` 与 `tests/auth_chain.rs`）。
//!
//! # 失败行为（§5.3）
//!
//! 所有拒绝变体在 HTTP 层统一映射 403 `PermissionDenied`（`retryDisposition
//! = never`），响应体不新增区分字段——对攻击者而言 CSRF 失败与普通无权
//! 本就不可区分；`csrf_rejected` 审计事件由 L4 中间件落账（带 requestId
//! 与路由）。
//!
//! # SSE 与 auth 路由的豁免
//!
//! `GET /api/v1/events/{streamId}`（EventSource 无法设自定义头）与
//! `GET /api/v1/auth/callback`（自身以 state 校验防登录 CSRF）不需要本
//! 令牌——豁免表在 L4 中间件的路径判定里（§5.2）；P8 native 已认证 API
//! 不使用 cookie CSRF（§5.2），随 native 凭据切片进入。

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

/// CSRF 令牌的 cookie 名（§4.4：`__Host-` 前缀 + `SameSite=Strict` +
/// 非 HttpOnly——前端需读取以放入请求头）。
pub const CSRF_COOKIE: &str = "__Host-dz_csrf";

/// CSRF 令牌的请求头名。
///
/// team-server §5 只约定了「前端经统一 `BackendClient` 封装注入令牌」，
/// 未冻结 header 字面名；`x-dz-csrf-token` 是本实现的注册名，wire 冻结
/// 随 web shell 的 `BackendClient` 切片一起落契约表（模块报告已登记）。
pub const CSRF_HEADER: &str = "x-dz-csrf-token";

/// `random` 分量的字节长度（256-bit，远超 §4.2 对 state 的 128-bit 下限
/// ——同一保密等级的在线令牌）。
const RANDOM_BYTES: usize = 32;

/// CSRF 签名密钥（`server_secret`，config 注入）。`Debug` 刻意脱敏：
/// 密钥形状不得进日志（与 `opaque_id!` 宏同一条纪律）。
#[derive(Clone)]
pub struct CsrfSecret(Vec<u8>);

impl CsrfSecret {
    /// 空密钥拒绝构造（fail-closed：空密钥的 HMAC 是公开常量）。
    pub fn new(secret: Vec<u8>) -> Result<Self, String> {
        if secret.is_empty() {
            return Err("csrf secret must not be empty".into());
        }
        Ok(Self(secret))
    }

    fn mac(&self, message: &[u8]) -> Result<[u8; 32], String> {
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.0)
            .map_err(|_| "csrf secret is not usable as an HMAC key".to_string())?;
        mac.update(message);
        let digest = mac.finalize().into_bytes();
        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        Ok(out)
    }
}

impl std::fmt::Debug for CsrfSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CsrfSecret(<redacted>)")
    }
}

/// 校验拒绝的原因。**仅供日志与审计**；HTTP 响应统一 403 不区分（§5.3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsrfRejection {
    /// 令牌不是「`random.mac`」两段 base64url 结构，或 mac 不可解码。
    Malformed,
    /// cookie 与 header 的 `random` 分量不一致（双提交失败）。
    RandomMismatch,
    /// HMAC 验证失败（换会话、换密钥或令牌被篡改——三者对外同形）。
    MacMismatch,
    /// 会话行已撤销（§5.1 校验项 3；层序上通常到不了这里，参数保留）。
    SessionInactive,
}

/// 铸造一枚绑定 `login_session_id` 的令牌（登录成功后随 `Set-Cookie` 与
/// 响应体下发）。
pub fn mint(secret: &CsrfSecret, login_session_id: &str) -> Result<String, String> {
    let mut random = [0u8; RANDOM_BYTES];
    // CSPRNG：令牌的 random 分量参与双提交与 HMAC 输入，弱随机 = 令牌可猜。
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut random);
    let random_part = URL_SAFE_NO_PAD.encode(random);
    let mac = secret.mac(format!("{login_session_id}:{random_part}").as_bytes())?;
    Ok(format!("{random_part}.{}", URL_SAFE_NO_PAD.encode(mac)))
}

/// 校验三件事（§5.1）。`cookie_token` / `header_token` 是双提交的两个
/// 半边；`session_active` 为 `false` 时（会话已撤销）一律
/// [`CsrfRejection::SessionInactive`]。
pub fn verify(
    secret: &CsrfSecret,
    login_session_id: &str,
    cookie_token: &str,
    header_token: &str,
    session_active: bool,
) -> Result<(), CsrfRejection> {
    if !session_active {
        return Err(CsrfRejection::SessionInactive);
    }

    // 结构：两段式 base64url，mac 段必须可解码（固定 32 字节摘要）。
    let (cookie_random, cookie_mac) = split_token(cookie_token)?;
    let (header_random, header_mac) = split_token(header_token)?;

    // 校验项 1：双提交的 random 常量时间相等。
    if !constant_time_eq(cookie_random.as_bytes(), header_random.as_bytes()) {
        return Err(CsrfRejection::RandomMismatch);
    }
    // 校验项 2：HMAC 绑定当前登录会话 id（对 header 提交的 mac 验证）。
    let expected = secret
        .mac(format!("{login_session_id}:{header_random}").as_bytes())
        .map_err(|_| CsrfRejection::MacMismatch)?;
    if !constant_time_eq(&expected, &header_mac) {
        return Err(CsrfRejection::MacMismatch);
    }
    // 校验项 3 已由 session_active 参数表达（见模块头）。
    let _ = cookie_mac; // cookie 的 mac 不参与验证——签名只认 header 提交值；
                       // cookie 半边的完整性由同值断言（校验项 1）与 HMAC
                       // 共同保证。
    Ok(())
}

fn split_token(token: &str) -> Result<(String, [u8; 32]), CsrfRejection> {
    let (random, mac_part) = token.split_once('.').ok_or(CsrfRejection::Malformed)?;
    if random.is_empty() {
        return Err(CsrfRejection::Malformed);
    }
    let mac_bytes = URL_SAFE_NO_PAD
        .decode(mac_part)
        .map_err(|_| CsrfRejection::Malformed)?;
    let mac: [u8; 32] = mac_bytes
        .try_into()
        .map_err(|_| CsrfRejection::Malformed)?;
    Ok((random.to_owned(), mac))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.ct_eq(b).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret() -> CsrfSecret {
        CsrfSecret::new(b"test-server-secret-32-bytes-minimum!!".to_vec()).expect("secret")
    }

    #[test]
    fn minted_tokens_are_two_base64url_parts_and_verify_round_trips() {
        let secret = secret();
        let token = mint(&secret, "session-1").expect("mint");
        let (random, mac) = token.split_once('.').expect("two parts");
        assert_eq!(random.len(), 43, "32 random bytes → 43 base64url chars");
        assert_eq!(mac.len(), 43, "HMAC-SHA256 → 43 base64url chars");
        assert!(
            token
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_'),
            "token must be base64url + separator: {token}"
        );
        assert_eq!(
            verify(&secret, "session-1", &token, &token, true),
            Ok(()),
            "cookie 与 header 提交同一令牌且会话存活 → 通过"
        );
    }

    #[test]
    fn each_mint_is_fresh() {
        let secret = secret();
        assert_ne!(
            mint(&secret, "session-1").expect("mint"),
            mint(&secret, "session-1").expect("mint"),
            "random 分量必须每次铸造都新鲜（防重放同值）"
        );
    }

    #[test]
    fn double_submit_random_mismatch_is_rejected() {
        let secret = secret();
        let cookie_token = mint(&secret, "session-1").expect("mint");
        let header_token = mint(&secret, "session-1").expect("mint");
        assert_eq!(
            verify(&secret, "session-1", &cookie_token, &header_token, true),
            Err(CsrfRejection::RandomMismatch),
            "两半边 random 不同 → 双提交失败"
        );
    }

    #[test]
    fn the_hmac_binds_the_login_session_id() {
        let secret = secret();
        let token = mint(&secret, "session-1").expect("mint");
        // 同一令牌在另一个会话 id 下验证必失败（§5.1 校验项 2：换会话即失效）。
        assert_eq!(
            verify(&secret, "session-2", &token, &token, true),
            Err(CsrfRejection::MacMismatch),
        );
    }

    #[test]
    fn a_rotated_secret_rejects_old_tokens() {
        let old = secret();
        let rotated = CsrfSecret::new(b"rotated-server-secret-32-bytes-min!".to_vec()).expect("secret");
        let token = mint(&old, "session-1").expect("mint");
        assert_eq!(
            verify(&rotated, "session-1", &token, &token, true),
            Err(CsrfRejection::MacMismatch),
            "密钥轮换后旧令牌即失效"
        );
    }

    #[test]
    fn tampered_and_malformed_tokens_are_rejected() {
        let secret = secret();
        let token = mint(&secret, "session-1").expect("mint");
        let (random, mac) = token.split_once('.').expect("two parts");

        // 篡改 mac（长度/字符集不变，只换首位——仍是合法 base64url，只是签名不再匹配）。
        let tampered_mac = if mac.starts_with('A') {
            format!("B{}", &mac[1..])
        } else {
            format!("A{}", &mac[1..])
        };
        let tampered = format!("{random}.{tampered_mac}");
        assert_eq!(
            verify(&secret, "session-1", &tampered, &tampered, true),
            Err(CsrfRejection::MacMismatch),
        );
        // 篡改 random（结构在、签名不再匹配）。
        let flipped = if random.starts_with('A') {
            format!("B{}.{mac}", &random[1..])
        } else {
            format!("A{}.{mac}", &random[1..])
        };
        assert_eq!(
            verify(&secret, "session-1", &flipped, &flipped, true),
            Err(CsrfRejection::MacMismatch),
        );
        // 结构破坏：无分隔符 / 空 random / mac 不可解码。
        assert_eq!(
            verify(&secret, "session-1", "no-separator", "no-separator", true),
            Err(CsrfRejection::Malformed),
        );
        assert_eq!(
            verify(&secret, "session-1", ".mac", ".mac", true),
            Err(CsrfRejection::Malformed),
        );
        assert_eq!(
            verify(&secret, "session-1", &format!("{random}.%%%"), &format!("{random}.%%%"), true),
            Err(CsrfRejection::Malformed),
        );
    }

    /// §5.1 校验项 3：会话撤销即拒绝（纯函数参数；组合语义由 L2→L4 层序
    /// 与探针测试共同钉住）。
    #[test]
    fn an_inactive_session_is_rejected_regardless_of_the_token() {
        let secret = secret();
        let token = mint(&secret, "session-1").expect("mint");
        assert_eq!(
            verify(&secret, "session-1", &token, &token, false),
            Err(CsrfRejection::SessionInactive),
        );
    }

    #[test]
    fn an_empty_secret_is_rejected_at_construction() {
        assert!(CsrfSecret::new(Vec::new()).is_err());
    }

    #[test]
    fn the_secret_debug_output_is_redacted() {
        let rendered = format!("{:?}", secret());
        assert_eq!(rendered, "CsrfSecret(<redacted>)");
    }
}
