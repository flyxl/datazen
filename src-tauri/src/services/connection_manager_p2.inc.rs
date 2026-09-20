    pub async fn get_or_connect_session(
        &self,
        connection_id: &str,
    ) -> Result<String, ConnectionError> {
        let lock = {
            let mut locks = self
                .connect_locks
                .lock()
                .map_err(|e| ConnectionError::Internal(format!("connect lock poisoned: {e}")))?;
            locks
                .entry(connection_id.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };

        let _guard = lock.lock().await;

        {
            let owner_map = self.session_owner_map.read().await;
            let connections = self.connections.read().await;
            for (session_id, owner_connection_id) in owner_map.iter() {
                if owner_connection_id == connection_id && connections.contains_key(session_id) {
                    let mut refs = self.ref_counts.write().await;
                    *refs.entry(session_id.clone()).or_insert(0) += 1;
                    tracing::debug!(db_session_id = %session_id, refs = refs[session_id], "session ref acquired (reuse)");
                    return Ok(session_id.clone());
                }
            }
        }
        let db_session_id = self.connect(connection_id).await?;
        let mut refs = self.ref_counts.write().await;
        *refs.entry(db_session_id.clone()).or_insert(0) += 1;
        tracing::debug!(db_session_id = %db_session_id, refs = refs[&db_session_id], "session ref acquired (new)");
        Ok(db_session_id)
    }

    pub async fn release(&self, db_session_id: &str) -> Result<bool, ConnectionError> {
        let should_disconnect = {
            let mut refs = self.ref_counts.write().await;
            if let Some(count) = refs.get_mut(db_session_id) {
                *count = count.saturating_sub(1);
                tracing::debug!(db_session_id = %db_session_id, refs = *count, "session ref released");
                if *count == 0 {
                    refs.remove(db_session_id);
                    true
                } else {
                    false
                }
            } else {
                true
            }
        };
        if should_disconnect {
            self.disconnect(db_session_id).await?;
        }
        Ok(should_disconnect)
    }

    pub async fn disconnect(&self, db_session_id: &str) -> Result<(), ConnectionError> {
        self.ref_counts.write().await.remove(db_session_id);
        self.session_owner_map.write().await.remove(db_session_id);

        let mut connections = self.connections.write().await;

        if let Some(active) = connections.remove(db_session_id) {
            if let Some(driver) = self.registry.get(&active.config.database_type).await {
                let _ = driver.disconnect(active.handle).await;
            }
        }

        Ok(())
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) async fn ref_count(&self, db_session_id: &str) -> usize {
        self.ref_counts
            .read()
            .await
            .get(db_session_id)
            .copied()
            .unwrap_or(0)
    }

    pub async fn get_session(
        &self,
        db_session_id: &str,
    ) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle), ConnectionError> {
        {
            let mut connections = self.connections.write().await;
            if let Some(active) = connections.get_mut(db_session_id) {
                active.last_used = Instant::now();

                let driver = self
                    .registry
                    .get(&active.config.database_type)
                    .await
                    .ok_or_else(|| {
                        ConnectionError::DriverNotFound(active.config.database_type.clone())
                    })?;

                return Ok((driver, active.handle.clone()));
            }
        }

        self.reconnect(db_session_id).await
    }

    pub async fn resolve_session_for_connection(
        &self,
        connection_id: &str,
    ) -> Result<(String, Arc<dyn DatabaseDriver>, ConnectionHandle), ConnectionError> {
        let db_session_id = self.get_or_connect_session(connection_id).await?;
        let (driver, handle) = self.get_session(&db_session_id).await?;
        Ok((db_session_id, driver, handle))
    }

    async fn reconnect(
        &self,
        db_session_id: &str,
    ) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle), ConnectionError> {
        let owner_connection_id = {
            let map = self.session_owner_map.read().await;
            map.get(db_session_id).cloned()
        };

        let connection_id = owner_connection_id
            .ok_or_else(|| ConnectionError::DbSessionNotFound(db_session_id.to_string()))?;

        let config = self
            .store
            .get_connection(&connection_id)
            .await
            .ok_or_else(|| ConnectionError::ConnectionConfigNotFound(connection_id.clone()))?;

        tracing::info!(db_session_id = %db_session_id, %connection_id, name = %config.name, "Auto-reconnecting evicted session");

        let (effective_config, tunnel) = self.maybe_start_tunnel(config).await?;

        let driver = self
            .registry
            .get(&effective_config.database_type)
            .await
            .ok_or(ConnectionError::DriverNotFound(
                effective_config.database_type.clone(),
            ))?;

        let mut handle = driver.connect(&effective_config).await?;
        handle.id = db_session_id.to_string();

        let mut connections = self.connections.write().await;
        connections.insert(
            db_session_id.to_string(),
            ActiveSession {
                handle: handle.clone(),
                config: effective_config,
                created_at: Instant::now(),
                last_used: Instant::now(),
                _tunnel: tunnel,
            },
        );

        tracing::info!(db_session_id = %db_session_id, "Auto-reconnect succeeded");
        Ok((driver, handle))
    }

    pub async fn get_session_config(
        &self,
        db_session_id: &str,
    ) -> Result<ConnectionConfig, ConnectionError> {
        let connections = self.connections.read().await;
        let active = connections
            .get(db_session_id)
            .ok_or_else(|| ConnectionError::DbSessionNotFound(db_session_id.to_string()))?;
        Ok(active.config.clone())
    }

    pub async fn set_active_database(
        &self,
        db_session_id: &str,
        database: &str,
    ) -> Result<(), ConnectionError> {
        let mut connections = self.connections.write().await;
        let active = connections
            .get_mut(db_session_id)
            .ok_or_else(|| ConnectionError::DbSessionNotFound(db_session_id.to_string()))?;
        active.config.database = Some(database.to_string());
        Ok(())
    }

    pub async fn list_sessions(&self) -> Vec<String> {
        self.connections.read().await.keys().cloned().collect()
    }

    pub async fn get_server_info(
        &self,
        db_session_id: &str,
    ) -> Result<ServerInfo, ConnectionError> {
        let (driver, handle) = self.get_session(db_session_id).await?;
        driver
            .get_server_info(&handle)
            .await
            .map_err(ConnectionError::DriverError)
    }

