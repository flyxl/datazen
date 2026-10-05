use super::identity::{local_principal, LOCAL_ORGANIZATION_ID};
use async_trait::async_trait;
use datazen_platform_api::{
    context::{DelegationRef, RequestContext},
    error::PortError,
    id::PolicyIsolationKey,
    ports::policy::*,
};

pub struct DesktopPolicy;
#[async_trait]
impl PolicyService for DesktopPolicy {
    async fn authorize(
        &self,
        ctx: &RequestContext,
        subject: AuthorizationSubject,
        _action: AuthorizationAction,
    ) -> Result<AuthorizationDecision, PortError> {
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
        Err(PortError::TokenInvalid)
    }
    fn isolation_key(&self, ctx: &RequestContext) -> Result<PolicyIsolationKey, PortError> {
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
