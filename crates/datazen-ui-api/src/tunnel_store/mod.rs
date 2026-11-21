mod flow;

#[cfg(test)]
mod tests;

pub use flow::{
    begin_load, plan_create, plan_remove, plan_update, plan_usage, settle_load, usage_from_lists,
    TunnelCreatePlan, TunnelStoreEffect,
};

use crate::types::SavedTunnelSummary;
use crate::{SavedTunnelKind, SshTunnelConfig, HttpProxyTunnelConfig, WebSocketTunnelConfig};

pub const TUNNEL_STORE_SURFACE: [&str; 8] = [
    "load",
    "create",
    "update",
    "remove",
    "usage",
    "newTunnelId",
    "errorMessage",
    "TUNNEL_STORE_GAP",
];

pub const TUNNEL_STORE_GAP: [&str; 5] = [
    "Zustand store creation and setState wiring",
    "tunnelCommands getTunnelSummaries/saveTunnel/deleteTunnel/getTunnelUsage IPC",
    "Math.random-based tunnel id generation",
    "async IPC timing and force-load refresh after writes",
    "i18n t() for the load-failure fallback message",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TunnelStoreState {
    pub summaries: Vec<SavedTunnelSummary>,
    pub loaded: bool,
    pub loading: bool,
    pub error: Option<String>,
}

impl Default for TunnelStoreState {
    fn default() -> Self {
        Self {
            summaries: Vec::new(),
            loaded: false,
            loading: false,
            error: None,
        }
    }
}

pub struct TunnelInput {
    pub id: Option<String>,
    pub name: String,
    pub kind: SavedTunnelKind,
    pub ssh: Option<SshTunnelConfig>,
    pub http_proxy: Option<HttpProxyTunnelConfig>,
    pub websocket: Option<WebSocketTunnelConfig>,
}

pub fn new_tunnel_id(random_base36: &str) -> String {
    let fragment = random_base36.get(2..10).unwrap_or("");
    format!("tun_{fragment}")
}

pub fn error_message(error: Option<&str>, fallback: &str) -> String {
    match error {
        Some(message) if !message.is_empty() => message.to_owned(),
        _ => fallback.to_owned(),
    }
}
