//! The state one HBase resource owns.
//!
//! Kept apart from
//! [`HBaseResourceProvider`](super::provider::HBaseResourceProvider)'s behaviour
//! so the two can be read — and reviewed — separately: this file is *what the
//! provider remembers*, and it contains no I/O.
//!
//! Two things are worth noticing here.
//!
//! * No `ConnectionConfig` is stored. The password must not outlive
//!   `HBaseDriver::connect`, so the registry keeps the driver's
//!   `ConnectionHandle` (an id) and never the credentials that produced it.
//! * There is no transaction field, and that is not an omission: this provider
//!   cannot open one — see
//!   [`hbase_capability_set`](super::capabilities::hbase_capability_set).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use datazen_driver_api::resource::{BudgetPermit, BudgetPort};
use datazen_driver_api::ConnectionHandle;

/// What this provider owns for one acquired resource.
///
/// `Clone` because [`acquire`](super::provider::HBaseResourceProvider::acquire)
/// hands the caller's handle and the registry's own copy from the same value;
/// [`snapshot`](LiveResource::snapshot) then takes plain copies for the calls
/// that must not hold the registry lock.
#[derive(Clone)]
pub(super) struct LiveResource {
    pub(super) connection: ConnectionHandle,
    pub(super) permit: BudgetPermit,
    /// Held so the charge can be released exactly once, in `close_resource`.
    pub(super) budget: Arc<dyn BudgetPort>,
    /// Bumped on every observation, so two look-alike readings can be told
    /// apart.
    pub(super) revision: u64,
}

impl LiveResource {
    /// The fields the provider reads without holding the registry lock.
    ///
    /// A copy, not a reference: the caller is about to `.await` on an HTTP
    /// request, and the registry lock is never held across a suspension point.
    pub(super) fn snapshot(&self) -> LiveResource {
        LiveResource {
            connection: self.connection.clone(),
            permit: self.permit.clone(),
            budget: self.budget.clone(),
            revision: self.revision,
        }
    }
}

/// Everything this provider instance owns, indexed by the handle's resource key.
#[derive(Clone, Default)]
pub(super) struct ResourceRegistry {
    /// Resources that are open. A key that is absent here is not a resource of
    /// this provider, whatever a caller holds.
    pub(super) live: BTreeMap<String, LiveResource>,
    /// Resources whose close was confirmed. A second close of one of these is
    /// idempotent and does not release the budget again.
    pub(super) closed: BTreeSet<String>,
}
