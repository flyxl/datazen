//! Unified local-port tunnels for database connections.
//!
//! Drivers always connect to `127.0.0.1:<local_port>` when a tunnel is active.
//! See `docs/architecture/rfc/http-https-websocket-tunnel.zh-CN.md`.

mod http_proxy;
mod websocket;

pub use http_proxy::HttpProxyTunnel;
pub use websocket::WebSocketTunnel;

use crate::db::{
    ConnectionConfig, DriverError, HttpProxyTunnelConfig, TunnelKind, WebSocketTunnelConfig,
};
use crate::ssh_tunnel::SshTunnel;
use std::path::Path;

/// Runtime tunnel held by [`crate::services::connection_manager::ActiveSession`].
pub enum Tunnel {
    Ssh(SshTunnel),
    HttpProxy(HttpProxyTunnel),
    WebSocket(WebSocketTunnel),
}

impl Tunnel {
    pub fn local_port(&self) -> u16 {
        match self {
            Self::Ssh(t) => t.local_port(),
            Self::HttpProxy(t) => t.local_port(),
            Self::WebSocket(t) => t.local_port(),
        }
    }
}

/// Resolve effective tunnel kind with backward compatibility for pre-tunnel_kind configs.
pub fn resolve_tunnel_kind(config: &ConnectionConfig) -> TunnelKind {
    if let Some(kind) = config.tunnel_kind {
        return kind;
    }
    if config
        .ssh_tunnel
        .as_ref()
        .map(|s| s.enabled)
        .unwrap_or(false)
    {
        return TunnelKind::Ssh;
    }
    if config
        .http_proxy_tunnel
        .as_ref()
        .map(|s| s.enabled)
        .unwrap_or(false)
    {
        return TunnelKind::HttpProxy;
    }
    if config
        .websocket_tunnel
        .as_ref()
        .map(|s| s.enabled)
        .unwrap_or(false)
    {
        return TunnelKind::WebSocket;
    }
    TunnelKind::None
}

/// Start the tunnel required by `config`, if any.
///
/// On success with a tunnel, the returned [`ConnectionConfig`] has `host`/`port`
/// rewritten to the local listener and tunnel config cleared so drivers dial loopback.
pub async fn start_for_connection(
    config: ConnectionConfig,
    known_hosts_path: &Path,
) -> Result<(ConnectionConfig, Option<Tunnel>), DriverError> {
    let kind = resolve_tunnel_kind(&config);
    match kind {
        TunnelKind::None => Ok((config, None)),
        TunnelKind::Ssh => start_ssh(config, known_hosts_path).await,
        TunnelKind::HttpProxy => start_http_proxy(config).await,
        TunnelKind::WebSocket => start_websocket(config).await,
    }
}

async fn start_websocket(
    config: ConnectionConfig,
) -> Result<(ConnectionConfig, Option<Tunnel>), DriverError> {
    let ws_cfg: WebSocketTunnelConfig = config
        .websocket_tunnel
        .clone()
        .filter(|s| s.enabled)
        .ok_or_else(|| {
            DriverError::InvalidConfig(
                "tunnelKind=websocket but websocketTunnel is missing or disabled".into(),
            )
        })?;

    let remote_host = config.host.as_deref().ok_or_else(|| {
        DriverError::InvalidConfig("WebSocket tunnel requires a database host".into())
    })?;
    let remote_port = config.port.ok_or_else(|| {
        DriverError::InvalidConfig("WebSocket tunnel requires a database port".into())
    })?;

    tracing::info!(
        url = %ws_cfg.url,
        remote = %format!("{remote_host}:{remote_port}"),
        mode = %ws_cfg.mode,
        "Starting WebSocket tunnel"
    );

    let tunnel = WebSocketTunnel::start(&ws_cfg, remote_host, remote_port).await?;
    Ok((
        rewrite_to_local(config, tunnel.local_port()),
        Some(Tunnel::WebSocket(tunnel)),
    ))
}

async fn start_ssh(
    config: ConnectionConfig,
    known_hosts_path: &Path,
) -> Result<(ConnectionConfig, Option<Tunnel>), DriverError> {
    let ssh = config
        .ssh_tunnel
        .as_ref()
        .filter(|s| s.enabled)
        .ok_or_else(|| {
            DriverError::InvalidConfig("tunnelKind=ssh but sshTunnel is missing or disabled".into())
        })?;

    let remote_host = config
        .host
        .as_deref()
        .ok_or_else(|| DriverError::InvalidConfig("SSH tunnel requires a database host".into()))?;
    let remote_port = config
        .port
        .ok_or_else(|| DriverError::InvalidConfig("SSH tunnel requires a database port".into()))?;

    tracing::info!(
        ssh_host = %ssh.host,
        ssh_port = ssh.port,
        remote = %format!("{remote_host}:{remote_port}"),
        "Starting SSH tunnel"
    );

    let tunnel = SshTunnel::start(ssh, remote_host, remote_port, known_hosts_path).await?;
    Ok((
        rewrite_to_local(config, tunnel.local_port()),
        Some(Tunnel::Ssh(tunnel)),
    ))
}

async fn start_http_proxy(
    config: ConnectionConfig,
) -> Result<(ConnectionConfig, Option<Tunnel>), DriverError> {
    let proxy: HttpProxyTunnelConfig = config
        .http_proxy_tunnel
        .clone()
        .filter(|s| s.enabled)
        .ok_or_else(|| {
            DriverError::InvalidConfig(
                "tunnelKind=httpProxy but httpProxyTunnel is missing or disabled".into(),
            )
        })?;

    let remote_host = config.host.as_deref().ok_or_else(|| {
        DriverError::InvalidConfig("HTTP proxy tunnel requires a database host".into())
    })?;
    let remote_port = config.port.ok_or_else(|| {
        DriverError::InvalidConfig("HTTP proxy tunnel requires a database port".into())
    })?;

    tracing::info!(
        proxy = %format!("{}://{}:{}", proxy.scheme, proxy.host, proxy.port),
        remote = %format!("{remote_host}:{remote_port}"),
        "Starting HTTP CONNECT tunnel"
    );

    let tunnel = HttpProxyTunnel::start(&proxy, remote_host, remote_port).await?;
    Ok((
        rewrite_to_local(config, tunnel.local_port()),
        Some(Tunnel::HttpProxy(tunnel)),
    ))
}

fn rewrite_to_local(mut config: ConnectionConfig, local_port: u16) -> ConnectionConfig {
    config.host = Some("127.0.0.1".to_string());
    config.port = Some(local_port);
    // Prevent drivers / nested logic from trying to start another tunnel.
    config.tunnel_kind = Some(TunnelKind::None);
    config.ssh_tunnel = None;
    config.http_proxy_tunnel = None;
    config.websocket_tunnel = None;
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::SshTunnelConfig;

    fn base_config() -> ConnectionConfig {
        ConnectionConfig {
            id: "id".into(),
            name: "n".into(),
            database_type: "postgresql".into(),
            host: Some("db.internal".into()),
            port: Some(5432),
            database: None,
            schema: None,
            username: None,
            password: None,
            ssl_mode: Default::default(),
            connection_timeout: 30,
            max_pool_size: 10,
            ssh_tunnel: None,
            http_proxy_tunnel: None,
            websocket_tunnel: None,
            tunnel_kind: None,
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
    fn resolve_kind_prefers_explicit_tunnel_kind() {
        let mut c = base_config();
        c.tunnel_kind = Some(TunnelKind::HttpProxy);
        c.ssh_tunnel = Some(SshTunnelConfig {
            enabled: true,
            host: "jump".into(),
            port: 22,
            username: "u".into(),
            auth_method: "password".into(),
            password: None,
            private_key_path: None,
            passphrase: None,
            jump: None,
        });
        assert_eq!(resolve_tunnel_kind(&c), TunnelKind::HttpProxy);
    }

    #[test]
    fn resolve_kind_falls_back_to_enabled_ssh() {
        let mut c = base_config();
        c.ssh_tunnel = Some(SshTunnelConfig {
            enabled: true,
            host: "jump".into(),
            port: 22,
            username: "u".into(),
            auth_method: "password".into(),
            password: None,
            private_key_path: None,
            passphrase: None,
            jump: None,
        });
        assert_eq!(resolve_tunnel_kind(&c), TunnelKind::Ssh);
    }

    #[test]
    fn resolve_kind_none_when_empty() {
        assert_eq!(resolve_tunnel_kind(&base_config()), TunnelKind::None);
    }
}
