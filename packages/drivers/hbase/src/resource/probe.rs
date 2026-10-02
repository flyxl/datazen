//! What one cheap round trip proves about an HBase resource.
//!
//! `observe_session` on this driver is not "read the server's session state" —
//! Stargate keeps no per-client session this driver could read back. It is one
//! authenticated request on the resource's own client, and the only thing it can
//! honestly establish is **liveness**. So this file does exactly that:
//!
//! * one `GET {base}/` — the same Stargate status request
//!   [`HBaseDriver::test_connection`](crate::hbase::HBaseDriver::test_connection)
//!   issues, reused so the probe needs no endpoint of its own;
//! * four outcomes, because four different things are true:
//!   * [`EndpointLiveness::Gone`] — the driver no longer holds a client for this
//!     handle. Nothing can be said about the cluster, but every handle onto this
//!     resource is certainly dead, and this is the *only* case
//!     [`resource_health_of`](super::observation::resource_health_of) reports as
//!     lost;
//!   * [`EndpointLiveness::Unreachable`] — the request failed in transport. A
//!     network failure is not proof the cluster is gone, so this is `Degraded`;
//!   * [`EndpointLiveness::AnsweredNotUsable`] — something answered, but not
//!     with a status this client can use. The endpoint is alive and refusing;
//!   * [`EndpointLiveness::Answered`] — a success status, proof it can serve a
//!     request right now.
//!
//! The distinction that matters: `Lost` is claimed **only** when the driver has
//! lost the connection. Reporting `Lost` on an unreachable endpoint would tell a
//! caller its handles are dead on the strength of one timeout.

use datazen_driver_api::ConnectionHandle;

use crate::hbase::HBaseDriver;

/// The outcome of one liveness probe, split by what each outcome actually proves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EndpointLiveness {
    /// The endpoint answered with a success status.
    Answered,
    /// The endpoint answered, but not with a status this client can use.
    AnsweredNotUsable,
    /// The request never completed: refused, timed out, or unresolvable.
    Unreachable,
    /// The driver no longer holds a client for this handle.
    Gone,
}

/// One authenticated round trip against the resource's own endpoint.
pub(crate) async fn probe_liveness(
    driver: &HBaseDriver,
    handle: &ConnectionHandle,
) -> EndpointLiveness {
    // The read guard is taken and dropped inside `endpoint`, so nothing holds a
    // lock across this request.
    let Some((client, base)) = driver.endpoint(handle).await else {
        return EndpointLiveness::Gone;
    };

    // Stargate answers `GET /` with the cluster status, which is the request
    // `test_connection` already makes for this driver. Any status is drained to
    // completion by `send()` dropping the response.
    match client.get(format!("{base}/")).send().await {
        Err(_) => EndpointLiveness::Unreachable,
        Ok(response) if response.status().is_success() => EndpointLiveness::Answered,
        Ok(_) => EndpointLiveness::AnsweredNotUsable,
    }
}