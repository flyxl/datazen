//! The organization scope of the Data Transfer plan registry.
//!
//! The desktop is a **single-tenant** shape: one app data directory is one
//! organization, and its id is the process constant
//! [`LOCAL_ORGANIZATION_ID`](crate::platform::LOCAL_ORGANIZATION_ID). The scope
//! is therefore a property of the *shape*, not of the request: it is derived
//! from the desktop identity the host assembles, never from a request body
//! field (INV-01).
//!
//! The scope is what the storage port checks on every access. A plan that
//! belongs to another organization is answered as **not found**, so a caller
//! can never learn that it exists (CM-05).

use datazen_platform_api::context::RequestContext;
use datazen_platform_api::id::{ClientInstanceId, OrganizationId, PrincipalId, RequestId};
use datazen_platform_api::ports::scope::OrganizationScope;

use crate::platform::LOCAL_ORGANIZATION_ID;

/// The local desktop principal of the plan registry's request context.
///
/// The plan registry stores **plans**, not user data: the principal is only
/// part of the request context the scope is derived from, and the registry
/// never uses it for a decision.
const LOCAL_DESKTOP_PRINCIPAL: &str = "datazen-local-user";

/// The local desktop client instance of the plan registry's request context.
const LOCAL_DESKTOP_CLIENT_INSTANCE: &str = "datazen-local-client";

/// The request context the plan registry's scope is derived from.
pub(crate) fn desktop_request_context() -> RequestContext {
    RequestContext::new(
        OrganizationId::new(LOCAL_ORGANIZATION_ID),
        PrincipalId::new(LOCAL_DESKTOP_PRINCIPAL),
        None,
        ClientInstanceId::new(LOCAL_DESKTOP_CLIENT_INSTANCE),
        RequestId::new(format!("transfer-plan-{}", uuid::Uuid::new_v4())),
        None,
    )
}

/// The organization scope of the plan registry.
pub(crate) fn desktop_organization_scope() -> OrganizationScope {
    OrganizationScope::from_context(&desktop_request_context())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_desktop_scope_names_the_local_organization() {
        let scope = desktop_organization_scope();
        assert_eq!(scope.organization_id().as_str(), LOCAL_ORGANIZATION_ID);
    }

    #[test]
    fn the_scope_is_assembled_not_taken_from_a_request_field() {
        // The scope carries the organization and nothing else: no principal
        // and no credential may leak into a storage key.
        let debug = format!("{:?}", desktop_organization_scope());
        assert!(!debug.to_ascii_lowercase().contains("password"));
        assert!(!debug.to_ascii_lowercase().contains("principal"));
    }
}
