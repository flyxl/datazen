//! Shared types for database drivers.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Database type identifier — a plain string (e.g. "postgresql", "mysql", "superset").
/// Plugins define their own identifiers without modifying this crate.
pub type DatabaseType = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DriverCategory {
    Sql,
    KeyValue,
    Document,
}

/// DDL atomicity semantics reported by a driver.
///
/// Host code uses this to decide whether schema-diff deploy and similar
/// multi-statement DDL paths should wrap operations in a real transaction.
/// Drivers that do not override [`crate::DatabaseDriver::ddl_atomicity`]
/// return [`Self::Unknown`], which disables transactional wrapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DdlAtomicity {
    /// DDL participates in the surrounding transaction (e.g. PostgreSQL, SQLite).
    Transactional,
    /// Each DDL statement auto-commits (e.g. MySQL / MariaDB).
    AutoCommitPerStatement,
    /// Semantics are unknown; the host executes statements without wrapping.
    Unknown,
}

/// High-level sync/transfer pairing category (broader than wire protocol).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncCategory {
    Sql,
    Document,
    Kv,
    Other,
}

impl std::fmt::Display for SyncCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sql => write!(f, "Sql"),
            Self::Document => write!(f, "Document"),
            Self::Kv => write!(f, "Kv"),
            Self::Other => write!(f, "Other"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SslMode {
    #[default]
    Disable,
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshTunnelConfig {
    #[serde(default = "default_ssh_enabled")]
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(default = "default_auth_method")]
    pub auth_method: String,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    pub passphrase: Option<String>,
    /// Optional first hop (ProxyJump). Nested `jump` is allowed.
    #[serde(default)]
    pub jump: Option<Box<SshTunnelConfig>>,
}

fn default_ssh_enabled() -> bool {
    true
}
fn default_auth_method() -> String {
    "password".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionConfig {
    pub id: String,
    pub name: String,
    pub database_type: DatabaseType,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(default)]
    pub ssl_mode: SslMode,
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout: u32,
    /// Max connections in the driver's sqlx pool (host injects from AppSettings on connect).
    #[serde(default = "default_max_pool_size")]
    pub max_pool_size: u32,
    pub ssh_tunnel: Option<SshTunnelConfig>,
    /// Preferred tunnel strategy. When absent, inferred from legacy ssh/http/ws fields
    /// or from the referenced [`tunnel_id`] SavedTunnel.
    #[serde(default)]
    pub tunnel_kind: Option<crate::TunnelKind>,
    /// Reference to a independently stored [`crate::SavedTunnel`] (`tunnels.json`).
    /// When set, the host resolves and injects tunnel fields before connect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel_id: Option<String>,
    pub http_proxy_tunnel: Option<crate::HttpProxyTunnelConfig>,
    pub websocket_tunnel: Option<crate::WebSocketTunnelConfig>,
    pub color_tag: Option<String>,
    pub group: Option<String>,
    pub last_connected_at: Option<String>,
    pub server_version: Option<String>,
    /// Opaque per-driver connection options (e.g. Redis topology/TLS).
    #[serde(default)]
    pub options: Option<serde_json::Map<String, serde_json::Value>>,
    /// When true, mutating SQL and row edits are rejected by the host.
    #[serde(default)]
    pub read_only: bool,
    /// When true, the connection is sorted first within its group in the navigator.
    #[serde(default)]
    pub pinned: bool,
}

fn default_connection_timeout() -> u32 {
    30
}

fn default_max_pool_size() -> u32 {
    10
}

impl ConnectionConfig {
    /// Pool size for sqlx drivers: at least 1, capped at 100.
    pub fn effective_max_pool_size(&self) -> u32 {
        self.max_pool_size.clamp(1, 100)
    }
}

#[cfg(test)]
mod connection_config_tests {
    use super::*;
    use serde_json::json;

    fn dummy_connection_config() -> ConnectionConfig {
        ConnectionConfig {
            id: "test-id".into(),
            name: "Test".into(),
            database_type: "redis".into(),
            host: Some("127.0.0.1".into()),
            port: Some(6379),
            database: Some("0".into()),
            schema: None,
            username: None,
            password: None,
            ssl_mode: SslMode::Disable,
            connection_timeout: 30,
            max_pool_size: 10,
            ssh_tunnel: None,
            tunnel_kind: None,
            tunnel_id: None,
            http_proxy_tunnel: None,
            websocket_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
            pinned: false,
        }
    }

    #[test]
    fn max_pool_size_defaults_when_missing() {
        let json = json!({
            "id": "x",
            "name": "n",
            "databaseType": "postgresql",
        });
        let c: ConnectionConfig = serde_json::from_value(json).unwrap();
        assert_eq!(c.max_pool_size, 10);
        assert_eq!(c.effective_max_pool_size(), 10);
        assert!(!c.read_only);
        assert!(!c.pinned);
    }

    #[test]
    fn connection_options_roundtrip() {
        let mut opts = serde_json::Map::new();
        opts.insert("topology".into(), json!("cluster"));
        let c = ConnectionConfig {
            options: Some(opts),
            ..dummy_connection_config()
        };
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["options"]["topology"], "cluster");

        let restored: ConnectionConfig = serde_json::from_value(v).unwrap();
        assert_eq!(
            restored.options.as_ref().unwrap()["topology"],
            json!("cluster")
        );
    }

    #[test]
    fn connection_options_default_missing() {
        let json = json!({
            "id": "x",
            "name": "n",
            "databaseType": "redis",
        });
        let c: ConnectionConfig = serde_json::from_value(json).unwrap();
        assert!(c.options.is_none());
    }
}

include!("types_rest.inc.rs");
