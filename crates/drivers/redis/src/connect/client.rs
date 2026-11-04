//! Low-level client construction: auth, standalone clients, pinned-node connections and sentinel node info.

use super::parse::parse_host_port;
use super::plan::ConnectionPlan;
use super::plan::SentinelPlan;
use super::plan::TlsPlan;
use super::standalone::open_standalone_conn_with_fallback;
use super::tls::build_node_url;
use super::tls::load_tls_certificates;
use super::tls::tls_mode_for_plan;
use datazen_driver_api::DriverError;
use redis::aio::MultiplexedConnection;
use redis::cluster::TlsMode;
use redis::sentinel::SentinelNodeConnectionInfo;
use redis::{Client, ConnectionInfo, RedisConnectionInfo};

fn connection_info_with_auth(
    url: &str,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<ConnectionInfo, DriverError> {
    let mut info: ConnectionInfo = url
        .parse::<ConnectionInfo>()
        .map_err(|e: redis::RedisError| DriverError::ConnectionFailed(e.to_string()))?;
    if let Some(user) = username {
        info.redis.username = Some(user.to_string());
    }
    if let Some(pass) = password {
        info.redis.password = Some(pass.to_string());
    }
    Ok(info)
}

pub(crate) fn open_standalone_client(
    url: &str,
    tls: &TlsPlan,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<Client, DriverError> {
    let info = connection_info_with_auth(url, username, password)?;
    if let Some(certs) = load_tls_certificates(tls)? {
        return Client::build_with_tls(info, certs)
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()));
    }
    Client::open(info).map_err(|e| DriverError::ConnectionFailed(e.to_string()))
}

/// Open a standalone multiplexed connection to a specific `host:port` node,
/// reusing TLS and credentials from the active connection plan (cluster pin routing).
pub async fn open_pinned_node_conn(
    plan: &ConnectionPlan,
    node_addr: &str,
) -> Result<MultiplexedConnection, DriverError> {
    let (host, port) = parse_host_port(node_addr)?;
    let (tls, username, password, connect_timeout) = match plan {
        ConnectionPlan::Cluster(p) => (
            &p.tls,
            p.username.as_deref(),
            p.password.as_deref(),
            p.connect_timeout,
        ),
        ConnectionPlan::Standalone(p) => (
            &p.tls,
            p.username.as_deref(),
            p.password.as_deref(),
            p.connect_timeout,
        ),
        ConnectionPlan::Sentinel(p) => (
            &p.tls,
            p.username.as_deref(),
            p.password.as_deref(),
            p.connect_timeout,
        ),
    };
    // Keep credentials out of the URL so connection errors cannot echo them.
    let url = build_node_url(tls, &host, port, None, None, None);
    open_standalone_conn_with_fallback(&url, tls, username, password, connect_timeout).await
}

/// Sentinel TLS in redis 0.27: `SentinelNodeConnectionInfo` only exposes `tls_mode`
/// (Secure/Insecure). Custom CA/client PEM paths cannot be applied to master/replica
/// connections — `create_connection_info` always sets `tls_params: None`. Sentinel
/// node URLs use `rediss://` when TLS is enabled (system trust store only).
pub(crate) fn sentinel_node_info(plan: &SentinelPlan) -> SentinelNodeConnectionInfo {
    let tls_mode = tls_mode_for_plan(&plan.tls).map(|m| match m {
        TlsMode::Insecure => redis::TlsMode::Insecure,
        TlsMode::Secure => redis::TlsMode::Secure,
    });
    let redis = RedisConnectionInfo {
        db: plan.db_index as i64,
        username: plan.username.clone(),
        password: plan.password.clone(),
        ..Default::default()
    };
    SentinelNodeConnectionInfo {
        tls_mode,
        redis_connection_info: Some(redis),
    }
}

/// Returns true when an operation error likely indicates a dropped master connection.
pub fn looks_like_connection_loss(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("connection reset")
        || lower.contains("connection refused")
        || lower.contains("broken pipe")
        || lower.contains("timed out")
        || lower.contains("connection closed")
        || lower.contains("io error")
        || lower.contains("connection lost")
}
