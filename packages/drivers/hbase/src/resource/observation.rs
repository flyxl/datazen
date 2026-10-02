//! What an HBase resource can be observed to be.
//!
//! `observe_session` on this driver is not "read the server's session state" —
//! Stargate has no session. It is one cheap authenticated round trip on the
//! resource's own client, and the only thing it can honestly establish is
//! **liveness**. So this file does exactly that and nothing more:
//!
//! * the context stays [`SessionContext::unobserved`], which carries an *empty*
//!   namespace and therefore can never be mistaken for the acquisition target
//!   (`session.rs:194`);
//! * the transaction stays `Unsupported`, because none exists — see
//!   [`HBaseDriver::execute`](crate::hbase::HBaseDriver::execute), which refuses
//!   writes before they reach the wire;
//! * the handle list stays empty, because none was issued. The scanner that
//!   `HBaseDriver::scan` opens is read to exhaustion and deleted again inside
//!   one command, so it never outlives the call that made it;
//! * `resource_health` is the one cell the probe actually earns, and each of its
//!   four possible values is derived from a distinct measured outcome in
//!   [`EndpointLiveness`](super::probe::EndpointLiveness).
//!
//! The distinction that matters: `Lost` is claimed **only** when the driver no
//! longer holds a client for the handle. A refused connection is reported
//! `Degraded`, because a network failure is not proof that the cluster is gone,
//! and reporting `Lost` would tell a caller its handles are dead on the strength
//! of one timeout.

use datazen_driver_api::session::{
    ResourceHealth, SessionContext, SessionObservation, SessionState, TransactionObservation,
    TransactionState,
};

use super::probe::EndpointLiveness;

/// The lifecycle state a given reading proves.
pub fn session_state_of(liveness: EndpointLiveness) -> SessionState {
    match liveness {
        // A success status on this resource's own client is proof it can serve
        // a request right now.
        EndpointLiveness::Answered => SessionState::Ready,
        // Gone from the driver's pool: every handle onto it is dead. This is the
        // only case that may be reported as lost.
        EndpointLiveness::Gone => SessionState::Lost,
        // Either the endpoint did not answer, or it answered with something this
        // client cannot use. Neither places the session in the state machine, so
        // no state is claimed.
        EndpointLiveness::Unreachable | EndpointLiveness::AnsweredNotUsable => {
            SessionState::Unknown
        }
    }
}

/// The health a given reading proves.
pub fn resource_health_of(liveness: EndpointLiveness) -> ResourceHealth {
    match liveness {
        EndpointLiveness::Answered => ResourceHealth::Healthy,
        EndpointLiveness::AnsweredNotUsable => ResourceHealth::Degraded,
        // No answer is not proof the cluster is gone; it is proof this resource
        // cannot be used *now*. Claiming `Lost` here would kill handles on the
        // strength of one unreachable endpoint.
        EndpointLiveness::Unreachable => ResourceHealth::Degraded,
        EndpointLiveness::Gone => ResourceHealth::Lost,
    }
}

/// The observation returned after one probe.
///
/// Every field except `resource_health` is a constant, and each constant is the
/// honest value rather than the convenient one.
pub fn observed_session(liveness: EndpointLiveness, revision: u64) -> SessionObservation {
    SessionObservation {
        state: session_state_of(liveness),
        // `unobserved()`, not the acquisition target: this resource has no
        // readable context, and repeating the target back would be the driver
        // agreeing with its own input.
        context: SessionContext::unobserved(),
        transaction: TransactionObservation {
            state: TransactionState::Unsupported,
            transaction_id: None,
            effect: None,
            revision,
        },
        // No cursor, no prepared statement, no scanner: the driver issued none.
        handles: Vec::new(),
        // The probe response was fully consumed, so nothing is left in flight.
        protocol_drained: true,
        resource_health: resource_health_of(liveness),
        context_revision: revision,
    }
}