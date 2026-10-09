//! `PolicyService`：鉴权与委托校验端口。
//!
//! 词汇表（§4.3）：`AuthorizationSubject`、`AuthorizationAction`、
//! `AuthorizationDecision`、`DelegationRef`、`DelegationGrant`、`PolicyIsolationKey`、
//! `PolicyChangeStream`。
//!
//! 三条硬约束：
//!
//! * 委托校验**最远到根委托**；被委托主体、作用域与有效期三者均需匹配，
//!   失败**不得降级为调用者权限**（CM-06）。
//! * `isolation_key` 进 `PoolKey`，因此权限撤销必须让键**立即变化**；
//!   它是隔离键，**不是密码散列**。
//! * 版本变更走**订阅**而不是轮询，订阅者据此淘汰缓存键。

use async_trait::async_trait;

use crate::context::{DelegationRef, RequestContext};
use crate::error::PortError;
use crate::id::{Counter, PolicyIsolationKey};

/// 主体。委托链最多一层：只有「本人」与「某人的委托」，不接受更深的嵌套。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizationSubject {
    Principal {
        principal_id: crate::id::PrincipalId,
    },
    Delegated(DelegationRef),
}

/// 动作。集合固定，新增需要同步更新策略与前端动作表。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizationAction {
    Connect,
    Query,
    Write,
    Administer,
    ViewCredential,
    ExportArtifact,
    ManageJob,
}

/// 判定结果。拒绝方必须给出 `reason_code` 供上层归类（不是自由文本）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizationDecision {
    Allow,
    Deny { reason_code: &'static str },
}

impl AuthorizationDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow)
    }
}

/// 委托的授予事实。`delegation.expires_at` 必填：永久委托是不允许的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegationGrant {
    pub delegation: DelegationRef,
    pub actions: Vec<AuthorizationAction>,
}

/// 策略版本变更通知。policy 变更必须**按受影响会话通知**，不能全量广播。
#[async_trait]
pub trait PolicyService: Send + Sync + 'static {
    async fn authorize(
        &self,
        ctx: &RequestContext,
        subject: AuthorizationSubject,
        action: AuthorizationAction,
    ) -> Result<AuthorizationDecision, PortError>;

    /// 委托校验：被委托主体、作用域与有效期三者均需匹配，失败不得降级为调用者权限（CM-06）。
    async fn verify_delegation(
        &self,
        ctx: &RequestContext,
        delegation: DelegationRef,
    ) -> Result<DelegationGrant, PortError>;

    /// 参与 PoolKey 的策略隔离键；权限撤销必须使键立即变化。
    fn isolation_key(&self, ctx: &RequestContext) -> Result<PolicyIsolationKey, PortError>;

    /// 权限版本变更通知：订阅者据此淘汰缓存键，不得轮询。
    fn subscribe_version_changes(&self) -> PolicyChangeStream;
}

/// 策略变更流。
pub type PolicyChangeStream = Box<dyn Iterator<Item = PolicyVersionChange> + Send + 'static>;

/// 一次策略版本变更。`isolation_key` 让受影响方能只处理与自己相关的那部分。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyVersionChange {
    pub version: Counter,
    /// 受影响的隔离域。策略隔离键进 `PoolKey`，不含密码原文。
    pub isolation_key: PolicyIsolationKey,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{
        ClientInstanceId, DelegationId, OrganizationId, PrincipalId, RequestId, Timestamp,
    };
    use crate::ports::secret::SecretPurpose;

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

    fn delegation(id: &str) -> DelegationRef {
        DelegationRef::new(
            DelegationId::new(id),
            OrganizationId::new("org-1"),
            PrincipalId::new("user-2"),
            ClientInstanceId::new("client-1"),
            "connections:read".into(),
            Timestamp::new("2026-01-02T00:00:00Z"),
        )
    }

    #[test]
    fn only_allow_is_allowed() {
        assert!(AuthorizationDecision::Allow.is_allowed());
        assert!(!AuthorizationDecision::Deny {
            reason_code: "policy"
        }
        .is_allowed());
    }

    #[test]
    fn delegated_subject_carries_the_full_delegation_reference() {
        let subject = AuthorizationSubject::Delegated(delegation("del-1"));
        assert_eq!(
            subject,
            AuthorizationSubject::Delegated(delegation("del-1"))
        );
        assert_ne!(
            subject,
            AuthorizationSubject::Delegated(delegation("del-2"))
        );
    }

    #[test]
    fn grant_carries_the_delegation_with_its_mandatory_expiry() {
        let grant = DelegationGrant {
            delegation: delegation("del-1"),
            actions: vec![AuthorizationAction::Query, AuthorizationAction::Connect],
        };
        assert!(
            !grant.delegation.expires_at.is_empty(),
            "委托必须有到期时间"
        );
        assert_eq!(grant.actions.len(), 2);
        // 授予的动作是子集判定，不含隐式提权。
        assert!(!grant.actions.contains(&AuthorizationAction::Administer));
    }

    #[test]
    fn policy_change_carries_an_isolation_key_not_credentials() {
        let change = PolicyVersionChange {
            version: Counter::new(4),
            isolation_key: PolicyIsolationKey::new("policy-iso-1"),
        };
        // 隔离键是 opaque ID：Debug 输出刻意脱敏，日志里不会留下策略分区标识。
        let debug = format!("{change:?}");
        assert!(
            !debug.contains("policy-iso-1"),
            "opaque id 必须脱敏：{debug}"
        );
        assert!(
            debug.contains("version: Counter(4)"),
            "版本号是可观测事实，应保留：{debug}"
        );
        assert!(!debug.to_ascii_lowercase().contains("password"));
        // 但值本身仍可按需取出，否则无法作为缓存分区键使用。
        assert_eq!(change.isolation_key.as_str(), "policy-iso-1");
    }

    #[test]
    fn authorization_subject_is_built_from_adapter_context_identity() {
        let ctx = context();
        let subject = AuthorizationSubject::Principal {
            principal_id: ctx.principal_id.clone(),
        };
        assert_eq!(
            subject,
            AuthorizationSubject::Principal {
                principal_id: PrincipalId::new("user-1")
            }
        );
        // 上下文里的委托 id 与主体里的委托引用是两处事实，交叉核对由用例层做。
        assert!(!context().is_delegated());
        let delegated_ctx = RequestContext::new(
            OrganizationId::new("org-1"),
            PrincipalId::new("user-2"),
            None,
            ClientInstanceId::new("client-1"),
            RequestId::new("req-2"),
            Some(DelegationId::new("del-1")),
        );
        assert!(delegated_ctx.is_delegated());
    }

    #[test]
    fn the_version_stream_is_an_iterator_of_isolation_scoped_changes() {
        let stream: PolicyChangeStream = Box::new(
            vec![
                PolicyVersionChange {
                    version: Counter::new(4),
                    isolation_key: PolicyIsolationKey::new("iso-a"),
                },
                PolicyVersionChange {
                    version: Counter::new(5),
                    isolation_key: PolicyIsolationKey::new("iso-b"),
                },
            ]
            .into_iter(),
        );
        let keys: Vec<String> = stream
            .map(|change| change.isolation_key.as_str().to_string())
            .collect();
        assert_eq!(keys, vec!["iso-a".to_string(), "iso-b".to_string()]);
    }

    #[test]
    fn secret_purpose_is_reachable_without_exposing_material() {
        // `ViewCredential` 是动作（判定用），`SecretPurpose` 是材料用途（读取用），
        // 两者不可互相替代。
        assert_ne!(
            SecretPurpose::DatabasePassword,
            SecretPurpose::SshPrivateKey
        );
        assert_eq!(
            AuthorizationAction::ViewCredential,
            AuthorizationAction::ViewCredential
        );
    }
}
