//! Connection plan parsing and live Redis backends (standalone / cluster / sentinel + TLS).

use datazen_driver_api::{ConnectionConfig, DriverError, SslMode};
use redis::aio::MultiplexedConnection;
use redis::cluster::{ClusterClient, TlsMode};
use redis::cluster_async::ClusterConnection;
use redis::sentinel::{Sentinel, SentinelClient, SentinelNodeConnectionInfo, SentinelServerType};
use redis::{Client, ClientTlsConfig, RedisConnectionInfo, TlsCertificates as RedisTlsCertificates};
use serde_json::Map;
use std::fs;
use std::path::Path;

/// Parsed TLS material paths and flags (no network I/O).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsPlan {
    pub enabled: bool,
    pub ca_path: Option<String>,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
    pub key_passphrase: Option<String>,
    pub insecure_skip_verify: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topology {
    Standalone,
    Cluster,
    Sentinel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StandalonePlan {
    pub url: String,
    pub tls: TlsPlan,
    pub db_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterPlan {
    pub node_urls: Vec<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub tls: TlsPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentinelPlan {
    pub sentinel_urls: Vec<String>,
    pub master_name: String,
    pub sentinel_password: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub tls: TlsPlan,
    pub db_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionPlan {
    Standalone(StandalonePlan),
    Cluster(ClusterPlan),
    Sentinel(SentinelPlan),
}

/// Live connection handle — standalone, cluster, or sentinel-backed master connection.
pub enum RedisLiveConn {
    Standalone(MultiplexedConnection),
    Cluster(ClusterConnection),
    Sentinel {
        client: SentinelClient,
        connection: MultiplexedConnection,
    },
}

impl RedisLiveConn {
    pub fn is_sentinel(&self) -> bool {
        matches!(self, Self::Sentinel { .. })
    }

    /// Re-resolve the current master via Sentinel and replace the cached connection.
    pub async fn rediscover_sentinel_master(&mut self) -> Result<(), DriverError> {
        let Self::Sentinel { client, connection } = self else {
            return Ok(());
        };
        *connection = client
            .get_async_connection()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        Ok(())
    }
}

pub fn build_connection_plan(config: &ConnectionConfig) -> Result<ConnectionPlan, DriverError> {
    let opts = config.options.as_ref();
    let topology = parse_topology(opts);
    let tls = parse_tls(opts, &config.ssl_mode);

    match topology {
        Topology::Standalone => {
            let host = config.host.as_deref().unwrap_or("127.0.0.1");
            let port = config.port.unwrap_or(6379);
            let db_index = parse_db_index(config.database.as_deref())?;
            let url = build_node_url(
                &tls,
                host,
                port,
                config.username.as_deref(),
                config.password.as_deref(),
                Some(db_index),
            );
            Ok(ConnectionPlan::Standalone(StandalonePlan {
                url,
                tls,
                db_index,
            }))
        }
        Topology::Cluster => {
            let node_urls = parse_node_urls(opts, config, &tls, None)?;
            if node_urls.is_empty() {
                return Err(DriverError::InvalidConfig(
                    "cluster topology requires at least one cluster node".into(),
                ));
            }
            Ok(ConnectionPlan::Cluster(ClusterPlan {
                node_urls,
                username: non_empty(config.username.as_deref()),
                password: config.password.clone(),
                tls,
            }))
        }
        Topology::Sentinel => {
            let master_name = opt_string(opts, "sentinelMasterName").ok_or_else(|| {
                DriverError::InvalidConfig("sentinel topology requires sentinelMasterName".into())
            })?;
            let sentinel_password = opt_string(opts, "sentinelNodePassword");
            let sentinel_urls = parse_sentinel_urls(opts, config, &tls, sentinel_password.as_deref())?;
            if sentinel_urls.is_empty() {
                return Err(DriverError::InvalidConfig(
                    "sentinel topology requires at least one sentinel node".into(),
                ));
            }
            Ok(ConnectionPlan::Sentinel(SentinelPlan {
                sentinel_urls,
                master_name,
                sentinel_password,
                username: non_empty(config.username.as_deref()),
                password: config.password.clone(),
                tls,
                db_index: parse_db_index(config.database.as_deref())?,
            }))
        }
    }
}

/// Open a dedicated Pub/Sub connection for SUBSCRIBE / PSUBSCRIBE.
///
/// Cluster topology uses the first seed node as a standalone client (Pub/Sub is
/// node-local on Cluster).
pub async fn open_pubsub_connection(
    plan: &ConnectionPlan,
) -> Result<redis::aio::PubSub, DriverError> {
    match plan {
        ConnectionPlan::Standalone(p) => {
            let client = open_standalone_client(&p.url, &p.tls)?;
            client
                .get_async_pubsub()
                .await
                .map_err(|e| DriverError::ConnectionFailed(e.to_string()))
        }
        ConnectionPlan::Cluster(p) => {
            let url = p.node_urls.first().ok_or_else(|| {
                DriverError::InvalidConfig("cluster topology requires at least one cluster node".into())
            })?;
            let client = open_standalone_client(url, &p.tls)?;
            client
                .get_async_pubsub()
                .await
                .map_err(|e| DriverError::ConnectionFailed(e.to_string()))
        }
        ConnectionPlan::Sentinel(p) => {
            let node_info = sentinel_node_info(p);
            let mut sentinel = Sentinel::build(p.sentinel_urls.clone())
                .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
            let master_client = sentinel
                .async_master_for(&p.master_name, Some(&node_info))
                .await
                .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
            master_client
                .get_async_pubsub()
                .await
                .map_err(|e| DriverError::ConnectionFailed(e.to_string()))
        }
    }
}

pub async fn open_live_conn(plan: &ConnectionPlan) -> Result<RedisLiveConn, DriverError> {
    match plan {
        ConnectionPlan::Standalone(p) => {
            let client = open_standalone_client(&p.url, &p.tls)?;
            let connection = client
                .get_multiplexed_async_connection()
                .await
                .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
            Ok(RedisLiveConn::Standalone(connection))
        }
        ConnectionPlan::Cluster(p) => {
            let mut builder = ClusterClient::builder(p.node_urls.clone());
            if let Some(user) = &p.username {
                builder = builder.username(user.clone());
            }
            if let Some(pass) = &p.password {
                builder = builder.password(pass.clone());
            }
            if let Some(mode) = tls_mode_for_plan(&p.tls) {
                builder = builder.tls(mode);
                if let Some(certs) = load_tls_certificates(&p.tls)? {
                    builder = builder.certs(certs);
                }
            }
            let client = builder
                .build()
                .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
            let connection = client
                .get_async_connection()
                .await
                .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
            Ok(RedisLiveConn::Cluster(connection))
        }
        ConnectionPlan::Sentinel(p) => {
            let node_info = sentinel_node_info(p);
            let mut client = SentinelClient::build(
                p.sentinel_urls.clone(),
                p.master_name.clone(),
                Some(node_info),
                SentinelServerType::Master,
            )
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
            let connection = client
                .get_async_connection()
                .await
                .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
            Ok(RedisLiveConn::Sentinel {
                client,
                connection,
            })
        }
    }
}

fn parse_topology(opts: Option<&Map<String, serde_json::Value>>) -> Topology {
    match opt_string(opts, "topology").as_deref() {
        Some("cluster") => Topology::Cluster,
        Some("sentinel") => Topology::Sentinel,
        _ => Topology::Standalone,
    }
}

fn parse_tls(opts: Option<&Map<String, serde_json::Value>>, ssl_mode: &SslMode) -> TlsPlan {
    let tls_obj = opts.and_then(|m| m.get("tls"));
    let explicit_enabled = tls_obj
        .and_then(|v| v.get("enabled"))
        .and_then(|v| v.as_bool());
    let ssl_requires = matches!(
        ssl_mode,
        SslMode::Prefer | SslMode::Require | SslMode::VerifyCa | SslMode::VerifyFull
    );
    TlsPlan {
        enabled: explicit_enabled.unwrap_or(false) || ssl_requires,
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

fn opt_string(opts: Option<&Map<String, serde_json::Value>>, key: &str) -> Option<String> {
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

fn non_empty(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

fn parse_db_index(raw: Option<&str>) -> Result<u32, DriverError> {
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

fn parse_host_port(node: &str) -> Result<(String, u16), DriverError> {
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
            return Err(DriverError::InvalidConfig(format!("missing host in node {node:?}")));
        }
        return Ok((host.to_string(), port));
    }
    Err(DriverError::InvalidConfig(format!(
        "node address must be host:port, got {node:?}"
    )))
}

fn parse_node_urls(
    opts: Option<&Map<String, serde_json::Value>>,
    config: &ConnectionConfig,
    tls: &TlsPlan,
    nodes_key: Option<&str>,
) -> Result<Vec<String>, DriverError> {
    let key = nodes_key.unwrap_or("clusterNodes");
    let mut nodes = opt_string_array(opts, key);
    if nodes.is_empty() && nodes_key.is_none() {
        let host = config.host.as_deref().unwrap_or("127.0.0.1");
        let port = config.port.unwrap_or(6379);
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

fn parse_sentinel_urls(
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

fn scheme_for_tls(tls: &TlsPlan) -> &'static str {
    if tls.enabled {
        "rediss"
    } else {
        "redis"
    }
}

fn build_node_url(
    tls: &TlsPlan,
    host: &str,
    port: u16,
    username: Option<&str>,
    password: Option<&str>,
    db_index: Option<u32>,
) -> String {
    let scheme = scheme_for_tls(tls);
    let user = non_empty(username).map(|u| urlencoding::encode(&u).into_owned());
    let pass = password.map(|p| urlencoding::encode(p).into_owned());

    let auth = match (user.as_deref(), pass.as_deref()) {
        (Some(u), Some(p)) => format!("{u}:{p}@"),
        (None, Some(p)) => format!(":{p}@"),
        (Some(u), None) => format!("{u}@"),
        (None, None) => String::new(),
    };

    match db_index {
        Some(db) => format!("{scheme}://{auth}{host}:{port}/{db}"),
        None => format!("{scheme}://{auth}{host}:{port}"),
    }
}

fn tls_mode_for_plan(tls: &TlsPlan) -> Option<TlsMode> {
    if !tls.enabled {
        return None;
    }
    if tls.insecure_skip_verify {
        Some(TlsMode::Insecure)
    } else {
        Some(TlsMode::Secure)
    }
}

fn read_pem(path: &str, label: &str) -> Result<Vec<u8>, DriverError> {
    if !Path::new(path).exists() {
        return Err(DriverError::SslError(format!(
            "TLS {label} file not found: {path}"
        )));
    }
    fs::read(path).map_err(|e| {
        DriverError::SslError(format!("failed to read TLS {label} file {path}: {e}"))
    })
}

fn load_tls_certificates(tls: &TlsPlan) -> Result<Option<RedisTlsCertificates>, DriverError> {
    if !tls.enabled {
        return Ok(None);
    }
    let client_tls = match (&tls.cert_path, &tls.key_path) {
        (Some(cert_path), Some(key_path)) => Some(ClientTlsConfig {
            client_cert: read_pem(cert_path, "client certificate")?,
            client_key: read_pem(key_path, "client private key")?,
        }),
        (None, None) => None,
        _ => {
            return Err(DriverError::SslError(
                "TLS client certificate and private key must both be set for mTLS".into(),
            ));
        }
    };
    let root_cert = tls
        .ca_path
        .as_ref()
        .map(|p| read_pem(p, "CA certificate"))
        .transpose()?;
    if client_tls.is_none() && root_cert.is_none() {
        return Ok(None);
    }
    Ok(Some(RedisTlsCertificates {
        client_tls,
        root_cert,
    }))
}

fn open_standalone_client(url: &str, tls: &TlsPlan) -> Result<Client, DriverError> {
    if let Some(certs) = load_tls_certificates(tls)? {
        return Client::build_with_tls(url, certs)
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()));
    }
    Client::open(url).map_err(|e| DriverError::ConnectionFailed(e.to_string()))
}

/// Sentinel TLS in redis 0.27: `SentinelNodeConnectionInfo` only exposes `tls_mode`
/// (Secure/Insecure). Custom CA/client PEM paths cannot be applied to master/replica
/// connections — `create_connection_info` always sets `tls_params: None`. Sentinel
/// node URLs use `rediss://` when TLS is enabled (system trust store only).
fn sentinel_node_info(plan: &SentinelPlan) -> SentinelNodeConnectionInfo {
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

#[macro_export]
macro_rules! with_redis_conn {
    ($live:expr, |$c:ident| $body:expr) => {
        match $live {
            $crate::connect::RedisLiveConn::Standalone($c) => $body,
            $crate::connect::RedisLiveConn::Cluster($c) => $body,
            $crate::connect::RedisLiveConn::Sentinel {
                connection: $c, ..
            } => $body,
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_driver_api::ConnectionConfig;
    use serde_json::json;

    fn base_config() -> ConnectionConfig {
        ConnectionConfig {
            id: "id".into(),
            name: "Redis".into(),
            database_type: "redis".into(),
            host: Some("127.0.0.1".into()),
            port: Some(6379),
            database: Some("db0".into()),
            schema: None,
            username: None,
            password: None,
            ssl_mode: SslMode::Disable,
            connection_timeout: 30,
            ssh_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
        }
    }

    #[test]
    fn default_topology_is_standalone() {
        let plan = build_connection_plan(&base_config()).unwrap();
        match plan {
            ConnectionPlan::Standalone(p) => {
                assert_eq!(p.db_index, 0);
                assert!(p.url.starts_with("redis://"));
                assert!(!p.tls.enabled);
            }
            _ => panic!("expected standalone plan"),
        }
    }

    #[test]
    fn cluster_topology_reads_nodes() {
        let mut config = base_config();
        let mut opts = Map::new();
        opts.insert("topology".into(), json!("cluster"));
        opts.insert(
            "clusterNodes".into(),
            json!(["10.0.0.1:7000", "10.0.0.2:7000"]),
        );
        config.options = Some(opts);
        let plan = build_connection_plan(&config).unwrap();
        match plan {
            ConnectionPlan::Cluster(p) => {
                assert_eq!(p.node_urls.len(), 2);
                assert!(p.node_urls[0].contains("10.0.0.1:7000"));
                assert!(p.node_urls[1].contains("10.0.0.2:7000"));
            }
            _ => panic!("expected cluster plan"),
        }
    }

    #[test]
    fn tls_enabled_from_options_flag() {
        let mut config = base_config();
        let mut opts = Map::new();
        opts.insert("tls".into(), json!({ "enabled": true }));
        config.options = Some(opts);
        let plan = build_connection_plan(&config).unwrap();
        match plan {
            ConnectionPlan::Standalone(p) => {
                assert!(p.tls.enabled);
                assert!(p.url.starts_with("rediss://"));
            }
            _ => panic!("expected standalone plan"),
        }
    }

    #[test]
    fn tls_enabled_from_ssl_mode_require() {
        let mut config = base_config();
        config.ssl_mode = SslMode::Require;
        let plan = build_connection_plan(&config).unwrap();
        match plan {
            ConnectionPlan::Standalone(p) => assert!(p.tls.enabled),
            _ => panic!("expected standalone plan"),
        }
    }

    #[test]
    fn sentinel_requires_master_name_and_nodes() {
        let mut config = base_config();
        let mut opts = Map::new();
        opts.insert("topology".into(), json!("sentinel"));
        opts.insert("sentinelNodes".into(), json!(["127.0.0.1:26379"]));
        config.options = Some(opts);
        assert!(build_connection_plan(&config).is_err());

        let mut config = base_config();
        let mut opts = Map::new();
        opts.insert("topology".into(), json!("sentinel"));
        opts.insert("sentinelMasterName".into(), json!("mymaster"));
        config.options = Some(opts);
        assert!(build_connection_plan(&config).is_err());
    }

    #[test]
    fn sentinel_plan_parses() {
        let mut config = base_config();
        let mut opts = Map::new();
        opts.insert("topology".into(), json!("sentinel"));
        opts.insert("sentinelMasterName".into(), json!("mymaster"));
        opts.insert("sentinelNodes".into(), json!(["127.0.0.1:26379"]));
        config.options = Some(opts);
        let plan = build_connection_plan(&config).unwrap();
        match plan {
            ConnectionPlan::Sentinel(p) => {
                assert_eq!(p.master_name, "mymaster");
                assert_eq!(p.sentinel_urls.len(), 1);
            }
            _ => panic!("expected sentinel plan"),
        }
    }
}
