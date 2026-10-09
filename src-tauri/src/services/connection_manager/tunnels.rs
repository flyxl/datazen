#[cfg(test)]
use super::ActiveSession;
use super::{ConnectionError, ConnectionManager};
use crate::db::{ConnectionConfig, SslMode, TunnelKind};
use crate::tunnel::Tunnel;
use std::sync::Arc;
use std::time::Instant;
use tokio::time::{interval, Duration};

impl ConnectionManager {
    pub(super) async fn start_tunnel(
        &self,
        config: ConnectionConfig,
    ) -> Result<(ConnectionConfig, Option<Tunnel>), ConnectionError> {
        let config = self.resolve_tunnel_ref(config).await?;
        let known_hosts_path = self.store.data_dir().join("ssh_known_hosts.json");
        crate::tunnel::start_for_connection(config, &known_hosts_path)
            .await
            .map_err(ConnectionError::DriverError)
    }

    pub(super) async fn resolve_tunnel_ref(
        &self,
        mut config: ConnectionConfig,
    ) -> Result<ConnectionConfig, ConnectionError> {
        let Some(tid) = config.tunnel_id.as_ref().filter(|s| !s.is_empty()) else {
            return Ok(config);
        };
        let Some(saved) = self.store.get_tunnel(tid).await else {
            return Err(ConnectionError::Internal(format!(
                "tunnel id '{tid}' not found"
            )));
        };
        config.tunnel_kind = Some(saved.kind);
        config.ssh_tunnel = saved.ssh;
        config.http_proxy_tunnel = saved.http_proxy;
        config.websocket_tunnel = saved.websocket;
        Ok(config)
    }

    pub async fn test_connection(
        &self,
        config: &ConnectionConfig,
    ) -> Result<crate::db::ServerInfo, ConnectionError> {
        let (effective_config, _tunnel) = self.start_tunnel(config.clone()).await?;
        let driver = self
            .registry
            .get(&effective_config.database_type)
            .await
            .ok_or_else(|| {
                ConnectionError::DriverNotFound(effective_config.database_type.clone())
            })?;
        driver
            .test_connection(&effective_config)
            .await
            .map_err(ConnectionError::DriverError)
    }

    /// Establish the tunnel referenced by `tunnel_id` toward
    /// `target_host:target_port`, probe its upstream leg, tear it down
    /// immediately, and report how long that took.
    ///
    /// Backs the standalone tunnel connectivity probe: the management UI can
    /// validate one saved tunnel without opening a database session. Only the
    /// tunnel fields plus the forwarded target reach the tunnel runtime, so the
    /// synthetic config's `database_type` is a placeholder and no driver is ever
    /// contacted.
    ///
    /// `SshTunnel::start` dials the bastion and authenticates, but
    /// `HttpProxyTunnel::start` / `WebSocketTunnel::start` are lazy — they only
    /// bind the local listener — so those kinds are verified explicitly.
    /// Otherwise an unreachable proxy/relay would be reported as reachable.
    pub async fn test_tunnel(
        &self,
        tunnel_id: &str,
        target_host: &str,
        target_port: u16,
    ) -> Result<Duration, ConnectionError> {
        let Some(saved) = self.store.get_tunnel(tunnel_id).await else {
            return Err(ConnectionError::Internal(format!(
                "tunnel id '{tunnel_id}' not found"
            )));
        };

        let config = ConnectionConfig {
            id: format!("__tunnel_test__{tunnel_id}"),
            name: format!("tunnel test: {tunnel_id}"),
            // Never dialed: `start_tunnel` only forwards `host`/`port` through
            // the tunnel and does not resolve a driver here.
            database_type: "postgresql".to_string(),
            host: Some(target_host.to_string()),
            port: Some(target_port),
            database: None,
            schema: None,
            username: None,
            password: None,
            ssl_mode: SslMode::default(),
            connection_timeout: 30,
            max_pool_size: 10,
            ssh_tunnel: None,
            tunnel_kind: None,
            tunnel_id: Some(tunnel_id.to_string()),
            http_proxy_tunnel: None,
            websocket_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
            pinned: false,
        };

        let started = Instant::now();
        let (_resolved, tunnel) = self.start_tunnel(config).await?;
        let Some(tunnel) = tunnel else {
            // `tunnelKind = none` (or a tunnel that resolves to nothing) means
            // nothing was probed; reporting success would be a false positive.
            return Err(ConnectionError::Internal(format!(
                "tunnel '{tunnel_id}' resolved to no tunnel configuration"
            )));
        };

        let verified: Result<(), ConnectionError> = match saved.kind {
            TunnelKind::HttpProxy => {
                let proxy = saved.http_proxy.ok_or_else(|| {
                    ConnectionError::Internal(format!(
                        "tunnel '{tunnel_id}' has no httpProxy config"
                    ))
                })?;
                crate::tunnel::verify_http_proxy_upstream(&proxy, target_host, target_port)
                    .await
                    .map_err(ConnectionError::DriverError)
            }
            TunnelKind::WebSocket => {
                let websocket = saved.websocket.ok_or_else(|| {
                    ConnectionError::Internal(format!(
                        "tunnel '{tunnel_id}' has no websocket config"
                    ))
                })?;
                crate::tunnel::verify_websocket_upstream(&websocket, target_host, target_port)
                    .await
                    .map_err(ConnectionError::DriverError)
            }
            // `SshTunnel::start` already dials the bastion and authenticates, so
            // building the tunnel *is* the probe. `None` never reaches here: it
            // yields no tunnel and we bail out above.
            TunnelKind::Ssh | TunnelKind::None => Ok(()),
        };

        let elapsed = started.elapsed();
        // Dropping the tunnel tears the local forwarder down (see `SshTunnel`'s
        // `Drop`): the probe is deliberately one-shot and must not leak a
        // listener, a task or an SSH session.
        drop(tunnel);
        verified?;
        Ok(elapsed)
    }

    pub async fn ping(&self, db_session_id: &str) -> bool {
        let mut connections = self.connections.write().await;
        if let Some(active) = connections.get_mut(db_session_id) {
            active.last_used = Instant::now();
            true
        } else {
            false
        }
    }

    pub async fn cleanup_idle_connections(&self) {
        let now = Instant::now();
        let to_remove = {
            let connections = self.connections.read().await;
            let refs = self.ref_counts.read().await;
            connections
                .iter()
                .filter(|(id, active)| {
                    refs.get(*id).copied().unwrap_or(0) == 0
                        && now.duration_since(active.last_used) > self.idle_timeout
                })
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>()
        };
        for id in to_remove {
            // Evict the live session but keep its `session_owner_map` entry:
            // that entry is what lets `reconnect()` rebuild the session under
            // the original `db_session_id`. Routing through `disconnect()`
            // here would clear it and permanently lose the auto-reconnect path.
            let active = self.connections.write().await.remove(&id);
            if let Some(active) = active {
                tracing::info!(
                    db_session_id = %id,
                    name = %active.config.name,
                    "Evicting idle db session (session_owner_map entry kept for auto-reconnect)"
                );
                if let Some(driver) = self.registry.get(&active.config.database_type).await {
                    // Idle eviction is a background sweep with no user action
                    // and no IPC caller to hand an error to, so the failure
                    // cannot be propagated — returning it would abort the
                    // ticker loop started by `spawn_idle_cleanup` and silently
                    // stop eviction for the rest of the process. It is logged
                    // rather than swallowed so the leak is diagnosable: the
                    // physical connection is still up, its `pool_id` is now
                    // unreachable, and `session_owner_map` is kept, so the next
                    // `reconnect` builds a *second* connection beside the
                    // orphan. (That retry-on-reconnect behaviour is the
                    // deliberate trade-off documented above; the unlogged
                    // variant of it was the actual defect.)
                    let pool_id = active.handle.pool_id.clone();
                    if let Err(e) = driver.disconnect(active.handle).await {
                        tracing::warn!(
                            db_session_id = %id,
                            pool_id = %pool_id,
                            name = %active.config.name,
                            error = %e,
                            "Idle eviction could not confirm the physical teardown; \
                             the connection is left up and unreachable under this \
                             db_session_id",
                        );
                    }
                }
            }
        }
    }

    pub fn spawn_idle_cleanup(self: &Arc<Self>) {
        let manager = Arc::clone(self);
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(60));
            loop {
                ticker.tick().await;
                manager.cleanup_idle_connections().await;
            }
        });
    }

    pub fn start_cleanup_task(self: Arc<Self>) {
        self.spawn_idle_cleanup();
    }

    pub async fn shutdown(&self) {
        let session_ids = self
            .connections
            .read()
            .await
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        // `shutdown` runs from `RunEvent::ExitRequested`
        // (`bootstrap/run.rs:584`) inside `block_on`, on the way out of the
        // process: there is no caller left to return a `Result` to, and
        // aborting the loop on the first failure would strand every remaining
        // session. So the errors are collected and summarised instead of
        // discarded one by one — a teardown that could not be confirmed is
        // exactly the thing a post-mortem log is for, and the previous
        // `let _ =` made it invisible.
        let mut unconfirmed = Vec::new();
        for session_id in session_ids {
            if let Err(e) = self.disconnect(&session_id).await {
                tracing::warn!(
                    db_session_id = %session_id,
                    error = %e,
                    "Shutdown teardown could not be confirmed",
                );
                unconfirmed.push(session_id);
            }
        }
        if !unconfirmed.is_empty() {
            tracing::warn!(
                count = unconfirmed.len(),
                db_session_ids = ?unconfirmed,
                "Shutdown finished with unconfirmed physical teardowns",
            );
        }
    }

    #[cfg(test)]
    pub(crate) async fn session_owner_map_len(&self) -> usize {
        self.session_owner_map.read().await.len()
    }

    #[cfg(test)]
    pub(crate) async fn ref_count(&self, db_session_id: &str) -> usize {
        self.ref_counts
            .read()
            .await
            .get(db_session_id)
            .copied()
            .unwrap_or(0)
    }

    #[cfg(test)]
    pub(crate) async fn insert_test_session(
        &self,
        db_session_id: &str,
        connection_id: &str,
        config: ConnectionConfig,
        handle: crate::db::ConnectionHandle,
    ) {
        self.session_owner_map
            .write()
            .await
            .insert(db_session_id.to_string(), connection_id.to_string());
        self.connections.write().await.insert(
            db_session_id.to_string(),
            ActiveSession {
                handle,
                identity_config: config.clone(),
                config,
                created_at: Instant::now(),
                last_used: Instant::now(),
                tunnel: None,
            },
        );
    }

    #[cfg(test)]
    pub(crate) async fn expire_test_session(&self, db_session_id: &str) {
        let mut connections = self.connections.write().await;
        if let Some(active) = connections.get_mut(db_session_id) {
            active.last_used = Instant::now()
                .checked_sub(self.idle_timeout + Duration::from_secs(1))
                .unwrap_or_else(Instant::now);
        }
    }
}
