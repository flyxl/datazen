//! The **desktop's own** organization scope, as a *test fixture*.
//!
//! # What changed and why
//!
//! This module used to be the production source of the Data Transfer plan
//! registry's scope: `desktop_organization_scope()` returned the desktop's
//! install-level organization constant ([`LOCAL_ORGANIZATION_ID`]), and every
//! plan access went through it.
//!
//! That is only correct while the desktop is the *only* shape. The plan
//! registry is an **organization-partitioned storage port**
//! ([`TransferPlanRepositoryPort`]: every method takes an `&OrganizationScope`), so
//! the scope the caller passes **is** the tenant boundary. Pinning it to the
//! desktop's install-level constant means a server shape would look up
//! *"the desktop's organization"* inside a multi-tenant store — the wrong
//! tenant, i.e. a cross-organization read that CM-05 requires to be treated as
//! "does not exist".
//!
//! Production code therefore takes the scope from the **host**
//! (`AppState::organization_scope()`), which is the shape's regular entry
//! point: on the desktop it still resolves to the local organization, and in a
//! server shape it resolves to the authenticated session's organization.
//!
//! What is left here is the **test fixture** for "the desktop's own
//! organization", plus the two invariants that used to be attached to it:
//!
//! * the scope is **assembled**, never taken from a request field (INV-01);
//! * the scope carries **only** the organization — no principal, no
//!   credential — so it can be used as a storage key.
//!
//! [`TransferPlanRepositoryPort`]: crate::data_transfer::plan_store::TransferPlanRepositoryPort

use std::sync::OnceLock;

use datazen_modules::shared::context::RequestContext;
use datazen_modules::shared::id::{ClientInstanceId, OrganizationId, PrincipalId, RequestId};
use datazen_modules::shared::scope::OrganizationScope;

use crate::platform::LOCAL_ORGANIZATION_ID;

/// The local desktop principal of the plan registry's request context.
///
/// The plan registry stores **plans**, not user data: the principal is only
/// part of the request context the scope is derived from, and the registry
/// never uses it for a decision.
const LOCAL_DESKTOP_PRINCIPAL: &str = "datazen-local-user";

/// The local desktop client instance of the plan registry's request context.
const LOCAL_DESKTOP_CLIENT_INSTANCE: &str = "datazen-local-client";

/// The local request id of the plan registry's request context.
///
/// **This id carries no meaning here.** The scope only ever names the
/// organization; the request id is not an authorization input, is not written
/// anywhere, and is never compared. It therefore gets a fixed local value
/// instead of a fresh UUID per access — the plan registry is read on every
/// plan access, and a UUID per read would be an allocation for nothing.
const LOCAL_DESKTOP_REQUEST_ID: &str = "transfer-plan-desktop";

/// The one request context the plan registry's scope is derived from.
///
/// The desktop is single-tenant, so the context is assembled **once** and
/// reused for the lifetime of the process.
fn desktop_context() -> &'static RequestContext {
    static CONTEXT: OnceLock<RequestContext> = OnceLock::new();
    CONTEXT.get_or_init(|| {
        RequestContext::new(
            OrganizationId::new(LOCAL_ORGANIZATION_ID),
            PrincipalId::new(LOCAL_DESKTOP_PRINCIPAL),
            None,
            ClientInstanceId::new(LOCAL_DESKTOP_CLIENT_INSTANCE),
            RequestId::new(LOCAL_DESKTOP_REQUEST_ID),
            None,
        )
    })
}

/// The organization scope of the desktop's own plan registry.
///
/// **Test fixture only.** Production code must take the scope from the host
/// (`AppState::organization_scope()`) — see the module docs for why.
///
/// Returned as a shared reference so that a plan access allocates nothing: the
/// scope is a constant of the desktop shape, so there is no reason to rebuild
/// it per call.
pub(crate) fn desktop_organization_scope() -> &'static OrganizationScope {
    static SCOPE: OnceLock<OrganizationScope> = OnceLock::new();
    SCOPE.get_or_init(|| OrganizationScope::from_context(desktop_context()))
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

    #[test]
    fn the_scope_is_reused_so_a_plan_access_allocates_nothing() {
        let first = desktop_organization_scope() as *const OrganizationScope;
        let second = desktop_organization_scope() as *const OrganizationScope;
        assert_eq!(
            first, second,
            "the scope must be one shared instance, not rebuilt per access"
        );
    }
}
