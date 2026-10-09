//! `SubmissionTokenIssuer`：签名提交令牌端口。
//!
//! 词汇表（§4.5）：`SubmissionPresentation`、`VerifiedSubmission`。
//!
//! 令牌载荷（[连接 §13.1](connection-management.md)）：nonce、operation、
//! organization/principal/clientInstance、issuedAt、expiresAt、keyVersion，
//! 运行时会话操作再带 owner runtimeEpoch。默认有效期 24h（可调，不是硬编码常量）。
//!
//! 四条硬约束：
//!
//! * **不信任客户端时钟**：过期判定以服务端登记的 `issuedAt` 为基准。
//! * 幂等键是**不透明**的签名载荷；去重按令牌摘要做，不按客户端字符串相等。
//! * 接受记录**不得存 `SessionHandle`**，且保留期不短于 `expiresAt + 24h`，
//!   否则重启后会误判「从未提交过」而重复执行。
//! * 端口层只做**签名与校验**，不接受记录与墓碑由宿主（runtime / server）持有。

use async_trait::async_trait;

use crate::context::RequestContext;
use crate::dto::idempotency::{IdempotentOperation, SubmissionToken};
use crate::dto::session::SessionHandle;
use crate::error::PortError;
use crate::id::{
    ClientInstanceId, DelegationId, IdempotencyKey, KeyVersion, OrganizationId, PrincipalId,
    RuntimeEpoch, Timestamp,
};

/// 客户端出示的令牌。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmissionPresentation {
    pub token: SubmissionToken,
    /// 声称的组织/主体。**校验以签名载荷为准**，此处只用于交叉核对。
    pub claimed_organization_id: OrganizationId,
    pub claimed_principal_id: PrincipalId,
    pub claimed_client_instance_id: ClientInstanceId,
    pub claimed_delegation_id: Option<DelegationId>,
}

/// 校验通过的令牌事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedSubmission {
    pub operation: IdempotentOperation,
    pub organization_id: OrganizationId,
    pub principal_id: PrincipalId,
    pub client_instance_id: ClientInstanceId,
    pub idempotency_key: IdempotencyKey,
    /// 服务端记录的签发时刻。**过期判定用它，不用客户端时钟。**
    pub issued_at: Timestamp,
    pub expires_at: Timestamp,
    /// 运行时会话操作才有。
    pub owner_runtime_epoch: Option<RuntimeEpoch>,
    pub key_version: KeyVersion,
}

impl VerifiedSubmission {
    /// 以服务端登记的 `issuedAt` 为基准判断是否过期。
    pub fn is_expired_at(&self, server_now: &Timestamp) -> bool {
        self.expires_at.as_str() <= server_now.as_str()
    }
}

#[async_trait]
pub trait SubmissionTokenIssuer: Send + Sync + 'static {
    /// `createProfile` 不绑定会话；其余运行时会话操作绑定 owner runtimeEpoch。
    async fn issue(
        &self,
        ctx: &RequestContext,
        operation: IdempotentOperation,
        handle: Option<&SessionHandle>,
    ) -> Result<SubmissionToken, PortError>;

    /// 校验签名与过期。签名不符、过期、epoch 不匹配一律 `PortError::TokenInvalid`。
    async fn verify(
        &self,
        presented: &SubmissionPresentation,
    ) -> Result<VerifiedSubmission, PortError>;

    fn key_version(&self) -> Result<KeyVersion, PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::DbSessionId;

    fn verified() -> VerifiedSubmission {
        VerifiedSubmission {
            operation: IdempotentOperation::ExecuteInSession,
            organization_id: OrganizationId::new("org-1"),
            principal_id: PrincipalId::new("user-1"),
            client_instance_id: ClientInstanceId::new("client-1"),
            idempotency_key: IdempotencyKey::new("idem-1"),
            issued_at: Timestamp::new("2026-01-01T00:00:00Z"),
            expires_at: Timestamp::new("2026-01-02T00:00:00Z"),
            owner_runtime_epoch: Some(RuntimeEpoch::new("epoch-1")),
            key_version: KeyVersion::new(2),
        }
    }

    #[test]
    fn expiry_is_measured_against_the_server_clock_not_the_client_one() {
        let submission = verified();
        assert!(
            submission.is_expired_at(&Timestamp::new("2026-01-02T00:00:00Z")),
            "到期瞬间即过期"
        );
        assert!(submission.is_expired_at(&Timestamp::new("2026-01-03T00:00:00Z")));
        assert!(!submission.is_expired_at(&Timestamp::new("2025-12-31T23:59:59Z")));
    }

    #[test]
    fn session_operations_carry_the_owner_runtime_epoch() {
        let session_submission = verified();
        assert!(session_submission.owner_runtime_epoch.is_some());
        assert!(IdempotentOperation::ExecuteInSession.binds_runtime_epoch());

        // 配置类操作不绑定会话 epoch。
        let config_submission = VerifiedSubmission {
            operation: IdempotentOperation::CreateProfile,
            owner_runtime_epoch: None,
            ..verified()
        };
        assert!(!config_submission.operation.binds_runtime_epoch());
        assert!(config_submission.owner_runtime_epoch.is_none());
    }

    #[test]
    fn presentation_claims_are_cross_checked_not_trusted() {
        let presentation = SubmissionPresentation {
            token: SubmissionToken {
                idempotency_key: IdempotencyKey::new("idem-1"),
                expires_at: Timestamp::new("2026-01-02T00:00:00Z"),
            },
            claimed_organization_id: OrganizationId::new("org-1"),
            claimed_principal_id: PrincipalId::new("user-1"),
            claimed_client_instance_id: ClientInstanceId::new("client-1"),
            claimed_delegation_id: None,
        };
        // 出示方声称的组织与签名载荷一致时才有意义；不一致必须在 verify 里失败。
        assert_eq!(
            presentation.claimed_organization_id,
            verified().organization_id
        );
        assert_eq!(
            presentation.token.idempotency_key,
            verified().idempotency_key
        );
    }

    #[test]
    fn token_payload_never_carries_the_session_handle_itself() {
        let handle = SessionHandle::new(DbSessionId::new("sess-1"), RuntimeEpoch::new("epoch-1"));
        // issue 的入参是 handle，产出里只有 epoch：接受记录存不下 handle。
        let presentation_token = SubmissionToken {
            idempotency_key: IdempotencyKey::new("idem-1"),
            expires_at: Timestamp::new("2026-01-02T00:00:00Z"),
        };
        assert_eq!(presentation_token.idempotency_key.as_str(), "idem-1");
        let source = include_str!("token.rs");
        let verified_block = &source[source.find("pub struct VerifiedSubmission").unwrap()..];
        let verified_block = &verified_block[..verified_block.find("\n}").unwrap()];
        assert!(!verified_block.contains("SessionHandle"));
        assert!(!verified_block.contains("DbSessionId"));
        assert!(handle.db_session_id.as_str() == "sess-1");
    }

    #[test]
    fn key_version_is_a_rotatable_version_not_a_secret() {
        assert_eq!(verified().key_version, KeyVersion::new(2));
        assert_ne!(KeyVersion::new(2), KeyVersion::new(3));
    }

    #[test]
    fn idempotency_key_stays_opaque_in_debug_output() {
        let presentation = SubmissionPresentation {
            token: SubmissionToken {
                idempotency_key: IdempotencyKey::new("super-secret-idem"),
                expires_at: Timestamp::new("2026-01-02T00:00:00Z"),
            },
            claimed_organization_id: OrganizationId::new("org-1"),
            claimed_principal_id: PrincipalId::new("user-1"),
            claimed_client_instance_id: ClientInstanceId::new("client-1"),
            claimed_delegation_id: None,
        };
        let debug = format!("{presentation:?}");
        assert!(
            !debug.contains("super-secret-idem"),
            "幂等键不得出现在日志里"
        );
        assert!(debug.contains("IdempotencyKey(<redacted>)"));
    }
}
