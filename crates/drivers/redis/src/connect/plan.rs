//! The connection *plan* value types: TLS material, topology, the three per-topology plans, and the live-connection handle.

use datazen_driver_api::DriverError;
use redis::aio::MultiplexedConnection;
use redis::cluster_async::ClusterConnection;
use redis::sentinel::SentinelClient;
use std::time::Duration;

/// Parsed TLS material paths and flags (no network I/O).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsPlan {
    pub enabled: bool,
    /// `ssl_mode = Prefer` without an explicit `options.tls.enabled` flag:
    /// attempt TLS first, then fall back to plaintext if the handshake fails.
    pub prefer_fallback: bool,
    pub ca_path: Option<String>,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
    pub key_passphrase: Option<String>,
    pub insecure_skip_verify: bool,
}

impl TlsPlan {
    pub(crate) fn plaintext() -> TlsPlan {
        TlsPlan {
            enabled: false,
            prefer_fallback: false,
            ca_path: None,
            cert_path: None,
            key_path: None,
            key_passphrase: None,
            insecure_skip_verify: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topology {
    Standalone,
    Cluster,
    Sentinel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StandalonePlan {
    /// Connection URL without credentials (password applied via `RedisConnectionInfo`).
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub tls: TlsPlan,
    pub db_index: u32,
    pub connect_timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterPlan {
    pub node_urls: Vec<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub tls: TlsPlan,
    pub connect_timeout: Duration,
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
    pub connect_timeout: Duration,
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

    /// Which topology this handle talks to, so an operation can pick a request
    /// shape the transport actually supports. A `ClusterConnection` folds batch
    /// errors and pins a pipeline to one slot, unlike the single-node
    /// `MultiplexedConnection` used by standalone and sentinel — see
    /// `ops_workbench`'s module docs for the consequence.
    pub fn topology(&self) -> Topology {
        match self {
            Self::Standalone(_) => Topology::Standalone,
            Self::Cluster(_) => Topology::Cluster,
            Self::Sentinel { .. } => Topology::Sentinel,
        }
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
