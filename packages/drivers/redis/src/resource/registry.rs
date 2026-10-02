//! The state one Redis resource owns.
//!
//! Kept apart from [`RedisResourceProvider`](super::provider::RedisResourceProvider)'s
//! behaviour so the two can be read — and reviewed — separately: this file is
//! *what the provider remembers*, and it deliberately contains no I/O.
//!
//! The one thing worth noticing here is that no `ConnectionConfig` is stored.
//! The password must not outlive `connect`, so the registry keeps the driver's
//! `ConnectionHandle` (an id) and never the credentials that produced it.
//!
//! Note what is *missing* compared to the PostgreSQL provider: there is no
//! `OpenTransaction`. Redis has no SQL transaction, so a transaction slot here
//! could only ever hold `None` — and a field that is permanently empty is a
//! shell, which this module exists to avoid. The space it frees is taken by
//! `db_index`, which is the state a Redis session genuinely carries.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use datazen_driver_api::resource::BudgetPort;
use datazen_driver_api::ConnectionHandle;

/// What this provider owns for one acquired resource.
pub(super) struct LiveResource {
    pub(super) connection: ConnectionHandle,
    pub(super) permit: datazen_driver_api::resource::BudgetPermit,
    pub(super) budget: Arc<dyn BudgetPort>,
    /// The logical database this session is currently attached to.
    ///
    /// This is the whole point of a Redis resource: it is a single wire whose
    /// current `SELECT` index is real session state, and it is the target an
    /// in-place `change_context` moves. It is recorded rather than re-derived
    /// because asking the server would need a probe the driver does not
    /// expose — so the provider tracks what it itself last selected and says
    /// so, rather than reporting a namespace the server never confirmed.
    pub(super) db_index: u32,
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
            db_index: self.db_index,
            revision: self.revision,
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
