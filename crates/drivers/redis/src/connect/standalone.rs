//! Standalone openers: the `PREFER_TLS_PROBE` plaintext fallback and the pub/sub pair.

use super::client::open_standalone_client;
use super::parse::connect_with_timeout;
use super::plan::TlsPlan;
use super::sentinel::plaintext_url;
use datazen_driver_api::DriverError;
use redis::aio::MultiplexedConnection;
use std::time::Duration;

/// Cap on the TLS attempt when the connection prefers TLS but may fall back to
/// plaintext. Real TLS handshakes finish in milliseconds, so this is only hit
/// when the server is plaintext (or TLS is broken) — fail fast and retry.
pub(crate) const PREFER_TLS_PROBE: Duration = Duration::from_secs(5);

async fn open_standalone_conn(
    url: &str,
    tls: &TlsPlan,
    username: Option<&str>,
    password: Option<&str>,
    timeout: Duration,
) -> Result<MultiplexedConnection, DriverError> {
    let client = open_standalone_client(url, tls, username, password)?;
    connect_with_timeout(timeout, client.get_multiplexed_async_connection()).await
}

pub(crate) async fn open_standalone_conn_with_fallback(
    url: &str,
    tls: &TlsPlan,
    username: Option<&str>,
    password: Option<&str>,
    timeout: Duration,
) -> Result<MultiplexedConnection, DriverError> {
    let tls_timeout = if tls.prefer_fallback {
        timeout.min(PREFER_TLS_PROBE)
    } else {
        timeout
    };
    match open_standalone_conn(url, tls, username, password, tls_timeout).await {
        Ok(conn) => Ok(conn),
        Err(DriverError::ConnectionFailed(_)) if tls.prefer_fallback => {
            open_standalone_conn(
                &plaintext_url(url),
                &TlsPlan::plaintext(),
                username,
                password,
                timeout,
            )
            .await
        }
        Err(e) => Err(e),
    }
}

pub(crate) async fn open_standalone_pubsub(
    url: &str,
    tls: &TlsPlan,
    username: Option<&str>,
    password: Option<&str>,
    timeout: Duration,
) -> Result<redis::aio::PubSub, DriverError> {
    let client = open_standalone_client(url, tls, username, password)?;
    connect_with_timeout(timeout, client.get_async_pubsub()).await
}

pub(crate) async fn open_standalone_pubsub_with_fallback(
    url: &str,
    tls: &TlsPlan,
    username: Option<&str>,
    password: Option<&str>,
    timeout: Duration,
) -> Result<redis::aio::PubSub, DriverError> {
    let tls_timeout = if tls.prefer_fallback {
        timeout.min(PREFER_TLS_PROBE)
    } else {
        timeout
    };
    match open_standalone_pubsub(url, tls, username, password, tls_timeout).await {
        Ok(pubsub) => Ok(pubsub),
        Err(DriverError::ConnectionFailed(_)) if tls.prefer_fallback => {
            open_standalone_pubsub(
                &plaintext_url(url),
                &TlsPlan::plaintext(),
                username,
                password,
                timeout,
            )
            .await
        }
        Err(e) => Err(e),
    }
}
