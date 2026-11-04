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

/// Metadata-only tunnel summary for selectors — must never carry any secret field.
///
/// The list IPC feeds a picker in the webview; projecting a full [`SavedTunnel`]
/// there would push decrypted SSH/proxy/WS credentials into the renderer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedTunnelSummary {
    pub id: String,
    pub name: String,
    pub kind: TunnelKind,
}

/// Which connections reference a tunnel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelUsage {
    pub connection_ids: Vec<String>,
    pub connection_names: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_serializes_metadata_only() {
        let summary = SavedTunnelSummary {
            id: "t1".into(),
            name: "Prod Bastion".into(),
            kind: TunnelKind::Ssh,
        };
        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "id": "t1", "name": "Prod Bastion", "kind": "ssh" })
        );
        for key in [
            "password",
            "passphrase",
            "authToken",
            "auth_token",
            "ssh",
            "httpProxy",
            "websocket",
        ] {
            assert!(
                json.get(key).is_none(),
                "SavedTunnelSummary must not carry `{key}`: {json}"
            );
        }
    }

    #[test]
    fn usage_serializes_camel_case_without_secrets() {
        let usage = TunnelUsage {
            connection_ids: vec!["c1".into(), "c2".into()],
            connection_names: vec!["Alpha".into(), "Beta".into()],
        };
        let json = serde_json::to_value(&usage).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "connectionIds": ["c1", "c2"],
                "connectionNames": ["Alpha", "Beta"],
            })
        );
        for key in ["password", "passphrase", "authToken", "auth_token"] {
            assert!(
                json.get(key).is_none(),
                "TunnelUsage must not carry `{key}`: {json}"
            );
        }
    }
}
