use super::identity::{local_principal, LOCAL_ORGANIZATION_ID};
use async_trait::async_trait;
use datazen_platform_api::{
    context::{DelegationRef, RequestContext},
    error::PortError,
    id::PolicyIsolationKey,
    ports::policy::*,
};

pub struct DesktopPolicy {
    store: std::sync::Arc<crate::store::DesktopRepository>,
}
impl DesktopPolicy {
    pub fn new(store: std::sync::Arc<crate::store::DesktopRepository>) -> Self {
        Self { store }
    }
    fn grant(
        &self,
        ctx: &RequestContext,
    ) -> Result<datazen_feature_core::dashboard::schedule_grant::MonitorGrant, PortError> {
        crate::monitor::grants::current(&self.store, ctx).map_err(|_| PortError::TokenInvalid)
    }
}
#[async_trait]
impl PolicyService for DesktopPolicy {
    async fn authorize(
        &self,
        ctx: &RequestContext,
        subject: AuthorizationSubject,
        _action: AuthorizationAction,
    ) -> Result<AuthorizationDecision, PortError> {
        if ctx.is_delegated() {
            let grant = self.grant(ctx)?;
            return Ok(
                if subject == AuthorizationSubject::Delegated(grant.delegation())
                    && matches!(
                        _action,
                        AuthorizationAction::Connect | AuthorizationAction::Query
                    )
                {
                    AuthorizationDecision::Allow
                } else {
                    AuthorizationDecision::Deny {
                        reason_code: "monitor_scope_denied",
                    }
                },
            );
        }
        let local = ctx.organization_id.as_str() == LOCAL_ORGANIZATION_ID
            && Some(ctx.principal_id.clone()) == local_principal();
        Ok(match subject {
            AuthorizationSubject::Principal { principal_id }
                if local && principal_id == ctx.principal_id && ctx.delegation_id.is_none() =>
            {
                AuthorizationDecision::Allow
            }
            _ => AuthorizationDecision::Deny {
                reason_code: "desktop_identity_required",
            },
        })
    }
    async fn verify_delegation(
        &self,
        _ctx: &RequestContext,
        _delegation: DelegationRef,
    ) -> Result<DelegationGrant, PortError> {
        let grant = self.resolve_delegation(_ctx).await?;
        if grant.delegation != _delegation {
            return Err(PortError::TokenInvalid);
        }
        Ok(grant)
    }
    async fn resolve_delegation(&self, ctx: &RequestContext) -> Result<DelegationGrant, PortError> {
        Ok(DelegationGrant {
            delegation: self.grant(ctx)?.delegation(),
            actions: vec![AuthorizationAction::Connect, AuthorizationAction::Query],
        })
    }
    async fn authorize_target(
        &self,
        ctx: &RequestContext,
        subject: AuthorizationSubject,
        action: AuthorizationAction,
        target: &datazen_platform_api::target::ExecutionTarget,
        command: &str,
        input: &serde_json::Value,
    ) -> Result<AuthorizationDecision, PortError> {
        if !ctx.is_delegated() {
            return self.authorize(ctx, subject, action).await;
        }
        let grant = self.grant(ctx)?;
        let scope = grant
            .scope(
                target,
                (action != AuthorizationAction::Connect).then_some(command),
            )
            .ok_or(PortError::TokenInvalid)?;
        let (config, metadata) = self
            .store
            .platform_profile(target.connection_id.as_str())
            .await
            .ok_or(PortError::TokenInvalid)?;
        if !metadata.enabled
            || metadata.config_revision != scope.config_revision
            || !matches!(config.database_type.as_str(), "postgresql" | "mysql")
        {
            return Err(PortError::TokenInvalid);
        }
        if action != AuthorizationAction::Connect {
            let sql = input["sql"].as_str().ok_or(PortError::TokenInvalid)?;
            datazen_feature_core::dashboard::schedule_grant::validate_read_only_sql(sql)
                .map_err(|_| PortError::TokenInvalid)?;
        }
        self.authorize(ctx, subject, action).await
    }
    fn isolation_key(&self, ctx: &RequestContext) -> Result<PolicyIsolationKey, PortError> {
        if ctx.is_delegated() {
            let grant = self.grant(ctx)?;
            return Ok(PolicyIsolationKey::new(format!(
                "monitor:{}:{}",
                grant.id, grant.revocation_version
            )));
        }
        if ctx.organization_id.as_str() != LOCAL_ORGANIZATION_ID
            || Some(ctx.principal_id.clone()) != local_principal()
            || ctx.delegation_id.is_some()
        {
            return Err(PortError::TokenInvalid);
        }
        Ok(PolicyIsolationKey::new(format!(
            "desktop:{}:{}",
            ctx.organization_id.as_str(),
            ctx.principal_id.as_str()
        )))
    }
    fn subscribe_version_changes(&self) -> PolicyChangeStream {
        Box::new(std::iter::empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_platform_api::id::{ClientInstanceId, OrganizationId, PrincipalId, RequestId};

    /// 桌面策略只放行**本机桌面身份**。这三条用例把这条契约钉住：
    ///
    /// - 本机主体 ⇒ 放行；
    /// - **主体不匹配 ⇒ 必须被拒**（这是本文件最要紧的一条：桌面主体改成「进程内
    ///   只解析一次」之后，这条校验一点没放松——它比的是「请求里的主体」与「本进程的
    ///   桌面主体」，不是一个被放行后就不再核对的快照）；
    /// - 组织不匹配 ⇒ 必须被拒。
    ///
    /// 主体取 [`local_principal`]（进程内解析一次的桌面主体），因此用例不依赖
    /// `$USER` 是什么，也不受测试进程里其它用例的调度影响。
    async fn desktop_policy() -> DesktopPolicy {
        let temp = tempfile::tempdir().expect("临时目录");
        let store = std::sync::Arc::new(
            crate::store::DesktopRepository::init_with_path(temp.path())
                .await
                .expect("store 初始化"),
        );
        DesktopPolicy::new(store)
    }

    fn context(organization: &str, principal: &str) -> RequestContext {
        RequestContext::new(
            OrganizationId::new(organization),
            PrincipalId::new(principal),
            None,
            ClientInstanceId::new("client-1"),
            RequestId::new("req-1"),
            None,
        )
    }

    fn subject(principal: &str) -> AuthorizationSubject {
        AuthorizationSubject::Principal {
            principal_id: PrincipalId::new(principal),
        }
    }

    #[tokio::test]
    async fn 本机桌面主体放行() {
        let policy = desktop_policy().await;
        let principal = local_principal().expect("桌面主体应当存在");
        let decision = policy
            .authorize(
                &context(LOCAL_ORGANIZATION_ID, principal.as_str()),
                subject(principal.as_str()),
                AuthorizationAction::Connect,
            )
            .await
            .expect("授权判定");
        assert!(decision.is_allowed(), "本机桌面主体必须放行");
    }

    /// **主体不匹配 ⇒ 必须被拒。**
    ///
    /// 这条是本文件最承重的一条：桌面主体改成「进程内只解析一次」之后，
    /// 「请求里的主体」与「本进程的桌面主体」仍然逐次比对，不匹配就没有理由放行。
    #[tokio::test]
    async fn 主体不匹配必须被拒() {
        let policy = desktop_policy().await;
        let principal = local_principal().expect("桌面主体应当存在");
        let impostor = format!("{}-impostor", principal.as_str());
        let decision = policy
            .authorize(
                &context(LOCAL_ORGANIZATION_ID, &impostor),
                subject(&impostor),
                AuthorizationAction::Connect,
            )
            .await
            .expect("授权判定");
        assert!(
            !decision.is_allowed(),
            "主体不是本机桌面主体时必须被拒：{decision:?}"
        );
        match decision {
            AuthorizationDecision::Deny { reason_code } => assert_eq!(
                reason_code, "desktop_identity_required",
                "拒绝理由必须是「需要桌面身份」"
            ),
            AuthorizationDecision::Allow => panic!("主体不匹配时不得放行"),
        }
    }

    /// **组织不匹配 ⇒ 必须被拒。**
    #[tokio::test]
    async fn 组织不匹配必须被拒() {
        let policy = desktop_policy().await;
        let principal = local_principal().expect("桌面主体应当存在");
        let decision = policy
            .authorize(
                &context("org-foreign", principal.as_str()),
                subject(principal.as_str()),
                AuthorizationAction::Connect,
            )
            .await
            .expect("授权判定");
        assert!(
            !decision.is_allowed(),
            "组织不是本地组织时必须被拒：{decision:?}"
        );
    }
}
