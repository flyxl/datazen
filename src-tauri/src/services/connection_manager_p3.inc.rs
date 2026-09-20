    async fn maybe_start_tunnel(
        &self,
        config: ConnectionConfig,
    ) -> Result<(ConnectionConfig, Option<SshTunnel>), ConnectionError> {
        let config = self.resolve_tunnel_ref(config).await?;
        let known_hosts_path = self.store.data_dir().join("ssh_known_hosts.json");
        crate::tunnel::start_for_connection(config, &known_hosts_path)
            .await
            .map_err(ConnectionError::DriverError)
    }

    /// When `tunnel_id` is set, load the SavedTunnel and inject its fields
    /// so the existing tunnel runtime path can run unchanged.
    async fn resolve_tunnel_ref(
        &self,
        mut config: ConnectionConfig,
    ) -> Result<ConnectionConfig, ConnectionError> {
        let Some(tid) = config.tunnel_id.as_ref().filter(|s| !s.is_empty()) else {
            return Ok(config);
        };
        let Some(saved) = self.store.get_tunnel(tid).await else {
            return Err(ConnectionError::Internal(format!(
                "tunnel id '{tid}' not found (connection references a deleted or missing tunnel)"
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
    ) -> Result<ServerInfo, ConnectionError> {
        let (effective_config, _tunnel) = self.maybe_start_tunnel(config.clone()).await?;
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
        let mut to_remove = Vec::new();
        {
            let connections = self.connections.read().await;
            let refs = self.ref_counts.read().await;
            for (id, active) in connections.iter() {
                let refs_n = refs.get(id).copied().unwrap_or(0);
                if refs_n == 0 && now.duration_since(active.last_used) > self.idle_timeout {
                    to_remove.push(id.clone());
                }
            }
        }
        for id in to_remove {
            tracing::info!(db_session_id = %id, "Evicting idle session");
            let _ = self.disconnect(&id).await;
        }
    }

    pub fn spawn_idle_cleanup(self: &Arc<Self>) {
        let mgr = Arc::clone(self);
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(60));
            loop {
                ticker.tick().await;
                mgr.cleanup_idle_connections().await;
            }
        });
    }

    #[cfg(test)]
    pub(crate) async fn insert_test_session(
        &self,
        db_session_id: &str,
        connection_id: &str,
        config: ConnectionConfig,
        handle: ConnectionHandle,
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
                _tunnel: None,
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
