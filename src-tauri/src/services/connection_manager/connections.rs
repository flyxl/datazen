use super::{ActiveSession, ConnectionError, ConnectionManager};
use crate::db::{ConnectionConfig, ConnectionHandle, DatabaseDriver, DatabaseType, ServerInfo};
use crate::tunnel::Tunnel;
use std::sync::Arc;
use std::time::Instant;

impl ConnectionManager {
    pub async fn connect(&self, connection_id: &str) -> Result<String, ConnectionError> {
        self.connect_with_config(connection_id, None).await
    }

    pub async fn connect_dedicated(
        &self,
        connection_id: &str,
        database: Option<&str>,
    ) -> Result<String, ConnectionError> {
        let db_session_id = self.connect_with_config(connection_id, database).await?;
        let mut refs = self.ref_counts.write().await;
        *refs.entry(db_session_id.clone()).or_insert(0) += 1;
        tracing::debug!(db_session_id = %db_session_id, refs = refs[&db_session_id], "dedicated session ref acquired");
        Ok(db_session_id)
    }

    async fn connect_with_config(
        &self,
        connection_id: &str,
        database_override: Option<&str>,
    ) -> Result<String, ConnectionError> {
        let (driver, handle, mut effective_config, tunnel) = self
            .establish_connection(connection_id, database_override)
            .await?;
        let db_session_id = handle.id.clone();

        if effective_config.server_version.is_none() {
            if let Ok(info) = driver.get_server_info(&handle).await {
                effective_config.server_version = Some(info.server_version.clone());
                if let Some(mut stored) = self.store.get_connection(connection_id).await {
                    stored.server_version = Some(info.server_version);
                    let _ = self.store.save_connection(stored).await;
                }
            }
        }

        self.session_owner_map
            .write()
            .await
            .insert(db_session_id.clone(), connection_id.to_string());
        self.connections.write().await.insert(
            db_session_id.clone(),
            ActiveSession {
                handle,
                config: effective_config,
                created_at: Instant::now(),
                last_used: Instant::now(),
                tunnel,
            },
        );
        Ok(db_session_id)
    }

    pub(crate) async fn establish_connection(
        &self,
        connection_id: &str,
        database_override: Option<&str>,
    ) -> Result<
        (
            Arc<dyn DatabaseDriver>,
            ConnectionHandle,
            ConnectionConfig,
            Option<Tunnel>,
        ),
        ConnectionError,
    > {
        let config = self
            .store
            .get_connection(connection_id)
            .await
            .ok_or_else(|| ConnectionError::ConnectionConfigNotFound(connection_id.to_string()))?;
        let (mut effective_config, tunnel) = self.start_tunnel(config).await?;
        if let Some(db) = database_override.map(str::trim).filter(|s| !s.is_empty()) {
            effective_config.database = Some(db.to_string());
        }
        effective_config.max_pool_size = crate::store::clamp_connection_pool_size(
            self.store.get_settings().await.connection_pool_size,
        );
        let driver = self
            .driver_for_type(&effective_config.database_type)
            .await?;
        let handle = driver.connect(&effective_config).await?;
        Ok((driver, handle, effective_config, tunnel))
    }

    pub(crate) async fn driver_for_type(
        &self,
        database_type: &DatabaseType,
    ) -> Result<Arc<dyn DatabaseDriver>, ConnectionError> {
        self.registry
            .get(database_type)
            .await
            .ok_or_else(|| ConnectionError::DriverNotFound(database_type.clone()))
    }

    pub async fn owner_connection_id(&self, db_session_id: &str) -> Option<String> {
        self.session_owner_map
            .read()
            .await
            .get(db_session_id)
            .cloned()
    }

    pub(crate) async fn server_info(
        &self,
        db_session_id: &str,
    ) -> Result<ServerInfo, ConnectionError> {
        let (driver, handle) = self.get_session(db_session_id).await?;
        driver
            .get_server_info(&handle)
            .await
            .map_err(ConnectionError::DriverError)
    }
}
