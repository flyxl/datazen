//! Config parsing: timeouts, topology, TLS, and the host/port/node/sentinel URL readers.

use super::plan::TlsPlan;
use super::plan::Topology;
use super::tls::build_node_url;
use datazen_driver_api::{ConnectionConfig, DriverError, SslMode};
use serde_json::Map;
use std::future::Future;
use std::time::Duration;

/// Bound connection establishment so a misconfigured endpoint (for example a
/// TLS handshake against a plaintext server) fails fast instead of hanging.
pub(crate) async fn connect_with_timeout<T>(
    timeout: Duration,
    fut: impl Future<Output = Result<T, redis::RedisError>>,
) -> Result<T, DriverError> {
    tokio::time::timeout(timeout, fut)
        .await
        .map_err(|_| {
            DriverError::ConnectionFailed(format!("Redis connection timed out after {timeout:?}"))
        })?
        .map_err(|e| DriverError::ConnectionFailed(e.to_string()))
}

pub(crate) fn parse_topology(opts: Option<&Map<String, serde_json::Value>>) -> Topology {
    match opt_string(opts, "topology").as_deref() {
        Some("cluster") => Topology::Cluster,
        Some("sentinel") => Topology::Sentinel,
        _ => Topology::Standalone,
    }
}

pub(crate) fn parse_tls(
    opts: Option<&Map<String, serde_json::Value>>,
    ssl_mode: &SslMode,
) -> TlsPlan {
    let tls_obj = opts.and_then(|m| m.get("tls"));
    let explicit_enabled = tls_obj
        .and_then(|v| v.get("enabled"))
        .and_then(|v| v.as_bool());
    // Redis TLS is connection-level, so `Prefer` is implemented as: attempt
    // TLS first, then fall back to plaintext if the handshake fails. An
    // explicit `options.tls.enabled = true` (Redis wizard TLS step) or a
    // strict ssl_mode is treated as a hard requirement — never downgrade.
    let ssl_prefer = matches!(ssl_mode, SslMode::Prefer);
    let ssl_forced = matches!(
        ssl_mode,
        SslMode::Require | SslMode::VerifyCa | SslMode::VerifyFull
    );
    let enabled = explicit_enabled.unwrap_or(false) || ssl_forced || ssl_prefer;
    TlsPlan {
        enabled,
        prefer_fallback: ssl_prefer && explicit_enabled != Some(true),
        ca_path: tls_nested_string(tls_obj, "caPath"),
        cert_path: tls_nested_string(tls_obj, "certPath"),
        key_path: tls_nested_string(tls_obj, "keyPath"),
        key_passphrase: tls_nested_string(tls_obj, "keyPassphrase"),
        insecure_skip_verify: tls_obj
            .and_then(|v| v.get("insecureSkipVerify"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    }
}

fn tls_nested_string(tls: Option<&serde_json::Value>, key: &str) -> Option<String> {
    tls.and_then(|v| v.get(key))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub(crate) fn opt_string(
    opts: Option<&Map<String, serde_json::Value>>,
    key: &str,
) -> Option<String> {
    opts.and_then(|m| m.get(key))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn opt_string_array(opts: Option<&Map<String, serde_json::Value>>, key: &str) -> Vec<String> {
    opts.and_then(|m| m.get(key))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub(crate) fn parse_db_index(raw: Option<&str>) -> Result<u32, DriverError> {
    let s = raw.map(str::trim).unwrap_or("");
    if s.is_empty() {
        return Ok(0);
    }
    if let Some(rest) = s.strip_prefix("db") {
        rest.parse::<u32>().map_err(|_| {
            DriverError::InvalidConfig("invalid database name (expected e.g. db0)".into())
        })
    } else {
        s.parse::<u32>().map_err(|_| {
            DriverError::InvalidConfig("invalid database name (expected e.g. db0)".into())
        })
    }
}

pub(crate) fn parse_host_port(node: &str) -> Result<(String, u16), DriverError> {
    let node = node.trim();
    if node.is_empty() {
        return Err(DriverError::InvalidConfig("empty node address".into()));
    }
    if let Some((host, port)) = node.rsplit_once(':') {
        let port = port
            .parse::<u16>()
            .map_err(|_| DriverError::InvalidConfig(format!("invalid port in node {node:?}")))?;
        let host = host.trim();
        if host.is_empty() {
            return Err(DriverError::InvalidConfig(format!(
                "missing host in node {node:?}"
            )));
        }
        return Ok((host.to_string(), port));
    }
    Err(DriverError::InvalidConfig(format!(
        "node address must be host:port, got {node:?}"
    )))
}

pub(crate) fn parse_node_urls(
    opts: Option<&Map<String, serde_json::Value>>,
    config: &ConnectionConfig,
    tls: &TlsPlan,
    nodes_key: Option<&str>,
) -> Result<Vec<String>, DriverError> {
    let key = nodes_key.unwrap_or("clusterNodes");
    let mut nodes = opt_string_array(opts, key);
    if nodes.is_empty() && nodes_key.is_none() {
        let host = config.host.as_deref().unwrap_or(super::DEFAULT_HOST);
        let port = config.port.unwrap_or(super::DEFAULT_PORT);
        nodes.push(format!("{host}:{port}"));
    }
    nodes
        .iter()
        .map(|node| {
            let (host, port) = parse_host_port(node)?;
            Ok(build_node_url(
                tls,
                &host,
                port,
                config.username.as_deref(),
                config.password.as_deref(),
                None,
            ))
        })
        .collect()
}

pub(crate) fn parse_sentinel_urls(
    opts: Option<&Map<String, serde_json::Value>>,
    config: &ConnectionConfig,
    tls: &TlsPlan,
    sentinel_password: Option<&str>,
) -> Result<Vec<String>, DriverError> {
    let nodes = opt_string_array(opts, "sentinelNodes");
    nodes
        .iter()
        .map(|node| {
            let (host, port) = parse_host_port(node)?;
            Ok(build_node_url(
                tls,
                &host,
                port,
                config.username.as_deref(),
                sentinel_password.or(config.password.as_deref()),
                None,
            ))
        })
        .collect()
}

pub(crate) fn scheme_for_tls(tls: &TlsPlan) -> &'static str {
    if tls.enabled {
        "rediss"
    } else {
        "redis"
    }
}
