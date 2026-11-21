use crate::{
    HttpProxyScheme, HttpProxyTunnelConfig, SavedTunnel, SavedTunnelKind, SshAuthMethod,
    SshTunnelConfig, WebSocketMode, WebSocketTunnelConfig,
};

use super::types::{
    SshDraft, SshJumpDraft, TunnelDraft, DEFAULT_HTTP_PROXY_PORT, DEFAULT_SSH_PORT,
    DEFAULT_TUNNEL_TIMEOUT,
};
use super::validation::{positive_port, timeout_seconds};

pub fn empty_tunnel_draft() -> TunnelDraft {
    TunnelDraft::default()
}

fn timeout_text(value: Option<u32>) -> String {
    match value {
        Some(value) => value.to_string(),
        None => DEFAULT_TUNNEL_TIMEOUT.to_owned(),
    }
}

fn ssh_draft_from_config(config: Option<&SshTunnelConfig>) -> SshDraft {
    let config = match config {
        Some(config) => config,
        None => return SshDraft::default(),
    };
    let jump = match config.jump.as_deref() {
        Some(jump) => SshJumpDraft {
            enabled: jump.enabled,
            host: jump.host.clone(),
            port: jump.port.to_string(),
            username: jump.username.clone(),
            auth_method: jump.auth_method,
            password: jump.password.clone().unwrap_or_default(),
            key_path: jump.private_key_path.clone().unwrap_or_default(),
            passphrase: jump.passphrase.clone().unwrap_or_default(),
        },
        None => SshJumpDraft::default(),
    };
    SshDraft {
        host: config.host.clone(),
        port: config.port.to_string(),
        username: config.username.clone(),
        auth_method: config.auth_method,
        password: config.password.clone().unwrap_or_default(),
        key_path: config.private_key_path.clone().unwrap_or_default(),
        passphrase: config.passphrase.clone().unwrap_or_default(),
        jump,
    }
}

pub fn saved_tunnel_to_draft(tunnel: &SavedTunnel) -> TunnelDraft {
    let mut draft = empty_tunnel_draft();
    draft.id = Some(tunnel.id.clone());
    draft.name = tunnel.name.clone();
    draft.kind = tunnel.kind;
    draft.ssh = ssh_draft_from_config(tunnel.ssh.as_ref());
    if let Some(http) = tunnel.http_proxy.as_ref() {
        draft.http_proxy.host = http.host.clone();
        draft.http_proxy.port = http.port.to_string();
        draft.http_proxy.scheme = if http.scheme == HttpProxyScheme::Https {
            HttpProxyScheme::Https
        } else {
            HttpProxyScheme::Http
        };
        draft.http_proxy.username = http.username.clone().unwrap_or_default();
        draft.http_proxy.password = http.password.clone().unwrap_or_default();
        draft.http_proxy.timeout = timeout_text(http.connect_timeout_secs);
    }
    if let Some(websocket) = tunnel.websocket.as_ref() {
        draft.websocket.url = websocket.url.clone();
        draft.websocket.mode = if websocket.mode == Some(WebSocketMode::RawBinary) {
            WebSocketMode::RawBinary
        } else {
            WebSocketMode::DatazenV1
        };
        draft.websocket.auth_token = websocket.auth_token.clone().unwrap_or_default();
        draft.websocket.timeout = timeout_text(websocket.connect_timeout_secs);
    }
    draft
}

fn credential(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

fn ssh_config_from_draft(draft: &SshDraft) -> SshTunnelConfig {
    SshTunnelConfig {
        enabled: true,
        host: draft.host.trim().to_owned(),
        port: positive_port(&draft.port, DEFAULT_SSH_PORT.parse::<u16>().unwrap_or(22)),
        username: draft.username.trim().to_owned(),
        auth_method: draft.auth_method,
        password: if draft.auth_method == SshAuthMethod::Password {
            credential(&draft.password)
        } else {
            None
        },
        private_key_path: if draft.auth_method == SshAuthMethod::PrivateKey {
            credential(&draft.key_path)
        } else {
            None
        },
        passphrase: if draft.auth_method == SshAuthMethod::PrivateKey {
            credential(&draft.passphrase)
        } else {
            None
        },
        jump: if draft.jump.enabled {
            Some(Box::new(SshTunnelConfig {
                enabled: true,
                host: draft.jump.host.trim().to_owned(),
                port: positive_port(&draft.jump.port, DEFAULT_SSH_PORT.parse::<u16>().unwrap_or(22)),
                username: draft.jump.username.trim().to_owned(),
                auth_method: draft.jump.auth_method,
                password: if draft.jump.auth_method == SshAuthMethod::Password {
                    credential(&draft.jump.password)
                } else {
                    None
                },
                private_key_path: if draft.jump.auth_method == SshAuthMethod::PrivateKey {
                    credential(&draft.jump.key_path)
                } else {
                    None
                },
                passphrase: if draft.jump.auth_method == SshAuthMethod::PrivateKey {
                    credential(&draft.jump.passphrase)
                } else {
                    None
                },
                jump: None,
            }))
        } else {
            None
        },
    }
}

fn http_proxy_config_from_draft(draft: &crate::tunnel_draft::HttpProxyDraft) -> HttpProxyTunnelConfig {
    HttpProxyTunnelConfig {
        enabled: true,
        host: draft.host.trim().to_owned(),
        port: positive_port(&draft.port, DEFAULT_HTTP_PROXY_PORT.parse::<u16>().unwrap_or(8080)),
        scheme: draft.scheme,
        username: credential(&draft.username),
        password: credential(&draft.password),
        headers: None,
        connect_timeout_secs: Some(timeout_seconds(&draft.timeout)),
    }
}

fn websocket_config_from_draft(draft: &crate::tunnel_draft::WebSocketDraft) -> WebSocketTunnelConfig {
    WebSocketTunnelConfig {
        enabled: true,
        url: draft.url.trim().to_owned(),
        auth_token: credential(&draft.auth_token),
        headers: None,
        connect_timeout_secs: Some(timeout_seconds(&draft.timeout)),
        ping_interval_secs: None,
        mode: Some(draft.mode),
    }
}

pub fn draft_to_saved_tunnel(draft: &TunnelDraft, id: &str) -> SavedTunnel {
    let mut saved = SavedTunnel {
        id: id.to_owned(),
        name: draft.name.trim().to_owned(),
        kind: draft.kind,
        ssh: None,
        http_proxy: None,
        websocket: None,
    };
    match draft.kind {
        SavedTunnelKind::Ssh => saved.ssh = Some(ssh_config_from_draft(&draft.ssh)),
        SavedTunnelKind::HttpProxy => saved.http_proxy = Some(http_proxy_config_from_draft(&draft.http_proxy)),
        SavedTunnelKind::WebSocket => saved.websocket = Some(websocket_config_from_draft(&draft.websocket)),
    }
    saved
}
