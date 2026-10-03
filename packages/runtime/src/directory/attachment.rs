//! attachment 令牌与归属校验。
//!
//! 令牌是**能力凭据**，不是路由依据：
//!
//! * 原文只在创建/替换时发给原 owner，**只存在于调用方内存**；
//! * 目录侧**只存摘要**（platform-api 的 `AttachmentToken` 文档就是这个要求）；
//! * 摘要**不出现**在 `SessionOwner`、`SessionSnapshot`、任何 `Debug` 或事件投影里——
//!   目录压根没有一个字段能装下它；
//! * 旧令牌对新会话**没有**挂载权利：签发时的绑定关系是「令牌 ↔ 签发它的那次登记」，
//!   替换换了 `dbSessionId`，旧令牌就再也对不上；
//! * 归属身份（editor / job / 客户端 / principal）必须单独校验，光有令牌不够。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use datazen_platform_api::context::OwnerRef;
use datazen_platform_api::id::{AttachmentToken, ClientInstanceId, JobId, PrincipalId, Timestamp};
use datazen_platform_api::ports::session_directory::{owner_matches_client, owner_matches_job};

use super::entry::{DirectoryEntry, RouteRejection};
use super::id::hex_encode;
use super::{DirectoryClock, SessionIdEntropy};

/// 令牌摘要。
///
/// 这是**进程内比对**用的摘要，不是密码哈希，也绝不落盘。目录不落盘、令牌又是每次会话
/// 新生成的随机串，所以 128 位进程内摘要足够支撑「是不是同一枚令牌」的判断，
/// 同时不必为一个从不离开内存的凭据引入密码学依赖。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TokenDigest {
    high: u64,
    low: u64,
}

impl TokenDigest {
    pub const fn new(high: u64, low: u64) -> Self {
        Self { high, low }
    }

    pub const fn halves(self) -> (u64, u64) {
        (self.high, self.low)
    }
}

/// 由令牌原文算摘要。原文到此为止，不再被任何人保存。
pub fn digest_token(token: &AttachmentToken) -> TokenDigest {
    let mut high = DefaultHasher::new();
    token.as_str().hash(&mut high);
    high.write_u8(0xa5);
    let mut low = DefaultHasher::new();
    token.as_str().hash(&mut low);
    low.write_u8(0x5a);
    TokenDigest::new(high.finish(), low.finish())
}

/// 签发一枚 attachment 令牌。前缀与 `dbSessionId` / `runtimeEpoch` 区分开，
/// 便于日志里一眼认出这是凭据而不是标识。
pub fn new_attachment_token(entropy: &Arc<dyn SessionIdEntropy>) -> AttachmentToken {
    let mut raw = [0u8; 16];
    entropy.fill(&mut raw);
    AttachmentToken::new(format!("att_{}", hex_encode(&raw)))
}

/// 挂载方声明自己是谁。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentClaim {
    /// 客户端页签 / 编辑器窗口。
    Client(ClientInstanceId),
    /// Job 调度出的执行体。
    Job(JobId),
    /// 只带令牌的重连（例如 MCP 侧重连），不声明具体身份。
    ///
    /// 这种情况下令牌本身就是完整凭据，因此仍然必须出示令牌；
    /// 身份声明一旦给出，就必须与 owner 对得上。
    Anonymous,
}

/// 一次挂载请求。
#[derive(Debug, Clone)]
pub struct AttachmentRequest {
    pub handle: datazen_platform_api::dto::session::SessionHandle,
    pub principal_id: PrincipalId,
    pub claim: AttachmentClaim,
    /// 调用方内存里的令牌。**不带就是无凭据挂载，一律拒绝。**
    pub token: Option<AttachmentToken>,
}

impl AttachmentRequest {
    pub fn new(
        handle: datazen_platform_api::dto::session::SessionHandle,
        principal_id: PrincipalId,
        claim: AttachmentClaim,
        token: Option<AttachmentToken>,
    ) -> Self {
        Self {
            handle,
            principal_id,
            claim,
            token,
        }
    }

    /// 无凭据的重连：只有句柄，没有令牌。契约要求这种请求被拒绝。
    pub fn credential_less(
        handle: datazen_platform_api::dto::session::SessionHandle,
        principal_id: PrincipalId,
    ) -> Self {
        Self::new(handle, principal_id, AttachmentClaim::Anonymous, None)
    }
}

/// 挂载被拒的理由。**逐条可区分**，因为调用方要据此决定是重连、换身份还是报会话丢失。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentRejection {
    /// 无凭据：不带令牌就挂。
    NoToken,
    /// 这个会话没有签发过令牌。
    NoTokenIssued,
    /// 令牌不是本次登记签发的那一枚（含「拿旧会话的令牌来挂新会话」）。
    TokenMismatch,
    /// principal 与 owner 不符。
    PrincipalMismatch,
    /// 声明的身份与 owner 不符（编辑器不属于该 client / job 不是该 job）。
    OwnerMismatch,
    /// 路由不通：未知 ID、旧 epoch、屏障中、已到期、已关闭。
    NotRoutable(RouteRejection),
}

impl AttachmentRejection {
    /// 稳定的机器可读标签。
    pub const fn code(&self) -> &'static str {
        match self {
            AttachmentRejection::NoToken => "attachmentTokenMissing",
            AttachmentRejection::NoTokenIssued => "attachmentTokenNotIssued",
            AttachmentRejection::TokenMismatch => "attachmentTokenInvalid",
            AttachmentRejection::PrincipalMismatch => "attachmentPrincipalMismatch",
            AttachmentRejection::OwnerMismatch => "attachmentOwnerMismatch",
            AttachmentRejection::NotRoutable(rejection) => rejection.code(),
        }
    }

    /// 转成端口错误。仍然只用 `PortError` 已有的变体。
    ///
    /// 路由拒绝的映射与 [`RouteRejection::to_port_error`] 是同一套：`Unknown`
    /// 一律回「查无此会话」，不承认这条会话曾经存在过。
    pub fn to_port_error(
        &self,
        db_session_id: &datazen_platform_api::id::DbSessionId,
    ) -> datazen_platform_api::error::PortError {
        use datazen_platform_api::error::PortError;
        match self {
            AttachmentRejection::NoToken
            | AttachmentRejection::NoTokenIssued
            | AttachmentRejection::TokenMismatch => PortError::TokenInvalid,
            AttachmentRejection::NotRoutable(rejection) => rejection.to_port_error(db_session_id),
            other => PortError::NotFound(format!("attachment rejected: {}", other.code())),
        }
    }
}

/// 挂载成功后的结果。**不返回令牌**：令牌只在签发时给原 owner，挂载不重发。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentOutcome {
    /// 本次把会话挂上了。
    Attached { expires_at: Option<Timestamp> },
    /// 已经是挂着的状态。幂等，**不**刷新任何期限。
    AlreadyAttached { expires_at: Option<Timestamp> },
}

impl AttachmentOutcome {
    pub const fn is_attached(&self) -> bool {
        matches!(self, AttachmentOutcome::Attached { .. })
    }

    pub const fn expires_at(&self) -> Option<&Timestamp> {
        match self {
            AttachmentOutcome::Attached { expires_at }
            | AttachmentOutcome::AlreadyAttached { expires_at } => expires_at.as_ref(),
        }
    }
}

/// 校验一次挂载请求。
///
/// 检查顺序是刻意的，理由写在每一步旁边：
/// 1. **先过路由闸**——未知 ID、旧 epoch、屏障、已到期、已关闭都不给挂，这是状态问题，
///    跟凭据无关，先说清楚状态，调用方才知道该不该去补令牌。
/// 2. **再校凭据**——无令牌直接拒；会话没签发过令牌也拒。
/// 3. **再校身份**——principal 与声明身份都要与 owner 对得上。
pub fn authorize_attachment(
    entry: &mut DirectoryEntry,
    request: &AttachmentRequest,
    stored_digest: Option<TokenDigest>,
    clock: &dyn DirectoryClock,
) -> Result<AttachmentOutcome, AttachmentRejection> {
    entry
        .admit_attachment(&request.handle)
        .map_err(AttachmentRejection::NotRoutable)?;

    let presented = request.token.as_ref().ok_or(AttachmentRejection::NoToken)?;
    let expected = stored_digest.ok_or(AttachmentRejection::NoTokenIssued)?;
    if digest_token(presented) != expected {
        return Err(AttachmentRejection::TokenMismatch);
    }

    let owner = entry.owner();
    if owner.principal_id != request.principal_id {
        return Err(AttachmentRejection::PrincipalMismatch);
    }
    match &request.claim {
        AttachmentClaim::Client(client_instance_id) => {
            if !owner_matches_client(&owner.owner, client_instance_id) {
                return Err(AttachmentRejection::OwnerMismatch);
            }
        }
        AttachmentClaim::Job(job_id) => {
            if !owner_matches_job(&owner.owner, job_id) {
                return Err(AttachmentRejection::OwnerMismatch);
            }
        }
        // 只带令牌的重连：令牌即凭据，身份不做声明。
        AttachmentClaim::Anonymous => {}
    }

    let expires_at = entry.expiration_projection(clock);
    // `DirectoryEntry::attach` 返回的是「这次是否真的挂上了」。
    let freshly_attached = entry.attach();
    Ok(if freshly_attached {
        AttachmentOutcome::Attached { expires_at }
    } else {
        AttachmentOutcome::AlreadyAttached { expires_at }
    })
}

/// 一个 owner 能不能用 `claim` 挂载。与 [`authorize_attachment`] 共用同一套判定，
/// 供只想做准入检查、不想真挂载的调用方使用。
pub fn claim_is_admissible(owner_ref: &OwnerRef, claim: &AttachmentClaim) -> bool {
    match claim {
        AttachmentClaim::Client(client_instance_id) => {
            owner_matches_client(owner_ref, client_instance_id)
        }
        AttachmentClaim::Job(job_id) => owner_matches_job(owner_ref, job_id),
        AttachmentClaim::Anonymous => true,
    }
}
