//! `SubmissionTokenIssuer` 的服务端适配器：HMAC-SHA256 签名提交令牌（WR-D4）。
//!
//! [web-server-host §10.1](../../../../../docs/architecture/platform/web-server-host.md)
//! 的「签名密钥由配置/KMS 提供，不落库」：密钥原料由构造参数注入（
//! `ServerSecret` 字节；不落库、不进日志——`Debug` 手写脱敏），签名是
//! **HMAC-SHA256**（与 `csrf.rs` 同款原语）。令牌形态：
//!
//! ```text
//! base64url(payload_json) || "." || base64url(HMAC-SHA256(key, payload_json))
//! ```
//!
//! 载荷（[连接 §13.1](../../../../../docs/architecture/platform/connection-management.md)
//! 的令牌内容清单）：nonce、operation、organization/principal/clientInstance、
//! delegationId、issuedAt、expiresAt、keyVersion、运行时会话操作的 owner
//! runtimeEpoch。**不含秘密**（幂等键是不透明签名载荷，不含任何凭据材料）。
//!
//! 端口四条硬约束的落地：
//!
//! * **不信任客户端时钟**：`issuedAt` / `expiresAt` 都由服务端时钟
//!   （[`Clock`] 端口）登记；`verify` 的过期判定拿 `expiresAt` 与
//!   **服务端当前时刻**比，与客户端声称的时间无关。
//! * **未知 `keyVersion` 一律拒绝**：v1 只保留当前版本的验证密钥，rotate
//!   之后旧令牌即拒（密钥保留期 ≥ 已发令牌全部过期时间的 KMS 形态随
//!   生产化切片，已登记缺口）。
//! * `keyVersion` **单调**：rotate 只递增（构造初版 = 1）。
//! * 端口只做**签名与校验**：接受记录与墓碑由宿主持有（幂等回执存储是
//!   WR-D2/D5 的对象），本实现不存任何记录。

use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::Sha256;
use subtle::ConstantTimeEq;

use datazen::execution::application::idempotency::{IdempotentOperation, SubmissionToken};
use datazen::execution::application::submission::{
    SubmissionPresentation, SubmissionTokenIssuer, VerifiedSubmission,
};
use datazen::execution::domain::SessionHandle;
use datazen::shared::clock::Clock;
use datazen::shared::context::RequestContext;
use datazen::shared::error::PortError;
use datazen::shared::id::{
    ClientInstanceId, DelegationId, IdempotencyKey, KeyVersion, OrganizationId, PrincipalId,
    RuntimeEpoch, Timestamp,
};

type HmacSha256 = Hmac<Sha256>;

/// 令牌默认有效期（[连接 §13.1](../../../../../docs/architecture/platform/connection-management.md)
/// 的默认 24h；可调，不是硬编码——部署覆盖走 [`ServerSubmissionTokenIssuer::with_ttl`]）。
pub const DEFAULT_TOKEN_TTL_SECS: u64 = 24 * 3600;

/// 当前签名密钥状态（rotate 单调递增的版本 + 密钥原料）。
struct TokenKey {
    version: u64,
    material: Vec<u8>,
}

/// 服务端提交令牌签发器。
pub struct ServerSubmissionTokenIssuer {
    key: RwLock<TokenKey>,
    clock: Arc<dyn Clock>,
    ttl: std::time::Duration,
}

impl std::fmt::Debug for ServerSubmissionTokenIssuer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerSubmissionTokenIssuer")
            .field("key", &"[redacted]")
            .field("ttl", &self.ttl)
            .finish_non_exhaustive()
    }
}

impl ServerSubmissionTokenIssuer {
    /// 规格默认有效期（24h）。
    pub fn new(key_material: Vec<u8>, clock: Arc<dyn Clock>) -> Result<Self, PortError> {
        Self::with_ttl(
            key_material,
            clock,
            std::time::Duration::from_secs(DEFAULT_TOKEN_TTL_SECS),
        )
    }

    /// 部署配置注入有效期（[连接 §9.2](../../../../../docs/architecture/platform/connection-management.md)
    /// 的「可调初始值」口径）。空密钥原料一律拒绝（fail-closed）。
    pub fn with_ttl(
        key_material: Vec<u8>,
        clock: Arc<dyn Clock>,
        ttl: std::time::Duration,
    ) -> Result<Self, PortError> {
        if key_material.is_empty() {
            return Err(PortError::BackendUnavailable(
                "submission token key material is empty".into(),
            ));
        }
        Ok(Self {
            key: RwLock::new(TokenKey {
                version: 1,
                material: key_material,
            }),
            clock,
            ttl,
        })
    }

    /// 当前签名密钥版本（单调只增）。
    fn current_version(&self) -> u64 {
        self.key.read().map(|key| key.version).unwrap_or(0)
    }

    /// 轮换签名密钥：版本**只递增**；旧版本的验证密钥不保留（v1 单密钥，
    /// 旧令牌立即失效——「未知 keyVersion 一律拒绝」）。空原料拒绝。
    pub fn rotate_key(&self, key_material: Vec<u8>) -> Result<KeyVersion, PortError> {
        if key_material.is_empty() {
            return Err(PortError::BackendUnavailable(
                "submission token key material is empty".into(),
            ));
        }
        let mut key = self
            .key
            .write()
            .map_err(|_| PortError::BackendUnavailable("token key lock poisoned".into()))?;
        key.version = key.version.saturating_add(1);
        key.material = key_material;
        Ok(KeyVersion::new(key.version))
    }

    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, PortError> {
        let key = self
            .key
            .read()
            .map_err(|_| PortError::BackendUnavailable("token key lock poisoned".into()))?;
        let mut mac = HmacSha256::new_from_slice(&key.material)
            .map_err(|_| PortError::BackendUnavailable("token key rejected".into()))?;
        mac.update(payload);
        Ok(mac.finalize().into_bytes().to_vec())
    }

    /// 服务端时刻 + TTL → 定宽 RFC3339（毫秒）UTC 的 `expiresAt`。
    fn expires_at(&self, issued_at: &Timestamp) -> Result<Timestamp, PortError> {
        let issued = chrono::DateTime::parse_from_rfc3339(issued_at.as_str())
            .map_err(|_| {
                PortError::BackendUnavailable("clock produced an unparseable instant".into())
            })?
            .with_timezone(&chrono::Utc);
        let expires = issued + chrono::Duration::from_std(self.ttl).unwrap_or_default();
        Ok(Timestamp::new(
            expires.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        ))
    }

    fn parse_instant(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
        chrono::DateTime::parse_from_rfc3339(value)
            .ok()
            .map(|parsed| parsed.with_timezone(&chrono::Utc))
    }
}

#[async_trait]
impl SubmissionTokenIssuer for ServerSubmissionTokenIssuer {
    async fn issue(
        &self,
        ctx: &RequestContext,
        operation: IdempotentOperation,
        handle: Option<&SessionHandle>,
    ) -> Result<SubmissionToken, PortError> {
        // 运行时会话操作绑定 owner runtimeEpoch；配置类操作不绑定
        //（CreateProfile / StartJob 不带会话，见端口注释）。
        let owner_runtime_epoch = match (operation.binds_runtime_epoch(), handle) {
            (true, Some(handle)) => Some(handle.runtime_epoch.clone()),
            (true, None) => {
                return Err(PortError::TokenInvalid);
            }
            (false, _) => None,
        };
        let issued_at = self.clock.now();
        let expires_at = self.expires_at(&issued_at)?;

        use rand::RngCore;
        let mut nonce = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut nonce);

        // nonce 保证同请求同秒的两次签发产出不同令牌（幂等键唯一的前提）。
        let payload = json!({
            "v": 1,
            "op": serde_json::to_value(&operation)
                .map_err(|_| PortError::BackendUnavailable("operation serialization failed".into()))?,
            "org": ctx.organization_id.as_str(),
            "sub": ctx.principal_id.as_str(),
            "cli": ctx.client_instance_id.as_str(),
            "del": ctx.delegation_id.as_ref().map(|id| id.as_str()),
            "epoch": owner_runtime_epoch.as_ref().map(|epoch| epoch.as_str()),
            "iat": issued_at.as_str(),
            "exp": expires_at.as_str(),
            "kv": self.current_version(),
            "nonce": BASE64_URL.encode(nonce),
        });
        let payload_bytes = serde_json::to_vec(&payload).map_err(|_| {
            PortError::BackendUnavailable("token payload serialization failed".into())
        })?;
        let tag = self.sign(&payload_bytes)?;

        let token = format!(
            "{}.{}",
            BASE64_URL.encode(&payload_bytes),
            BASE64_URL.encode(tag)
        );
        Ok(SubmissionToken {
            idempotency_key: IdempotencyKey::new(token),
            expires_at,
        })
    }

    async fn verify(
        &self,
        presented: &SubmissionPresentation,
    ) -> Result<VerifiedSubmission, PortError> {
        let token = presented.token.idempotency_key.as_str();
        let (payload_b64, tag_b64) = token.split_once('.').ok_or(PortError::TokenInvalid)?;
        let payload_bytes = BASE64_URL
            .decode(payload_b64)
            .map_err(|_| PortError::TokenInvalid)?;
        let tag = BASE64_URL
            .decode(tag_b64)
            .map_err(|_| PortError::TokenInvalid)?;

        // 签名校验：常量时间比较（与 csrf.rs 同款纪律）。
        let expected = self.sign(&payload_bytes)?;
        if !bool::from(expected.as_slice().ct_eq(&tag)) {
            return Err(PortError::TokenInvalid);
        }

        let payload: serde_json::Value =
            serde_json::from_slice(&payload_bytes).map_err(|_| PortError::TokenInvalid)?;

        // 未知 keyVersion 一律拒绝（v1 只保留当前版本）。
        let key_version = payload
            .get("kv")
            .and_then(|value| value.as_u64())
            .ok_or(PortError::TokenInvalid)?;
        if key_version != self.current_version() {
            return Err(PortError::TokenInvalid);
        }

        // 签名载荷是权威；声称值只做交叉核对，不一致即拒绝。
        let operation: IdempotentOperation =
            serde_json::from_value(payload.get("op").cloned().ok_or(PortError::TokenInvalid)?)
                .map_err(|_| PortError::TokenInvalid)?;
        let organization_id = OrganizationId::new(
            payload
                .get("org")
                .and_then(|value| value.as_str())
                .ok_or(PortError::TokenInvalid)?,
        );
        let principal_id = PrincipalId::new(
            payload
                .get("sub")
                .and_then(|value| value.as_str())
                .ok_or(PortError::TokenInvalid)?,
        );
        let client_instance_id = ClientInstanceId::new(
            payload
                .get("cli")
                .and_then(|value| value.as_str())
                .ok_or(PortError::TokenInvalid)?,
        );
        let delegation_id = payload
            .get("del")
            .and_then(|value| value.as_str())
            .map(DelegationId::new);
        if organization_id != presented.claimed_organization_id
            || principal_id != presented.claimed_principal_id
            || client_instance_id != presented.claimed_client_instance_id
            || delegation_id != presented.claimed_delegation_id
        {
            return Err(PortError::TokenInvalid);
        }

        // 过期判定以服务端登记的时刻为基准（不信任客户端时钟）。
        let issued_at = Timestamp::new(
            payload
                .get("iat")
                .and_then(|value| value.as_str())
                .ok_or(PortError::TokenInvalid)?,
        );
        let expires_at = Timestamp::new(
            payload
                .get("exp")
                .and_then(|value| value.as_str())
                .ok_or(PortError::TokenInvalid)?,
        );
        let now = Self::parse_instant(self.clock.now().as_str()).ok_or(PortError::TokenInvalid)?;
        let expires = Self::parse_instant(expires_at.as_str()).ok_or(PortError::TokenInvalid)?;
        if expires <= now {
            return Err(PortError::TokenInvalid);
        }

        let owner_runtime_epoch = payload
            .get("epoch")
            .and_then(|value| value.as_str())
            .map(RuntimeEpoch::new);
        // 操作与会话绑定的一致性：绑定操作必须有 epoch，配置操作必须没有。
        if operation.binds_runtime_epoch() != owner_runtime_epoch.is_some() {
            return Err(PortError::TokenInvalid);
        }

        Ok(VerifiedSubmission {
            operation,
            organization_id,
            principal_id,
            client_instance_id,
            idempotency_key: presented.token.idempotency_key.clone(),
            issued_at,
            expires_at,
            owner_runtime_epoch,
            key_version: KeyVersion::new(key_version),
        })
    }

    fn key_version(&self) -> Result<KeyVersion, PortError> {
        Ok(KeyVersion::new(self.current_version()))
    }
}
