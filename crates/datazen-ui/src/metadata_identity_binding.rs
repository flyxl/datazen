//! The dependency-inversion seam between the connection store and the
//! schema store, ported from React
//! `packages/frontend/src/stores/metadataIdentityBinding.ts` (T274).
//!
//! ## Why this exists (the React doc comment, verbatim in intent)
//!
//! `schemaStore` transitively depends on `activeConnectionStore`
//! (column loading reads driver capabilities through that store), so a
//! static import in the opposite direction would close a module cycle
//! — and under Vitest the cycle deadlocks collection for every suite
//! that mocks `schemaStore` while the real `activeConnectionStore` is
//! still in the graph. The schema store registers its binder when it
//! initialises; callers only need to announce that a session's
//! metadata identity was observed, which is inert until the store
//! exists.
//!
//! ## Rust shape
//!
//! The React module holds one module-level mutable binder; the Rust
//! host owns one [`MetadataIdentityBinding`] cell instead (the
//! panel-close notifier's ruling: a module singleton becomes a value
//! the host holds, one per process, same observable discipline). The
//! `Rc` identity stands in for the closure identity the disposer
//! compares (`binder === next` → [`MetadataIdentityRegistration`]'s
//! `Rc::ptr_eq`), and the registration value is the disposer: hand it
//! to [`MetadataIdentityBinding::unregister`], which restores the
//! void only if the binder is still this one.

use std::rc::Rc;

/// `MetadataIdentityBinder`: the schema store's binding action.
pub(crate) type MetadataIdentityBinder =
    Rc<dyn Fn(&str, &str, Option<&str>)>;

/// The disposer `registerMetadataIdentityBinder` returns: a plain
/// value naming the binder it installed ([`Rc::ptr_eq`] is the
/// `binder === next` the closure compared).
#[derive(Clone)]
pub(crate) struct MetadataIdentityRegistration {
    binder: MetadataIdentityBinder,
}

/// The seam's cell: one per host, the module-level `binder` variable.
#[derive(Default)]
pub(crate) struct MetadataIdentityBinding {
    binder: Option<MetadataIdentityBinder>,
}

impl MetadataIdentityBinding {
    /// The module's initial state: no binder, announcements inert.
    pub(crate) fn new() -> Self {
        Self { binder: None }
    }

    /// `registerMetadataIdentityBinder(next)`: install the schema-store
    /// binder. Installing over an existing one replaces it, exactly as
    /// the module write does.
    pub(crate) fn register(
        &mut self,
        next: MetadataIdentityBinder,
    ) -> MetadataIdentityRegistration {
        self.binder = Some(Rc::clone(&next));
        MetadataIdentityRegistration { binder: next }
    }

    /// The registration's disposer: restore the void — but only if the
    /// binder is still this registration's (a replaced binder is not
    /// uninstalled by the old token, the closure's own guard).
    pub(crate) fn unregister(&mut self, registration: &MetadataIdentityRegistration) {
        if let Some(current) = &self.binder {
            if Rc::ptr_eq(current, &registration.binder) {
                self.binder = None;
            }
        }
    }

    /// `bindSessionMetadataIdentity(dbSessionId, connectionId, scope)`:
    /// forward a newly observed (session, config) pair to the schema
    /// store — inert until one exists (`binder?.(...)`).
    pub(crate) fn bind_session_metadata_identity(
        &self,
        db_session_id: &str,
        connection_id: &str,
        scope: Option<&str>,
    ) {
        if let Some(binder) = &self.binder {
            binder(db_session_id, connection_id, scope);
        }
    }

    /// Whether a binder is installed (the `binder != null` half the
    /// tests observe directly).
    pub(crate) fn has_binder(&self) -> bool {
        self.binder.is_some()
    }
}
