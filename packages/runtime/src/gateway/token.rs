//! CM-70：签名提交令牌（submission token）。
//!
//! # 这一层为什么存在
//!
//! CM-54 的账本按「幂等键」查重，而 CM-70 把那个键**升级成一张有签名、有有效期、
//! 绑定 owner 代次的令牌**。差别都在「谁说了算」：键是客户端送上来的字符串，
//! 服务端凭什么相信它的 `issuedAt`？凭什么相信它还没过期？凭什么相信它不是
//! 从上一个 owner 代次偷来的？——只靠一个不透明字符串，一条都答不上来。
//!
//! 所以令牌必须**自证**：
//!
//! - 载荷在服务端签发，客户端只能原样回送，**改一个字节就验不过**；
//! - `issued_at` / `expires_at` 写在载荷里，客户端**伪造不出来**（伪造 → 签名不符）；
//! - `key_version` 是签名密钥版本，**未知版本一律拒绝**（不是「尝试降级」）；
//! - `owner_runtime_epoch` 绑定签发时的 owner 代次，owner 重启后旧令牌自然失效。
//!
//! # 令牌串格式
//!
//! ```text
//! cm70.<keyVersion>.<payload-hex>.<mac-hex>
//! ```
//!
//! 全部 hex / ASCII，**单行、可打印、无凭据**。MAC 覆盖 `cm70.<keyVersion>.<payload-hex>`
//! 整段，因此连前缀与版本号都在签名范围内——把 `cm70.1.` 改成 `cm70.2.` 一样验不过。
//!
//! 为什么签名藏在**键字符串内部**：`platform-api` 的 `SubmissionToken` 只有
//! `{ idempotency_key, expires_at }` 两个字段，**没有签名字段可放**。而
//! `application/dto/requests.rs` 早就写明「键内容本身由 `SubmissionTokenIssuer`
//! 签发与核验」。两者一致：签名本就属于这串不透明文本，`ExecutionRequest.idempotency_key`
//! 就是令牌本身，令牌层与网关之间**不需要任何 DTO 改造**。
//!
//! # 分层
//!
//! 端口（`platform-api` 的 `SubmissionTokenIssuer`）一个字都没改，实现落在这里——
//! 与 `SessionDirectory` 端口 → `InMemorySessionDirectory` 实现是同一套惯例：
//! 端口在上游包，具体实现落 runtime，依赖方向 runtime → platform-api 合法。
//!
//! # MAC 原语
//!
//! 真 HMAC-SHA256（`hmac` + `sha2`），不是摘要。摘要在「伪造 issuedAt」这条判据下
//! 不成立：攻击者能构造碰撞，`issuedAt` 也就算被保护了——测试会跟着失去意义。
//! 令牌只需要一处 HMAC，所以刻意不引 `ring` 这类完整密码学栈。

use std::collections::hash_map::RandomState;
use std::collections::BTreeMap;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::connection::types::fnv1a64_hex;
use crate::connection::{Counter, ExecutionId};
use crate::gateway::idempotency::IdempotencyScope;
use crate::gateway::retention::{Grant, GrantRegistry, SweepReport};

type HmacSha256 = Hmac<Sha256>;

/// 令牌前缀。换前缀即换格式版本，与 `key_version` 互相独立。
pub const TOKEN_PREFIX: &str = "cm70";

/// 默认有效期：24 小时虚拟时间（§3.5）。
pub const DEFAULT_TTL_NANOS: u64 = 24 * 60 * 60 * 1_000_000_000;

/// 签名密钥版本。
///
/// 「未知版本一律拒绝」是 CM-70 的硬要求：密钥轮换期间如果对未知版本**尝试**降级，
/// 攻击者就能拿一个自己编造的版本号把令牌送进验签路径。所以这里只有两种结果——
/// 命中本版本，或拒绝。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyVersion(u32);

impl KeyVersion {
    pub fn new(version: u32) -> Self {
        Self(version)
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for KeyVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 令牌所担保的操作类别。绑定它是为了让令牌**不能跨操作复用**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubmissionOperation {
    /// 打开会话（owner 尚不存在，因此 `owner_runtime_epoch` 为 `None`）。
    OpenSession,
    /// 设置会话上下文。
    SetContext,
    /// 会话内执行（`owner_runtime_epoch` 必填）。
    ExecuteInSession,
}

impl SubmissionOperation {
    fn tag(self) -> &'static str {
        match self {
            Self::OpenSession => "open",
            Self::SetContext => "context",
            Self::ExecuteInSession => "execute",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "open" => Some(Self::OpenSession),
            "context" => Some(Self::SetContext),
            "execute" => Some(Self::ExecuteInSession),
            _ => None,
        }
    }

    /// 该操作是否必须绑定 owner 代次。
    pub fn requires_owner_epoch(self) -> bool {
        !matches!(self, Self::OpenSession)
    }
}

/// 令牌载荷（已验签，字段可信）。
///
/// 字段私有且只经 [`TokenKeyring::issue`] 产生——这就是「`issuedAt` 由服务端登记、
/// 客户端伪造不出来」在代码层面的形状：客户端没有任何构造入口。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenPayload {
    nonce: String,
    operation: SubmissionOperation,
    owner_runtime_epoch: Option<Counter>,
    issued_at_nanos: u64,
    expires_at_nanos: u64,
}

impl TokenPayload {
    pub fn nonce(&self) -> &str {
        &self.nonce
    }

    pub fn operation(&self) -> SubmissionOperation {
        self.operation
    }

    pub fn owner_runtime_epoch(&self) -> Option<Counter> {
        self.owner_runtime_epoch
    }

    pub fn issued_at_nanos(&self) -> u64 {
        self.issued_at_nanos
    }

    pub fn expires_at_nanos(&self) -> u64 {
        self.expires_at_nanos
    }

    /// 是否已过期。判据是「服务端现在 ≥ 签发的 expires_at」——**只看服务端时钟**，
    /// 客户端报上来的时间一律不作数（它就是伪造目标）。
    pub fn is_expired_at(&self, server_now_nanos: u64) -> bool {
        server_now_nanos >= self.expires_at_nanos
    }

    /// 编码成载荷段。字段用 `|` 分隔，nonce 已在构造时保证不含 `|`。
    fn encode(&self) -> String {
        let epoch = match self.owner_runtime_epoch {
            Some(epoch) => epoch.get().to_string(),
            None => "none".to_owned(),
        };
        format!(
            "{}|{}|{}|{}|{}",
            self.nonce,
            self.operation.tag(),
            epoch,
            self.issued_at_nanos,
            self.expires_at_nanos
        )
    }

    fn decode(text: &str) -> Option<Self> {
        let mut parts = text.split('|');
        let nonce = parts.next()?.to_owned();
        let operation = SubmissionOperation::parse(parts.next()?)?;
        let owner_runtime_epoch = match parts.next()? {
            "none" => None,
            raw => Some(Counter::new(raw.parse().ok()?)),
        };
        let issued_at_nanos = parts.next()?.parse().ok()?;
        let expires_at_nanos = parts.next()?.parse().ok()?;
        // 多余一段就是畸形负载，不做「取前五段」的宽容解析。
        if parts.next().is_some() {
            return None;
        }
        if nonce.is_empty() || nonce.contains('|') {
            return None;
        }
        Some(Self {
            nonce,
            operation,
            owner_runtime_epoch,
            issued_at_nanos,
            expires_at_nanos,
        })
    }
}

/// 拒绝理由。**每一种都不执行**——令牌闸门没有「降级放行」这条路。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenRejection {
    /// 结构不对（段数、前缀、载荷编码）。
    Malformed,
    /// 签名密钥版本未知。**一律拒绝，不尝试降级。**
    UnknownKeyVersion,
    /// MAC 不符：内容被改过，或用别的密钥签的。
    SignatureMismatch,
    /// 已过期。
    Expired,
    /// owner 代次不符（owner 重启后的旧令牌）。
    OwnerEpochMismatch,
    /// 已被保留期清扫退役。**即便令牌自己还没过期也拒绝**——见 [`GrantRegistry`]。
    Tombstoned,
}

impl TokenRejection {
    /// 机器可读理由，落进 `to_persistable_json()`。
    pub fn reason(self) -> &'static str {
        match self {
            Self::Malformed => "submissionTokenMalformed",
            Self::UnknownKeyVersion => "submissionTokenKeyVersionUnknown",
            Self::SignatureMismatch => "submissionTokenSignatureInvalid",
            Self::Expired => "submissionTokenExpired",
            Self::OwnerEpochMismatch => "submissionTokenOwnerEpochMismatch",
            Self::Tombstoned => "submissionTokenRetired",
        }
    }
}

/// 签发失败。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenIssueError {
    /// 当前版本没有密钥材料。
    UnknownCurrentKeyVersion,
    /// 该操作的语义要求绑定 owner 代次，却没给。
    OwnerEpochRequired(SubmissionOperation),
    /// `issued_at + ttl` 溢出单调纳秒空间。
    ExpiryOverflow,
}

/// 令牌摘要。作为授予登记表与墓碑的索引键。
///
/// 刻意**不是**密码学摘要：它是索引，不是安全边界。理论上两个不同令牌摘要相同会让
/// 其中一个被另一个的墓碑挡住——那个方向是**失败即拒绝**（fail-closed），
/// 而把摘要换成 SHA-256 只会掩盖「令牌表是内存表」这个事实。
pub fn token_digest(token: &str) -> String {
    fnv1a64_hex(token.as_bytes())
}

/// 签名密钥环。
///
/// `Debug` 是**手写**的，这是有代价的选择：派生版会把 `secrets` 原样打印出来，而这里装
/// 的正是用来签发与验签的密钥。`Debug` 的输出会进 `tracing`、日志文件与第三方采集器，
/// 一旦流出去，任何人都能拿它签出网关肯收的令牌——那不再是调试信息，那是**签发能力**
/// 的泄漏。所以这里只打版本结构：一个调试输出不该改变攻击者的能力。
pub struct TokenKeyring {
    current: KeyVersion,
    secrets: BTreeMap<KeyVersion, Vec<u8>>,
    sequence: AtomicU64,
}

impl std::fmt::Debug for TokenKeyring {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenKeyring")
            .field("current", &self.current)
            .field("versions", &self.secrets.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl TokenKeyring {
    /// 单版本密钥环。
    pub fn single(secret: impl Into<Vec<u8>>) -> Self {
        let version = KeyVersion::new(1);
        Self::rotate(vec![(version, secret.into())], version)
    }

    /// 多版本密钥环，`current` 为签发用版本；其余版本**仅供验签**（轮换窗口内）。
    pub fn rotate(secrets: Vec<(KeyVersion, Vec<u8>)>, current: KeyVersion) -> Self {
        Self {
            current,
            secrets: secrets.into_iter().collect(),
            sequence: AtomicU64::new(0),
        }
    }

    pub fn current_version(&self) -> KeyVersion {
        self.current
    }

    pub fn shared(self) -> Arc<Self> {
        Arc::new(self)
    }

    /// 签发一张令牌。
    pub fn issue(
        &self,
        operation: SubmissionOperation,
        owner_runtime_epoch: Option<Counter>,
        issued_at_nanos: u64,
        ttl_nanos: u64,
    ) -> Result<String, TokenIssueError> {
        let secret = self
            .secrets
            .get(&self.current)
            .ok_or(TokenIssueError::UnknownCurrentKeyVersion)?;
        let expires_at_nanos = issued_at_nanos
            .checked_add(ttl_nanos)
            .ok_or(TokenIssueError::ExpiryOverflow)?;
        // 该操作的语义要求绑定代次却没给，就是调用方的 bug，不签。
        if operation.requires_owner_epoch() && owner_runtime_epoch.is_none() {
            return Err(TokenIssueError::OwnerEpochRequired(operation));
        }
        let payload = TokenPayload {
            nonce: fresh_nonce(self.sequence.fetch_add(1, Ordering::Relaxed)),
            operation,
            owner_runtime_epoch,
            issued_at_nanos,
            expires_at_nanos,
        };
        Ok(self.render(self.current, secret, &payload))
    }

    fn render(&self, version: KeyVersion, secret: &[u8], payload: &TokenPayload) -> String {
        let payload_hex = to_hex(payload.encode().as_bytes());
        let mac_hex = to_hex(&mac_of(secret, version, &payload_hex));
        format!("{TOKEN_PREFIX}.{version}.{payload_hex}.{mac_hex}")
    }

    /// 验签 + 判过期 + 比代次。三道闸**任一不过都拒绝**，没有例外分支。
    pub fn verify(
        &self,
        token: &str,
        server_now_nanos: u64,
        expected_owner_epoch: Option<Counter>,
    ) -> Result<TokenPayload, TokenRejection> {
        let mut parts = token.split('.');
        let prefix = parts.next().ok_or(TokenRejection::Malformed)?;
        let version_raw = parts.next().ok_or(TokenRejection::Malformed)?;
        let payload_hex = parts.next().ok_or(TokenRejection::Malformed)?;
        let mac_hex = parts.next().ok_or(TokenRejection::Malformed)?;
        if parts.next().is_some() || prefix != TOKEN_PREFIX {
            return Err(TokenRejection::Malformed);
        }
        let version = KeyVersion::new(version_raw.parse().map_err(|_| TokenRejection::Malformed)?);
        // 版本先查再验签：未知版本**立刻**拒绝，不把它送进 MAC 计算去比对，
        // 免得「密钥是否存在」变成一个可测量的时间信号。
        let secret = self
            .secrets
            .get(&version)
            .ok_or(TokenRejection::UnknownKeyVersion)?;

        let presented = from_hex(mac_hex).ok_or(TokenRejection::Malformed)?;
        let mut verifier = match HmacSha256::new_from_slice(secret) {
            Ok(mac) => mac,
            // 不可达：HMAC 接受任意长度密钥。失败即返回空 MAC，
            // 于是 `verify_slice` 必然不通过——失败方向永远是拒绝。
            Err(_) => return Err(TokenRejection::SignatureMismatch),
        };
        verifier.update(mac_input(version, payload_hex).as_bytes());
        verifier
            .verify_slice(&presented)
            .map_err(|_| TokenRejection::SignatureMismatch)?;

        let payload_text =
            String::from_utf8(from_hex(payload_hex).ok_or(TokenRejection::Malformed)?)
                .map_err(|_| TokenRejection::Malformed)?;
        let payload = TokenPayload::decode(&payload_text).ok_or(TokenRejection::Malformed)?;

        if payload.is_expired_at(server_now_nanos) {
            return Err(TokenRejection::Expired);
        }
        // 代次绑定：该绑的必须绑，绑了就必须相等。
        // 唯一例外是「令牌没绑、且它的操作语义本来就不要求绑、且验签方也没有
        // 期望代次」——只有这种组合才放行；其余一律 OwnerEpochMismatch。
        let epoch_ok = match (payload.owner_runtime_epoch, expected_owner_epoch) {
            (Some(bound), Some(expected)) => bound == expected,
            (None, None) => !payload.operation.requires_owner_epoch(),
            _ => false,
        };
        if !epoch_ok {
            return Err(TokenRejection::OwnerEpochMismatch);
        }
        Ok(payload)
    }
}

fn mac_input(version: KeyVersion, payload_hex: &str) -> String {
    format!("{TOKEN_PREFIX}.{version}.{payload_hex}")
}

fn mac_of(secret: &[u8], version: KeyVersion, payload_hex: &str) -> Vec<u8> {
    let mut mac = match HmacSha256::new_from_slice(secret) {
        Ok(mac) => mac,
        Err(_) => return Vec::new(),
    };
    mac.update(mac_input(version, payload_hex).as_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// nonce：每次签发都不同。
///
/// 没有 `rand` 依赖，就从两个**各自独立**的 `RandomState` 取种子——`RandomState::new()`
/// 由 OS 播种，进程内一次性随机。再叠一个实例内自增序号，保证「同一个 keyring 实例内
/// 不可能重复」，这两条合起来就是 nonce 需要的全部性质。
fn fresh_nonce(sequence: u64) -> String {
    let mut out = String::with_capacity(64);
    for _ in 0..2 {
        let hasher = RandomState::new().build_hasher();
        let bytes = hasher.finish().to_le_bytes();
        out.push_str(&fnv1a64_hex(&bytes));
        out.push('-');
    }
    out.push_str(&format!("{sequence:016x}"));
    out
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // write! 到 String 不会失败；用 push 免去 format 样板。
        out.push(char::from_digit(u32::from(byte >> 4), 16).unwrap_or('0'));
        out.push(char::from_digit(u32::from(byte & 0x0f), 16).unwrap_or('0'));
    }
    out
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(text.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let hi = char::from(pair[0]).to_digit(16)?;
        let lo = char::from(pair[1]).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
    }
    Some(out)
}

/// 一次通过令牌闸门的请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenAdmission {
    digest: String,
    payload: TokenPayload,
}

impl TokenAdmission {
    pub fn digest(&self) -> &str {
        &self.digest
    }

    pub fn payload(&self) -> &TokenPayload {
        &self.payload
    }
}

/// 令牌闸门：密钥环 + 保留期登记表。
///
/// 放在网关受理路径的**第 0 步**，先于一切。顺序不是风格问题：
/// 「过期令牌重放不再执行」和「删除记录后重放不再执行」这两条，**只有在闸门排在
/// 账本查重之前时才成立**。若闸门排在查重之后，记录被删干净的过期令牌会一路走到
/// `IdempotencyLookup::Miss`，然后被真的执行一遍——正是 CM-70 明令禁止的那件事。
pub struct SubmissionTokenGuard {
    keyring: Arc<TokenKeyring>,
    registry: Arc<GrantRegistry>,
}

impl std::fmt::Debug for SubmissionTokenGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubmissionTokenGuard")
            .field("keyring", &self.keyring)
            .field("registry", &self.registry)
            .finish()
    }
}

impl SubmissionTokenGuard {
    pub fn new(keyring: Arc<TokenKeyring>, registry: Arc<GrantRegistry>) -> Self {
        Self { keyring, registry }
    }

    pub fn shared(keyring: Arc<TokenKeyring>, registry: Arc<GrantRegistry>) -> Arc<Self> {
        Arc::new(Self::new(keyring, registry))
    }

    pub fn registry(&self) -> &Arc<GrantRegistry> {
        &self.registry
    }

    pub fn keyring(&self) -> &Arc<TokenKeyring> {
        &self.keyring
    }

    /// 签发一张令牌，并按 `issued_at + ttl` 算出它的到期时刻。
    pub fn issue(
        &self,
        operation: SubmissionOperation,
        owner_runtime_epoch: Option<Counter>,
        issued_at_nanos: u64,
        ttl_nanos: u64,
    ) -> Result<String, TokenIssueError> {
        self.keyring
            .issue(operation, owner_runtime_epoch, issued_at_nanos, ttl_nanos)
    }

    /// 受理路径的第 0 步。任何拒绝都**不会**走到账本，也**不会**分配 `executionId`。
    pub fn admit(
        &self,
        token: &str,
        server_now_nanos: u64,
        expected_owner_epoch: Option<Counter>,
    ) -> Result<TokenAdmission, TokenRejection> {
        let digest = token_digest(token);
        // 墓碑先查：一条被提前退役的令牌即便签名完好、尚未过期，也必须拒绝。
        if self.registry.is_retired(&digest) {
            return Err(TokenRejection::Tombstoned);
        }
        let payload = self
            .keyring
            .verify(token, server_now_nanos, expected_owner_epoch)?;
        Ok(TokenAdmission { digest, payload })
    }

    /// 首次受理成功后登记授予。`scope` 是这条幂等记录在 CM-54 账本里的键，
    /// 清扫时靠它把账本记录一并删掉。
    pub fn register(
        &self,
        admission: &TokenAdmission,
        execution_id: &ExecutionId,
        scope: &IdempotencyScope,
    ) {
        let payload = admission.payload();
        self.registry.record(
            admission.digest(),
            Grant::for_execution(
                execution_id.clone(),
                scope.clone(),
                payload.issued_at_nanos(),
                payload.expires_at_nanos(),
            ),
        );
    }

    /// 保留期清扫。到期且过保留期的授予被退役，未到期的**一个都不动**。
    pub fn sweep(&self, now_nanos: u64) -> SweepReport {
        self.registry.sweep(now_nanos)
    }
}
