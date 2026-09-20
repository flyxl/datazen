//! Tunnel configuration types shared with the host app.

use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

fn default_timeout_30() -> u32 {
    30
}

fn default_ping_30() -> u32 {
    30
}

fn default_ws_mode() -> String {
    "datazen_v1".to_string()
}

/// Tunnel strategy. Mutually exclusive paths; inferred from legacy configs when absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TunnelKind {
    #[default]
    #[serde(rename = "none")]
    None,
    #[serde(rename = "ssh")]
    Ssh,
    #[serde(rename = "httpProxy")]
    HttpProxy,
    #[serde(rename = "websocket")]
    WebSocket,
}

/// HTTP CONNECT proxy tunnel (local listener -> proxy CONNECT -> remote DB).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpProxyTunnelConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    /// Transport to the proxy: `"http"` or `"https"`.
    pub scheme: String,
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(default)]
    pub headers: Option<std::collections::HashMap<String, String>>,
    #[serde(default = "default_timeout_30")]
    pub connect_timeout_secs: u32,
}

/// WebSocket tunnel client (local listener -> WS relay -> remote DB).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketTunnelConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Full URL, e.g. `wss://relay.example.com/v1/tunnel`.
    pub url: String,
    pub auth_token: Option<String>,
    #[serde(default)]
    pub headers: Option<std::collections::HashMap<String, String>>,
    #[serde(default = "default_timeout_30")]
    pub connect_timeout_secs: u32,
    #[serde(default = "default_ping_30")]
    pub ping_interval_secs: u32,
    /// `"datazen_v1"` or `"raw_binary"`.
    #[serde(default = "default_ws_mode")]
    pub mode: String,
}

/// Independently persisted tunnel definition (stored in `tunnels.json`).
///
/// Connections reference a tunnel via [`crate::ConnectionConfig::tunnel_id`].
/// Only one of `ssh` / `http_proxy` / `websocket` is populated based on `kind`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedTunnel {
    pub id: String,
    pub name: String,
    /// Must be `ssh` | `httpProxy` | `websocket` (not `none`).
    pub kind: TunnelKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh: Option<crate::SshTunnelConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_proxy: Option<HttpProxyTunnelConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub websocket: Option<WebSocketTunnelConfig>,
}
