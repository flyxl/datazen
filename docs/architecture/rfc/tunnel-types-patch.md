# Required types.rs patch (apply on branch)

## 1. After `ssh_tunnel` field in `ConnectionConfig`, add:

```rust
    #[serde(default)]
    pub tunnel_kind: Option<TunnelKind>,
    #[serde(default)]
    pub http_proxy_tunnel: Option<HttpProxyTunnelConfig>,
    #[serde(default)]
    pub websocket_tunnel: Option<WebSocketTunnelConfig>,
```

## 2. In `dummy_connection_config` and all `ConnectionConfig` literals, add after `ssh_tunnel: None`:

```rust
            tunnel_kind: None,
            http_proxy_tunnel: None,
            websocket_tunnel: None,
```

## 3. In `DriverError` after `SshTunnelError`:

```rust
    #[error("HTTP proxy tunnel error: {0}")]
    HttpProxyTunnelError(String),
    #[error("WebSocket tunnel error: {0}")]
    WebSocketTunnelError(String),
```

`TunnelKind` / `HttpProxyTunnelConfig` / `WebSocketTunnelConfig` are already in `packages/driver-api/src/tunnel_types.rs` and re-exported from `lib.rs`.

## 4. ConnectionManager

Replace `maybe_start_tunnel` body with:

```rust
    async fn maybe_start_tunnel(
        &self,
        config: ConnectionConfig,
    ) -> Result<(ConnectionConfig, Option<Tunnel>), ConnectionError> {
        let known_hosts_path = self.store.data_dir().join("ssh_known_hosts.json");
        crate::tunnel::start_for_connection(config, &known_hosts_path)
            .await
            .map_err(ConnectionError::DriverError)
    }
```

And change `SshTunnel` imports/types to `crate::tunnel::Tunnel`.
