#[cfg(test)]
use super::ActiveSession;
use super::{ConnectionError, ConnectionManager};
use crate::db::ConnectionConfig;
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

    async fn resolve_tunnel_ref(
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
                    let _ = driver.disconnect(active.handle).await;
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
        for session_id in session_ids {
            let _ = self.disconnect(&session_id).await;
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
