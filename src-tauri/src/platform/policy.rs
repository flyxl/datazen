use super::identity::{local_principal, LOCAL_ORGANIZATION_ID};
use async_trait::async_trait;
use datazen_platform_api::{
    context::{DelegationRef, RequestContext},
    error::PortError,
    id::PolicyIsolationKey,
    ports::policy::*,
};

pub struct DesktopPolicy {
    store: std::sync::Arc<crate::store::Store>,
}
impl DesktopPolicy {
    pub fn new(store: std::sync::Arc<crate::store::Store>) -> Self {
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
            .scopes
            .iter()
            .find(|scope| {
                &scope.target == target
                    && (action == AuthorizationAction::Connect || scope.command == command)
            })
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
