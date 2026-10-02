//! The state one PostgreSQL resource owns.
//!
//! Kept apart from [`PostgresResourceProvider`](super::provider::PostgresResourceProvider)'s
//! behaviour so the two can be read — and reviewed — separately: this file is
//! *what the provider remembers*, and it deliberately contains no I/O.
//!
//! The one thing worth noticing here is that no `ConnectionConfig` is stored.
//! The password must not outlive `connect_impl`, so the registry keeps the
//! driver's `ConnectionHandle` (an id) and never the credentials that produced
//! it.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use datazen_driver_api::resource::BudgetPort;
use datazen_driver_api::{ConnectionHandle, TransactionHandle};

/// What this provider owns for one acquired resource.
pub(super) struct LiveResource {
    pub(super) connection: ConnectionHandle,
    pub(super) permit: datazen_driver_api::resource::BudgetPermit,
    pub(super) budget: Arc<dyn BudgetPort>,
    /// The transaction this provider opened, when one is open. `commit_impl` /
    /// `rollback_impl` consume the handle by value, so it is *taken* out under
    /// the lock rather than read out: a second concurrent commit then finds
    /// `None` and is refused instead of issuing a second `COMMIT`.
    pub(super) transaction: Option<OpenTransaction>,
    /// Bumped on every state change and every observation, so two look-alike
    /// contexts can be told apart.
    pub(super) revision: u64,
}

impl LiveResource {
    /// The fields the provider reads without holding the registry lock.
    ///
    /// A copy, not a reference: the caller is about to `.await` on a socket, and
    /// the registry lock is never held across a suspension point.
    pub(super) fn snapshot(&self) -> LiveResource {
        LiveResource {
            connection: self.connection.clone(),
            permit: self.permit.clone(),
            budget: self.budget.clone(),
            transaction: self.transaction.clone(),
            revision: self.revision,
        }
    }
}

/// The transaction this provider opened, kept as the two fields
/// [`TransactionHandle`] carries.
///
/// `TransactionHandle` is deliberately not `Clone` in the contract, so it is
/// never copied out of the registry: the transaction is *taken* under the lock
/// and the handle is rebuilt once, by value, for the call that consumes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OpenTransaction {
    pub(super) id: String,
    pub(super) connection_id: String,
}

impl OpenTransaction {
    /// The driver's handle, for the by-value `commit_impl` / `rollback_impl`.
    pub(super) fn to_handle(&self) -> TransactionHandle {
        TransactionHandle {
            id: self.id.clone(),
            connection_id: self.connection_id.clone(),
        }
    }
}

/// Everything this provider instance owns, indexed by the handle's resource key.
#[derive(Default)]
pub(super) struct ResourceRegistry {
    /// Resources that are open. A key that is absent here is not a resource of
    /// this provider, whatever a caller holds.
    pub(super) live: BTreeMap<String, LiveResource>,
    /// Resources whose close was confirmed. A second close of one of these is
    /// idempotent and does not release the budget again.
    pub(super) closed: BTreeSet<String>,
}
