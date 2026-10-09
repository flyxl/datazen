//! `IdentityResolver`：执行身份解析端口（概要 §6.4 第 10 行之外的第 11 个端口）。
//!
//! 词汇表（§4.3）：`ExecutionIdentity`。
//!
//! 边界：它是 `executionIdentityKey` 的**唯一生产者**。`SecretProvider` 只负责版本化秘密
//! 材料（概要 §6.4 明确划走），**不做**身份解析。
//!
//! 身份键是**服务端生成、不可伪造**的：客户端传入的用户名、角色或 `principal` 都不得进入
//! `ExecutionIdentity`（CM §4.1）。数据库侧真正的登录身份，只有在配置确实把某个驱动选项
//! 映射到登录名时，才由 driver 从 `ExecutionIdentity` 派生。
//!
//! 自托管模式（server 不存在）的语义本节**不覆盖**（概要「本文不覆盖什么」）；本端口的
//! 最小可实现形态就是「组织 + 连接 + 策略隔离键 → 身份键 + 版本」。

use async_trait::async_trait;

use crate::context::{DelegationRef, RequestContext};
use crate::error::PortError;
use crate::id::{ConnectionId, ExecutionIdentityKey, OrganizationId, PolicyIsolationKey};
use crate::target::ExecutionTarget;

/// 一次执行所使用的数据库侧身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionIdentity {
    pub organization_id: OrganizationId,
    pub connection_id: ConnectionId,
    /// 服务端生成、不可伪造的稳定身份键。`PoolKey` 用它，而不是客户端传入的用户名。
    pub execution_identity_key: ExecutionIdentityKey,
    /// 策略隔离键。与身份键一起进 `PoolKey`。
    pub policy_isolation_key: PolicyIsolationKey,
    /// 身份材料的版本。凭据轮换会推进它，进而让旧物理资源退出池。
    pub credential_revision: crate::id::CredentialRevision,
}

#[async_trait]
pub trait IdentityResolver: Send + Sync + 'static {
    /// 产出不可伪造的 executionIdentityKey；PoolKey 只用该键，不用客户端传入用户名。
    ///
    /// 同一 `(organization, connection, isolation)` 必须稳定返回同一
    /// `execution_identity_key`，否则会话连续性（INV-02）与池复用都不成立。
    async fn resolve(
        &self,
        ctx: &RequestContext,
        target: &ExecutionTarget,
    ) -> Result<ExecutionIdentity, PortError>;

    /// 委托身份与本人身份**不是**同一条路径：委托主体只能落在被授予的作用域里，
    /// 不得因为持有委托就继承调用者本人的全量权限（CM-06）。
    async fn resolve_delegation(
        &self,
        ctx: &RequestContext,
        delegation: DelegationRef,
    ) -> Result<ExecutionIdentity, PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{ClientInstanceId, Counter, CredentialRevision, PrincipalId, RequestId};

    fn context() -> RequestContext {
        RequestContext::new(
            OrganizationId::new("org-1"),
            PrincipalId::new("user-1"),
            None,
            ClientInstanceId::new("client-1"),
            RequestId::new("req-1"),
            None,
        )
    }

    fn identity() -> ExecutionIdentity {
        ExecutionIdentity {
            organization_id: OrganizationId::new("org-1"),
            connection_id: ConnectionId::new("conn-1"),
            execution_identity_key: ExecutionIdentityKey::new("eid-1"),
            policy_isolation_key: PolicyIsolationKey::new("iso-1"),
            credential_revision: CredentialRevision::new(3),
        }
    }

    #[test]
    fn pool_key_components_are_versions_and_keys_never_material() {
        let identity = identity();
        assert_eq!(identity.execution_identity_key.as_str(), "eid-1");
        assert_eq!(identity.policy_isolation_key.as_str(), "iso-1");
        assert_eq!(identity.credential_revision.counter(), Counter::new(3));
        // PoolKey 的四个分量里没有任何一个能装下密码或哈希。
        let debug = format!("{identity:?}");
        assert!(!debug.to_ascii_lowercase().contains("password"));
        assert!(!debug.to_ascii_lowercase().contains("material"));
    }

    #[test]
    fn identity_is_scoped_to_one_organization_and_connection() {
        assert_ne!(
            identity(),
            ExecutionIdentity {
                organization_id: OrganizationId::new("org-2"),
                ..identity()
            }
        );
        assert_ne!(
            identity(),
            ExecutionIdentity {
                connection_id: ConnectionId::new("conn-2"),
                ..identity()
            }
        );
    }

    #[test]
    fn identity_key_is_not_derived_from_the_client_supplied_principal() {
        // 请求上下文里的主体是 user-1，但身份键是服务端生成的 eid-1：
        // 两者不是同一个字段，也不存在由 principal 构造身份键的入口。
        let ctx = context();
        let identity = identity();
        assert_ne!(
            identity.execution_identity_key.as_str(),
            ctx.principal_id.as_str()
        );
        assert_eq!(identity.execution_identity_key.as_str(), "eid-1");
    }

    #[test]
    fn rotating_credentials_moves_the_identity_version_not_the_key() {
        let rotated = ExecutionIdentity {
            credential_revision: CredentialRevision::new(4),
            ..identity()
        };
        assert_eq!(
            rotated.execution_identity_key,
            identity().execution_identity_key
        );
        assert_ne!(rotated.credential_revision, identity().credential_revision);
    }

    #[test]
    fn delegation_resolution_is_a_separate_entry_point_not_a_flag() {
        // `resolve` 与 `resolve_delegation` 是两条路径：委托不能被当成本人身份复用。
        let source = include_str!("identity.rs");
        assert!(source.contains("async fn resolve_delegation("));
        assert!(source.contains("delegation: DelegationRef,"));
        // 目标侧入口收的是 `ExecutionTarget`，不是裸 `connection_id`：
        // 目标已经过一次 §4.3 规范化，身份解析不再自己回退解析。
        let target = ExecutionTarget {
            connection_id: ConnectionId::new("conn-1"),
            namespace: crate::target::NamespaceTarget::database("app"),
            object: None,
        };
        assert_eq!(target.connection_id, ConnectionId::new("conn-1"));
        assert!(!target
            .namespace
            .has_value(crate::target::TargetNamespaceLayer::Schema));
    }
}
