//! Reading the session back off the server — the evidence behind every
//! observation this provider returns.
//!
//! Redis is the awkward case, and the whole file exists to be honest about it.
//!
//! PostgreSQL can read its own session back: `current_database()` and
//! `search_path` come straight off the connection, so its observation is
//! [`ObservationConfidence::Confirmed`]. **Redis cannot.** A Redis server does
//! not report which logical database a connection is attached to — `INFO
//! server` carries no such field, and the one command that would (`CLIENT
//! INFO`) is only referenced in this crate for classifying danger
//! (`ops/exec.rs:24,230`), never for context.
//!
//! So this provider splits the difference honestly rather than guessing:
//!
//! * The **round trip is the evidence**. `INFO server` must come back before
//!   any field is set, so liveness, health and protocol drain are observed
//!   facts. [`resource::redis_context`] reports the session
//!   [`SessionState::Ready`] and [`ResourceHealth::Healthy`] only because that
//!   command succeeded — and a failed probe is an error, never a defaulted
//!   observation.
//! * The **namespace stays empty**. `redis_version` is parsed nowhere on
//!   purpose: `ResourceDescriptor` has no field for it, and a read whose result
//!   is thrown away is not evidence. A round trip that *must succeed* is a
//!   liveness proof; a value nobody can carry is not.
//!
//! The context therefore reports [`ObservationConfidence::Partial`] with an empty
//! namespace. That combination is deliberate and it is the contract's own way of
//! saying "some of this was observed": `SessionContext::matches_target` requires
//! [`ObservationConfidence::Confirmed`], so a caller can never mistake this
//! partial answer for a namespace the server confirmed.

use datazen_driver_api::namespace::NamespaceTarget;
use datazen_driver_api::resource::ResourceError;
use datazen_driver_api::session::{
    ObservationConfidence, ResourceHealth, SessionContext, SessionState, TransactionState,
};
use datazen_driver_api::{ConnectionHandle, DriverError};

use crate::driver::RedisDriver;

/// Proof that the session is live.
///
/// The value carries no data on purpose: `INFO server` returns a dozen fields
/// and `ResourceDescriptor` has nowhere to put any of them. What the round trip
/// establishes is the *fact of a reply*, which is exactly what
/// `protocol_drained` and `resource_health` claim, so the proof is the proof
/// and the payload is discarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LivenessProbe;

/// Issue `INFO server` on the resource's own connection and require it to
/// answer.
///
/// The lookup goes through [`RedisDriver::get_conn`], the single keyed access
/// point every operation in this crate uses — so the probe runs on the very
/// session that will run the caller's next command, and not on some scratch
/// connection opened for enumeration. (The crate makes that distinction
/// deliberately elsewhere: `database.rs:110-124 get_tables` opens a *pinned*
/// connection so that enumerating databases cannot flip the shared session.)
///
/// `INFO server` is issued through the same `with_redis_conn!` dispatch as every
/// other command, so this works identically for standalone, cluster and
/// sentinel topologies without the provider knowing which it has.
///
/// The registry write lock is held across the await, which is this crate's own
/// established convention (`get_databases`, `get_server_info` both do it). The
/// hold is one `INFO server` round trip on one connection.
pub(crate) async fn probe_liveness(
    driver: &RedisDriver,
    handle: &ConnectionHandle,
) -> Result<LivenessProbe, ResourceError> {
    let mut conns = driver.connections.write().await;
    let rc = RedisDriver::get_conn(&mut conns, handle).map_err(ResourceError::Driver)?;

    crate::with_redis_conn!(&mut rc.live, |conn| {
        crate::driver::session::info_server_on(conn).await
    })
    .map_err(|error| {
        ResourceError::Driver(DriverError::QueryFailed(format!(
            "session observation failed: the INFO server probe on the resource's own \
             connection did not answer: {error}"
        )))
    })?;

    Ok(LivenessProbe)
}

/// The context of a live Redis session.
///
/// [`ObservationConfidence::Partial`], and the namespace is empty on purpose:
/// the probe proved the session answers, but the server never told us which
/// logical database it is attached to, and reporting the one this provider
/// *last selected* would be stating the provider's own bookkeeping as a server
/// fact. [`SessionContext::matches_target`] refuses anything that is not
/// [`ObservationConfidence::Confirmed`], so the honest "I saw the socket, I did
/// not see the context" answer cannot be read as a match.
pub(crate) fn redis_context() -> SessionContext {
    SessionContext {
        namespace: NamespaceTarget::empty(),
        // Redis has no role attached to the connection as far as this contract
        // is concerned: `INFO server` does not carry the authenticated user, and
        // `CLIENT INFO` is not used for context here.
        effective_identity: None,
        search_path: Vec::new(),
        transaction_state: TransactionState::Unknown,
        autocommit: None,
        confidence: ObservationConfidence::Partial,
    }
}

/// The state a session that answered `INFO server` can honestly be placed in.
///
/// The command completing without error is the whole argument: a session that
/// can still run a command against the server is open and ready.
pub(crate) fn live_session_state() -> SessionState {
    SessionState::Ready
}

/// The health a session that answered `INFO server` can honestly be given.
///
/// The observation succeeded, which *is* the evidence for `Healthy`. A failure
/// would have been returned as an error rather than reported as `Degraded`,
/// because this provider has no way to distinguish "slow" from "gone" without
/// a probe it does not have.
pub(crate) fn live_resource_health() -> ResourceHealth {
    ResourceHealth::Healthy
}
