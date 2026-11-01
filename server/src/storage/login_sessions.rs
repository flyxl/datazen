//! 登录会话存储（WR-D3 第一刀）：`login_sessions` 表（v5）的宿主侧仓储。
//!
//! 会话是**服务端行**，不是自包含令牌（team-server §4.3）：登出、撤销与
//! 权限变更因此可以立即生效。本模块实现三个动词：
//!
//! - [`LoginSessions::create`]：登录成功后落行（绝对/空闲过期、权限版本
//!   快照、auth_mode；cookie 路径本刀启用，native 只保留列与 CHECK）。
//! - [`LoginSessions::validate`]：L2 中间件的逐请求校验——过期/撤销检查
//!   + **touch 顺延空闲期**（§4.3：`idle_expires_at` 只由入站请求顺延；
//!   SSE 心跳不刷新它是上游语义，本层只提供「入站请求」这一个顺延入口）。
//!   顺延上限是 `absolute_expires_at`（`ck_login_window` 的要求：空闲窗
//!   不得越过绝对窗）。
//! - [`LoginSessions::revoke`] / [`revoke_all_for_principal`]：撤销并
//!   **自增 `token_epoch`**（§4.5：撤销动作本身就作废此前签发的短期
//!   令牌）；撤销原因取值与 §4.3 的 CHECK 逐字一致。
//!
//! 撤销传播的同事务组合（禁用 membership + 递增权限版本 + 撤销全部会话）
//! 在 `crate::policy`——本模块暴露 `pub(crate)` 的事务 helper 供其复用，
//! 保证「授权变更与版本递增、会话撤销同事务」（§6.3）。
//!
//! 时刻口径：所有写入的时刻列在边界处规范化为**定宽 RFC3339 UTC（毫秒）**
//! （与 `shared::clock` 的单一形状同口径）——`ck_login_window` 在 SQLite
//! 里是字符串比较，定宽是字典序 = 时序的前提。解析失败是显式错误，绝不
//! 原样落库。

use std::sync::Arc;

use rusqlite::{params, OptionalExtension, Transaction};

use datazen::shared::id::Timestamp;

use super::AdminStore;
use crate::storage::StorageError;

/// 会话凭据形态（v5 `auth_mode` CHECK 取值）。
///
/// `Native` 只保留列与 CHECK（P8 桌面团队认证的 expand 已一次给全）；
/// 本刀不启用 native 校验路径（推迟项，见模块报告）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginSessionMode {
    Cookie,
    Native,
}

impl LoginSessionMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cookie => "cookie",
            Self::Native => "native",
        }
    }

    fn from_db(value: &str) -> Result<Self, StorageError> {
        match value {
            "cookie" => Ok(Self::Cookie),
            "native" => Ok(Self::Native),
            other => Err(StorageError::Query(format!(
                "login_sessions.auth_mode has an unknown value `{other}`"
            ))),
        }
    }
}

/// 撤销原因（§4.3 `revoke_reason` CHECK 取值，与 team-server §4.5 逐字一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevokeReason {
    /// 用户登出。
    Logout,
    /// 权限撤销传播（§6.3）。
    PermissionRevoked,
    /// 管理员禁用账号/成员（§4.5）。
    Admin,
    /// 密码/凭据轮换（§4.5）。
    CredentialRotated,
}

impl RevokeReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Logout => "logout",
            Self::PermissionRevoked => "permission_revoked",
            Self::Admin => "admin",
            Self::CredentialRotated => "credential_rotated",
        }
    }

    fn from_db(value: &str) -> Result<Self, StorageError> {
        match value {
            "logout" => Ok(Self::Logout),
            "permission_revoked" => Ok(Self::PermissionRevoked),
            "admin" => Ok(Self::Admin),
            "credential_rotated" => Ok(Self::CredentialRotated),
            other => Err(StorageError::Query(format!(
                "login_sessions.revoke_reason has an unknown value `{other}`"
            ))),
        }
    }
}

/// 新会话行的输入（登录端点的产物；`session_id` 是 ≥128-bit 随机不透明
/// 值，由调用方铸造——本仓储不承担随机性）。
#[derive(Debug, Clone)]
pub struct NewLoginSession {
    pub organization_id: String,
    pub session_id: String,
    pub principal_id: String,
    /// IdP 认证时刻（§4.3：敏感操作是否需要重新认证的判据）。
    pub auth_time: String,
    pub issued_at: String,
    pub absolute_expires_at: String,
    pub idle_expires_at: String,
    /// 建立会话时的组织权限版本快照（§6.2/§6.3 撤销传播比对的基准）。
    pub permission_version: i64,
    /// 浏览器实例指纹（UA 摘要 + 客户端实例标识）。
    pub client_binding: String,
    pub mode: LoginSessionMode,
    /// native 会话必填（仅存 secret 的 HMAC 摘要）；cookie 会话必须为空。
    pub native_secret_hash: Option<String>,
}

/// 一行登录会话（全列投影）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginSessionRow {
    pub organization_id: String,
    pub session_id: String,
    pub principal_id: String,
    pub auth_time: String,
    pub issued_at: String,
    pub absolute_expires_at: String,
    pub idle_expires_at: String,
    pub reauth_required: bool,
    pub permission_version: i64,
    pub client_binding: String,
    pub token_epoch: i64,
    pub mode: LoginSessionMode,
    pub native_secret_hash: Option<String>,
    pub revoked_at: Option<String>,
    pub revoke_reason: Option<RevokeReason>,
}

/// `validate` 的拒绝原因。调用方（L2）对**所有**变体给出同一条脱敏 401
/// 文案——不向请求方说明会话是否存在/为何失效（§3.2 L2 的失败行为）。
///
/// 实现 `std::error::Error`（thiserror）：`ValidateFailure::Rejected` 的
/// `#[from]` 需要它，且错误文案只描述事实、不带会话细节（脱敏纪律）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("login session rejected")]
pub enum SessionRejection {
    /// 无此会话行。
    Unknown,
    /// 已撤销（原因供审计与日志，不进响应）。
    Revoked(RevokeReason),
    /// 超过绝对寿命。
    AbsoluteExpired,
    /// 超过空闲寿命。
    IdleExpired,
}

/// `validate` 的失败。与 [`SessionRejection`] 分立是因为两者的 HTTP
/// 映射不同（§9.2）：拒绝 → 401 脱敏；存储不可用 → 503
/// `ServiceUnavailable`——fail-closed 不等于把基础设施故障伪装成认证失败。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidateFailure {
    #[error("login session rejected: {0:?}")]
    Rejected(#[from] SessionRejection),

    #[error(transparent)]
    Storage(#[from] StorageError),
}

/// `revoke` 的处置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevokeOutcome {
    /// 本次调用完成了撤销（epoch 已自增）。
    Revoked,
    /// 行早已撤销（幂等：不重复自增 epoch）。
    AlreadyRevoked,
    /// 无此会话行。
    Unknown,
}

/// 登录会话仓储。同一 [`AdminStore`] 上构造；调用方负责先迁移。
#[derive(Clone)]
pub struct LoginSessions {
    store: Arc<AdminStore>,
}

impl LoginSessions {
    pub fn new(store: Arc<AdminStore>) -> Self {
        Self { store }
    }

    fn backend(source: impl std::fmt::Display) -> StorageError {
        StorageError::Query(format!("login_sessions: {source}"))
    }

    /// 时刻规范化：任何入参时刻先解析再以定宽 RFC3339 UTC（毫秒）重写。
    /// 拒绝歧义形状进入数据库（`ck_login_window` 是字符串比较）。
    fn canonical_timestamp(raw: &str, field: &'static str) -> Result<String, StorageError> {
        let parsed = chrono::DateTime::parse_from_rfc3339(raw)
            .map_err(|source| Self::backend(format!("{field} is not RFC3339: {source}")))?;
        Ok(parsed.with_timezone(&chrono::Utc).to_rfc3339_opts(
            chrono::SecondsFormat::Millis,
            true,
        ))
    }

    // ── create ──

    pub fn create(&self, new: NewLoginSession) -> Result<LoginSessionRow, StorageError> {
        if new.session_id.is_empty() {
            return Err(StorageError::Query(
                "login_sessions: session_id must not be blank".into(),
            ));
        }
        if new.client_binding.is_empty() {
            return Err(StorageError::Query(
                "login_sessions: client_binding must not be blank".into(),
            ));
        }
        if new.permission_version < 1 {
            return Err(StorageError::Query(
                "login_sessions: permission_version snapshot must be >= 1".into(),
            ));
        }
        let mode_has_hash = new.mode == LoginSessionMode::Native;
        if mode_has_hash != new.native_secret_hash.is_some() {
            return Err(StorageError::Query(
                "login_sessions: native_secret_hash must be set exactly for native sessions \
                 (ck_login_native)"
                    .into(),
            ));
        }

        let organization_id = new.organization_id;
        let session_id = new.session_id;
        let principal_id = new.principal_id;
        let auth_time = Self::canonical_timestamp(&new.auth_time, "auth_time")?;
        let issued_at = Self::canonical_timestamp(&new.issued_at, "issued_at")?;
        let absolute_expires_at =
            Self::canonical_timestamp(&new.absolute_expires_at, "absolute_expires_at")?;
        let idle_expires_at = Self::canonical_timestamp(&new.idle_expires_at, "idle_expires_at")?;
        if idle_expires_at.as_str() > absolute_expires_at.as_str() {
            return Err(StorageError::Query(
                "login_sessions: idle_expires_at must not exceed absolute_expires_at \
                 (ck_login_window)"
                    .into(),
            ));
        }

        let conn = self.store.lock_for_adapter()?;
        // 外键目标存在性先行检查：FK 违例的底层报错对运维不可读，这里给
        // 出精确事实（memberships 是登录映射的权威落库处，§3.10）。
        let membership: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM memberships
                 WHERE organization_id = ?1 AND principal_id = ?2",
                params![organization_id, principal_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Self::backend)?;
        if membership.is_none() {
            return Err(StorageError::Query(format!(
                "login_sessions: membership ({organization_id}, {principal_id}) does not exist; \
                 sessions require an enabled membership row"
            )));
        }
        conn.execute(
            "INSERT INTO login_sessions (
                 organization_id, session_id, principal_id, auth_time, issued_at,
                 absolute_expires_at, idle_expires_at, reauth_required, permission_version,
                 client_binding, token_epoch, auth_mode, native_secret_hash
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?9, 1, ?10, ?11)",
            params![
                organization_id,
                session_id,
                principal_id,
                auth_time,
                issued_at,
                absolute_expires_at,
                idle_expires_at,
                new.permission_version,
                new.client_binding,
                new.mode.as_str(),
                new.native_secret_hash,
            ],
        )
        .map_err(Self::backend)?;

        Ok(LoginSessionRow {
            organization_id,
            session_id,
            principal_id,
            auth_time,
            issued_at,
            absolute_expires_at,
            idle_expires_at,
            reauth_required: false,
            permission_version: new.permission_version,
            client_binding: new.client_binding,
            token_epoch: 1,
            mode: new.mode,
            native_secret_hash: new.native_secret_hash,
            revoked_at: None,
            revoke_reason: None,
        })
    }

    // ── validate ──

    /// L2 的逐请求校验 + 空闲顺延。`now` 是入站请求时刻；`idle_ttl_seconds`
    /// 是部署配置的空闲寿命（§4.3 默认 8 小时，来自配置而非协议常量）。
    ///
    /// 顺延只对**存活**会话发生：撤销/过期分支直接拒绝，不产生写副作用。
    /// 新空闲窗 = min(now + ttl, absolute_expires_at)。
    pub fn validate(
        &self,
        session_id: &str,
        now: Timestamp,
        idle_ttl_seconds: u64,
    ) -> Result<LoginSessionRow, ValidateFailure> {
        let now_canonical =
            Self::canonical_timestamp(now.as_str(), "now").map_err(ValidateFailure::Storage)?;
        let now_dt = parse_utc(&now_canonical).map_err(ValidateFailure::Storage)?;

        let conn = self.store.lock_for_adapter().map_err(ValidateFailure::Storage)?;

        let row = self
            .read_row(&conn, session_id)
            .map_err(ValidateFailure::Storage)?
            .ok_or(SessionRejection::Unknown)?;

        if let Some(reason) = row.revoke_reason {
            return Err(SessionRejection::Revoked(reason).into());
        }
        let absolute = parse_utc(&row.absolute_expires_at).map_err(ValidateFailure::Storage)?;
        if now_dt >= absolute {
            return Err(SessionRejection::AbsoluteExpired.into());
        }
        let idle = parse_utc(&row.idle_expires_at).map_err(ValidateFailure::Storage)?;
        if now_dt >= idle {
            return Err(SessionRejection::IdleExpired.into());
        }

        // touch：顺延空闲窗，封顶于绝对窗（ck_login_window）。
        let candidate = now_dt + chrono::Duration::seconds(idle_ttl_seconds as i64);
        let new_idle = candidate.min(absolute);
        let new_idle = new_idle.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let updated = conn
            .execute(
                "UPDATE login_sessions SET idle_expires_at = ?3
                 WHERE organization_id = ?1 AND session_id = ?2 AND revoked_at IS NULL",
                params![row.organization_id, row.session_id, new_idle],
            )
            .map_err(Self::backend)?;
        debug_assert_eq!(updated, 1, "live row must accept the idle touch");

        Ok(LoginSessionRow {
            idle_expires_at: new_idle,
            ..row
        })
    }

    // ── revoke ──

    /// 撤销单个会话并自增 `token_epoch`（§4.5：撤销动作本身作废此前
    /// 签发的短期令牌）。幂等：已撤销行不重复自增。
    pub fn revoke(
        &self,
        session_id: &str,
        reason: RevokeReason,
        now: Timestamp,
    ) -> Result<RevokeOutcome, StorageError> {
        let now = Self::canonical_timestamp(now.as_str(), "now")?;
        let conn = self.store.lock_for_adapter()?;
        let changed = conn
            .execute(
                "UPDATE login_sessions
                 SET revoked_at = ?2, revoke_reason = ?3, token_epoch = token_epoch + 1
                 WHERE session_id = ?1 AND revoked_at IS NULL",
                params![session_id, now, reason.as_str()],
            )
            .map_err(Self::backend)?;
        if changed == 1 {
            return Ok(RevokeOutcome::Revoked);
        }
        let exists: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM login_sessions WHERE session_id = ?1",
                [session_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Self::backend)?;
        if exists.is_some() {
            Ok(RevokeOutcome::AlreadyRevoked)
        } else {
            Ok(RevokeOutcome::Unknown)
        }
    }

    /// 撤销某主体的**全部**存活会话（§4.5：管理员禁用账号/成员时一并
    /// 失效）。独立入口；同事务组合见 `crate::policy`。
    pub fn revoke_all_for_principal(
        &self,
        organization_id: &str,
        principal_id: &str,
        reason: RevokeReason,
        now: Timestamp,
    ) -> Result<u64, StorageError> {
        let now = Self::canonical_timestamp(now.as_str(), "now")?;
        let mut conn = self.store.lock_for_adapter()?;
        let tx = conn.transaction().map_err(Self::backend)?;
        let revoked = revoke_all_for_principal_tx(&tx, organization_id, principal_id, reason, &now)
            .map_err(Self::backend)?;
        tx.commit().map_err(Self::backend)?;
        Ok(revoked)
    }

    fn read_row(
        &self,
        conn: &rusqlite::Connection,
        session_id: &str,
    ) -> Result<Option<LoginSessionRow>, StorageError> {
        conn.query_row(
            "SELECT organization_id, session_id, principal_id, auth_time, issued_at,
                    absolute_expires_at, idle_expires_at, reauth_required, permission_version,
                    client_binding, token_epoch, auth_mode, native_secret_hash,
                    revoked_at, revoke_reason
             FROM login_sessions WHERE session_id = ?1",
            [session_id],
            row_from,
        )
        .optional()
        .map_err(Self::backend)
    }
}

/// 同事务撤销 helper：`crate::policy` 的管理用例在**同一个**事务里调用
/// 它与权限版本递增（§6.3：实际授权变更与版本递增同事务提交）。
pub(crate) fn revoke_all_for_principal_tx(
    tx: &Transaction<'_>,
    organization_id: &str,
    principal_id: &str,
    reason: RevokeReason,
    now: &str,
) -> Result<u64, rusqlite::Error> {
    let changed = tx.execute(
        "UPDATE login_sessions
         SET revoked_at = ?3, revoke_reason = ?4, token_epoch = token_epoch + 1
         WHERE organization_id = ?1 AND principal_id = ?2 AND revoked_at IS NULL",
        params![organization_id, principal_id, now, reason.as_str()],
    )?;
    Ok(changed as u64)
}

fn parse_utc(raw: &str) -> Result<chrono::DateTime<chrono::Utc>, StorageError> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|parsed| parsed.with_timezone(&chrono::Utc))
        .map_err(|source| StorageError::Query(format!("login_sessions: bad timestamp: {source}")))
}

fn row_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<LoginSessionRow> {
    let mode_raw: String = row.get(11)?;
    let revoked_at: Option<String> = row.get(13)?;
    let revoke_reason_raw: Option<String> = row.get(14)?;
    let revoke_reason = match &revoke_reason_raw {
        Some(raw) => Some(
            RevokeReason::from_db(raw)
                .map_err(|error| rusqlite::Error::FromSqlConversionFailure(
                    14,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string())),
                ))?,
        ),
        None => None,
    };
    let mode = LoginSessionMode::from_db(&mode_raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            11,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string())),
        )
    })?;
    Ok(LoginSessionRow {
        organization_id: row.get(0)?,
        session_id: row.get(1)?,
        principal_id: row.get(2)?,
        auth_time: row.get(3)?,
        issued_at: row.get(4)?,
        absolute_expires_at: row.get(5)?,
        idle_expires_at: row.get(6)?,
        reauth_required: row.get::<_, i64>(7)? != 0,
        permission_version: row.get(8)?,
        client_binding: row.get(9)?,
        token_epoch: row.get(10)?,
        mode,
        native_secret_hash: row.get(12)?,
        revoked_at,
        revoke_reason,
    })
}
