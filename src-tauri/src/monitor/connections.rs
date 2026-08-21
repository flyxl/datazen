//! Monitor-only connection registry — never shares UI session pools.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::RwLock;

use crate::db::{ConnectionConfig, ConnectionHandle, DatabaseDriver};
use crate::services::connection_manager::{ConnectionError, ConnectionManager};
use crate::ssh_tunnel::SshTunnel;

/// Logical registry key for a monitor connection (`monitor:{config_id}`).
#[cfg_attr(not(test), allow(dead_code))]
pub fn monitor_registry_key(config_id: &str) -> String {
    format!("monitor:{config_id}")
}

struct MonitorEntry {
    driver: Arc<dyn DatabaseDriver>,
    handle: ConnectionHandle,
    #[allow(dead_code)]
    config: ConnectionConfig,
    last_used: Instant,
    _tunnel: Option<SshTunnel>,
}

/// Holds monitor connections keyed by `config_id` (logical key `monitor:{config_id}`).
///
/// Uses [`ConnectionManager::establish_connection`] so handles are never inserted
/// into the UI `config_id_map`. Each monitor entry gets its own driver pool instance.
pub struct MonitorConnectionRegistry {
    connection_manager: Arc<ConnectionManager>,
    entries: Arc<RwLock<HashMap<String, MonitorEntry>>>,
    connect_locks: std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl MonitorConnectionRegistry {
    pub fn new(connection_manager: Arc<ConnectionManager>) -> Self {
        Self {
            connection_manager,
            entries: Arc::new(RwLock::new(HashMap::new())),
            connect_locks: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Return an existing monitor handle for `config_id`, or establish a new one.
    pub async fn get_or_connect_monitor(
        &self,
        config_id: &str,
    ) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle), ConnectionError> {
        let lock = {
            let mut locks = self.connect_locks.lock().unwrap();
            locks
                .entry(config_id.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        let _guard = lock.lock().await;

        {
            let mut entries = self.entries.write().await;
            if let Some(entry) = entries.get_mut(config_id) {
                entry.last_used = Instant::now();
                return Ok((entry.driver.clone(), entry.handle.clone()));
            }
        }

        let (driver, handle, config, tunnel) = self
            .connection_manager
            .establish_connection(config_id)
            .await?;

        self.entries.write().await.insert(
            config_id.to_string(),
            MonitorEntry {
                driver: driver.clone(),
                handle: handle.clone(),
                config,
                last_used: Instant::now(),
                _tunnel: tunnel,
            },
        );

        Ok((driver, handle))
    }

    /// Disconnect and remove the monitor connection for `config_id`, if present.
    pub async fn disconnect_monitor(&self, config_id: &str) -> Result<(), ConnectionError> {
        let entry = self.entries.write().await.remove(config_id);
        if let Some(entry) = entry {
            let _ = entry.driver.disconnect(entry.handle).await;
        }
        Ok(())
    }

    pub async fn shutdown(&self) {
        let keys: Vec<String> = self.entries.read().await.keys().cloned().collect();
        for config_id in keys {
            let _ = self.disconnect_monitor(&config_id).await;
        }
    }

    #[cfg(test)]
    async fn insert_test_entry(
        &self,
        config_id: &str,
        handle: ConnectionHandle,
        database_type: &str,
    ) {
        use crate::db::{ConnectionConfig, SslMode};

        struct StubDriver(String);

        #[async_trait::async_trait]
        impl DatabaseDriver for StubDriver {
            fn driver_type(&self) -> crate::db::DatabaseType {
                self.0.clone()
            }

            async fn connect(
                &self,
                _config: &ConnectionConfig,
            ) -> Result<ConnectionHandle, crate::db::DriverError> {
                Err(crate::db::DriverError::QueryFailed("stub".into()))
            }

            async fn test_connection(
                &self,
                _config: &ConnectionConfig,
            ) -> Result<crate::db::ServerInfo, crate::db::DriverError> {
                Err(crate::db::DriverError::QueryFailed("stub".into()))
            }

            async fn disconnect(
                &self,
                _handle: ConnectionHandle,
            ) -> Result<(), crate::db::DriverError> {
                Ok(())
            }

            async fn get_databases(
                &self,
                _handle: &ConnectionHandle,
            ) -> Result<Vec<String>, crate::db::DriverError> {
                Ok(vec![])
            }

            async fn get_tables(
                &self,
                _handle: &ConnectionHandle,
                _database: &str,
            ) -> Result<Vec<crate::db::TableInfo>, crate::db::DriverError> {
                Ok(vec![])
            }

            async fn get_table_schema(
                &self,
                _handle: &ConnectionHandle,
                _table: &str,
            ) -> Result<crate::db::TableSchema, crate::db::DriverError> {
                Err(crate::db::DriverError::QueryFailed("stub".into()))
            }

            async fn query(
                &self,
                _handle: &ConnectionHandle,
                _sql: &str,
            ) -> Result<crate::db::QueryResult, crate::db::DriverError> {
                Err(crate::db::DriverError::QueryFailed("stub".into()))
            }

            async fn query_multi(
                &self,
                _handle: &ConnectionHandle,
                _sql: &str,
                _limit: Option<u32>,
            ) -> Result<crate::db::MultiQueryResult, crate::db::DriverError> {
                Err(crate::db::DriverError::QueryFailed("stub".into()))
            }

            async fn query_with_params(
                &self,
                _handle: &ConnectionHandle,
                _sql: &str,
                _params: &[crate::db::Value],
            ) -> Result<crate::db::QueryResult, crate::db::DriverError> {
                Err(crate::db::DriverError::QueryFailed("stub".into()))
            }

            async fn execute(
                &self,
                _handle: &ConnectionHandle,
                _sql: &str,
            ) -> Result<u64, crate::db::DriverError> {
                Ok(0)
            }

            async fn cancel_query(
                &self,
                _handle: &ConnectionHandle,
            ) -> Result<(), crate::db::DriverError> {
                Ok(())
            }
        }

        self.entries.write().await.insert(
            config_id.to_string(),
            MonitorEntry {
                driver: Arc::new(StubDriver(database_type.to_string())),
                handle,
                config: ConnectionConfig {
                    id: config_id.to_string(),
                    name: "test".into(),
                    database_type: database_type.into(),
                    host: None,
                    port: None,
                    database: None,
                    schema: None,
                    username: None,
                    password: None,
                    ssl_mode: SslMode::Prefer,
                    connection_timeout: 30,
                    max_pool_size: 10,
                    ssh_tunnel: None,
                    color_tag: None,
                    group: None,
                    last_connected_at: None,
                    server_version: None,
                    options: None,
                    read_only: false,
                    pinned: false,
                },
                last_used: Instant::now(),
                _tunnel: None,
            },
        );
    }

    #[cfg(test)]
    async fn has_entry(&self, config_id: &str) -> bool {
        self.entries.read().await.contains_key(config_id)
    }

    #[cfg(test)]
    async fn entry_pool_id(&self, config_id: &str) -> Option<String> {
        self.entries
            .read()
            .await
            .get(config_id)
            .map(|e| e.handle.pool_id.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::registry::DriverRegistry;
    use crate::store::Store;

    async fn test_registry() -> MonitorConnectionRegistry {
        std::env::set_var("DATAZEN_KEYRING", "file");
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::init_with_path(dir.path()).await.unwrap());
        let driver_registry = Arc::new(DriverRegistry::new());
        let connection_manager = Arc::new(ConnectionManager::new(driver_registry, store));
        MonitorConnectionRegistry::new(connection_manager)
    }

    #[test]
    fn monitor_registry_key_prefixes_config_id() {
        assert_eq!(monitor_registry_key("cfg-abc"), "monitor:cfg-abc");
    }

    #[tokio::test]
    async fn get_or_connect_reuses_existing_handle_pool_id() {
        let registry = test_registry().await;
        let handle = ConnectionHandle {
            id: "mon-h1".into(),
            pool_id: "mon-pool-1".into(),
        };
        registry
            .insert_test_entry("cfg-1", handle.clone(), "postgres")
            .await;

        let (_, returned) = registry.get_or_connect_monitor("cfg-1").await.unwrap();
        assert_eq!(returned.pool_id, "mon-pool-1");
        assert_eq!(returned.id, "mon-h1");
    }

    #[tokio::test]
    async fn disconnect_monitor_removes_entry() {
        let registry = test_registry().await;
        let handle = ConnectionHandle {
            id: "mon-h2".into(),
            pool_id: "mon-pool-2".into(),
        };
        registry
            .insert_test_entry("cfg-2", handle, "postgres")
            .await;
        assert!(registry.has_entry("cfg-2").await);

        registry.disconnect_monitor("cfg-2").await.unwrap();
        assert!(!registry.has_entry("cfg-2").await);
    }

    #[tokio::test]
    async fn monitor_and_ui_handles_use_different_pool_ids() {
        let registry = test_registry().await;
        let monitor_handle = ConnectionHandle {
            id: "mon-h3".into(),
            pool_id: "monitor-pool".into(),
        };
        let ui_handle = ConnectionHandle {
            id: "ui-h3".into(),
            pool_id: "ui-pool".into(),
        };

        registry
            .insert_test_entry("cfg-3", monitor_handle.clone(), "postgres")
            .await;

        assert_ne!(monitor_handle.pool_id, ui_handle.pool_id);
        assert_eq!(
            registry.entry_pool_id("cfg-3").await.as_deref(),
            Some("monitor-pool")
        );
    }

    #[tokio::test]
    async fn monitor_registry_does_not_touch_ui_config_id_map() {
        std::env::set_var("DATAZEN_KEYRING", "file");
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::init_with_path(dir.path()).await.unwrap());
        let driver_registry = Arc::new(DriverRegistry::new());
        let connection_manager = Arc::new(ConnectionManager::new(driver_registry, store));
        let registry = MonitorConnectionRegistry::new(connection_manager.clone());

        registry
            .insert_test_entry(
                "cfg-4",
                ConnectionHandle {
                    id: "mon-h4".into(),
                    pool_id: "mon-pool-4".into(),
                },
                "postgres",
            )
            .await;

        assert_eq!(connection_manager.ui_session_map_len().await, 0);
    }
}
