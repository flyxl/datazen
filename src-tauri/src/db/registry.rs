//! Driver registry — resolves `DatabaseType` to a concrete `DatabaseDriver`.
//!
//! Drivers are discovered via `inventory` factories from optional path/git
//! driver crates linked into the host binary.

use datazen_driver_api::*;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Static capabilities advertised by a driver factory.
///
/// These flags describe whether the driver has a meaningful implementation;
/// they are not inferred from the presence of a trait method because the
/// trait intentionally has a default unsupported/no-op implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverCapabilities {
    pub supports_cancel_query: bool,
    pub supports_query_execution_cancel: bool,
    pub supports_explain: bool,
    pub supports_streaming_results: bool,
}

impl DriverCapabilities {
    fn from_factory(factory: &dyn DatabaseDriverFactory, driver: &dyn DatabaseDriver) -> Self {
        Self {
            supports_cancel_query: factory.supports_cancel_query(),
            supports_query_execution_cancel: factory.supports_query_execution_cancel()
                && driver.supports_query_execution_cancel(),
            supports_explain: factory.supports_explain(),
            supports_streaming_results: factory.supports_streaming_results(),
        }
    }
}

/// Holds registered drivers. Starts empty; call [`DriverRegistry::ensure_type`]
/// (or rely on [`DriverRegistry::get`]) to load a type on demand.
pub struct DriverRegistry {
    drivers: Arc<RwLock<HashMap<DatabaseType, Arc<dyn DatabaseDriver>>>>,
    kv_drivers: Arc<RwLock<HashMap<DatabaseType, Arc<dyn KeyValueDriver>>>>,
    capabilities: Arc<RwLock<HashMap<DatabaseType, DriverCapabilities>>>,
}

impl DriverRegistry {
    pub fn new() -> Self {
        Self {
            drivers: Arc::new(RwLock::new(HashMap::new())),
            kv_drivers: Arc::new(RwLock::new(HashMap::new())),
            capabilities: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Catalog of types this build can provide (inventory factories only).
    /// Does **not** instantiate any driver.
    pub fn available_types(&self) -> Vec<DatabaseType> {
        let mut types: Vec<DatabaseType> = Vec::new();
        for factory in iter_driver_factories() {
            let id = factory.driver_id().to_string();
            if !types.iter().any(|t| t == &id) {
                types.push(id);
            }
        }
        types
    }

    /// Ensure drivers for every distinct type in `types` are loaded.
    pub async fn ensure_types<I, S>(&self, types: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for t in types {
            let db_type = t.as_ref();
            if let Err(e) = self.ensure_type(db_type).await {
                tracing::warn!(db_type, error = %e, "Failed to preload driver");
            }
        }
    }

    /// Lazily construct and register the driver for `db_type` if missing.
    pub async fn ensure_type(&self, db_type: &str) -> Result<(), String> {
        {
            let drivers = self.drivers.read().await;
            if drivers.contains_key(db_type) {
                return Ok(());
            }
        }

        let mut drivers = self.drivers.write().await;
        if drivers.contains_key(db_type) {
            return Ok(());
        }

        self.register_from_inventory(db_type, &mut drivers).await?;
        tracing::info!(db_type, "Registered driver on demand");
        Ok(())
    }

    async fn register_from_inventory(
        &self,
        db_type: &str,
        drivers: &mut HashMap<DatabaseType, Arc<dyn DatabaseDriver>>,
    ) -> Result<(), String> {
        for factory in iter_driver_factories() {
            if factory.driver_id() != db_type {
                continue;
            }
            let pv = factory.protocol_version();
            if pv < datazen_driver_api::MIN_PROTOCOL_VERSION {
                return Err(format!(
                    "Plugin '{}' protocol version {} is too old (minimum {})",
                    factory.driver_id(),
                    pv,
                    datazen_driver_api::MIN_PROTOCOL_VERSION
                ));
            }
            if pv > datazen_driver_api::PROTOCOL_VERSION {
                tracing::warn!(
                    "Plugin '{}' protocol version {} is newer than host {}. Loading with possible incompatibility.",
                    factory.driver_id(),
                    pv,
                    datazen_driver_api::PROTOCOL_VERSION
                );
            } else if pv < datazen_driver_api::PROTOCOL_VERSION {
                tracing::warn!(
                    "Plugin '{}' protocol version {} < host {}. Running in degraded mode \
                     (cancel_query={}, explain={}, streaming={}).",
                    factory.driver_id(),
                    pv,
                    datazen_driver_api::PROTOCOL_VERSION,
                    factory.supports_cancel_query(),
                    factory.supports_explain(),
                    factory.supports_streaming_results(),
                );
            }

            let driver = factory.create();
            let actual = driver.driver_type();
            let capabilities = DriverCapabilities::from_factory(*factory, driver.as_ref());
            {
                let mut capability_map = self.capabilities.write().await;
                capability_map.insert(db_type.to_string(), capabilities);
                if actual != db_type {
                    capability_map.insert(actual.clone(), capabilities);
                }
            }
            if let Some(kv) = factory.create_kv() {
                let mut kv_map = self.kv_drivers.write().await;
                kv_map.insert(kv.driver_type(), kv);
            }
            if actual != db_type {
                drivers.insert(actual, driver.clone());
            }
            drivers.insert(db_type.to_string(), driver);
            return Ok(());
        }
        Err(format!("No driver available for database type '{db_type}'"))
    }

    /// Look up a driver, loading it on demand if needed.
    pub async fn get(&self, db_type: &DatabaseType) -> Option<Arc<dyn DatabaseDriver>> {
        if let Err(e) = self.ensure_type(db_type).await {
            tracing::warn!(db_type = %db_type, error = %e, "Driver ensure failed");
            return None;
        }
        let drivers = self.drivers.read().await;
        drivers.get(db_type).cloned()
    }

    /// Return the static capabilities advertised by the registered factory.
    /// `None` means the driver was injected by a test/legacy integration that
    /// does not expose capability metadata yet; callers must treat that as
    /// unknown rather than supported.
    pub async fn get_capabilities(&self, db_type: &DatabaseType) -> Option<DriverCapabilities> {
        // Keep capability lookup aligned with driver lookup: a connection-info
        // request must not depend on a separate, eagerly populated registry.
        // `ensure_type` is a no-op for legacy/test registrations, so those
        // drivers deliberately remain `None` (unknown).
        if let Err(error) = self.ensure_type(db_type).await {
            tracing::debug!(db_type = %db_type, %error, "Driver capability lookup failed");
            return None;
        }
        let capabilities = self.capabilities.read().await;
        capabilities.get(db_type).copied()
    }

    /// Types currently loaded in memory (for diagnostics). Prefer
    /// [`available_types`] for UI catalogs.
    pub async fn loaded_types(&self) -> Vec<DatabaseType> {
        let drivers = self.drivers.read().await;
        drivers.keys().cloned().collect()
    }

    pub async fn get_kv_driver(&self, db_type: &DatabaseType) -> Option<Arc<dyn KeyValueDriver>> {
        if let Err(e) = self.ensure_type(db_type).await {
            tracing::warn!(db_type = %db_type, error = %e, "KV driver ensure failed");
            return None;
        }
        let kv_drivers = self.kv_drivers.read().await;
        kv_drivers.get(db_type).cloned()
    }

    /// Look up a SQL driver by its type string, loading on demand.
    pub async fn get_sql_driver_by_name(&self, name: &str) -> Option<Arc<dyn DatabaseDriver>> {
        self.get(&name.to_string()).await
    }

    /// Register a driver instance for unit tests (bypasses inventory).
    #[cfg(any(test, feature = "test-harness"))]
    pub async fn register_test_driver(
        &self,
        db_type: impl Into<DatabaseType>,
        driver: Arc<dyn DatabaseDriver>,
    ) {
        let db_type = db_type.into();
        self.drivers.write().await.insert(db_type.clone(), driver);
        // Replacing a previously factory-registered driver with a legacy test
        // driver must not leave stale capability metadata behind.
        self.capabilities.write().await.remove(&db_type);
    }

    /// Register a test driver together with explicit capability metadata.
    #[cfg(any(test, feature = "test-harness"))]
    pub async fn register_test_driver_with_capabilities(
        &self,
        db_type: impl Into<DatabaseType>,
        driver: Arc<dyn DatabaseDriver>,
        capabilities: DriverCapabilities,
    ) {
        let db_type = db_type.into();
        self.drivers.write().await.insert(db_type.clone(), driver);
        self.capabilities
            .write()
            .await
            .insert(db_type, capabilities);
    }

    /// Register a KV driver instance for unit tests (bypasses inventory).
    #[cfg(test)]
    pub async fn register_test_kv_driver(
        &self,
        db_type: impl Into<DatabaseType>,
        kv: Arc<dyn KeyValueDriver>,
    ) {
        self.kv_drivers.write().await.insert(db_type.into(), kv);
    }
}

impl Default for DriverRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Create an empty registry. Drivers load via [`DriverRegistry::ensure_type`].
pub fn init_drivers() -> DriverRegistry {
    DriverRegistry::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::mock_driver::{MockDriver, MockDriverOptions};

    fn capabilities(supports_cancel_query: bool) -> DriverCapabilities {
        DriverCapabilities {
            supports_cancel_query,
            supports_query_execution_cancel: supports_cancel_query,
            supports_explain: true,
            supports_streaming_results: true,
        }
    }

    #[tokio::test]
    async fn get_capabilities_returns_registered_metadata_by_type() {
        let registry = DriverRegistry::new();
        registry
            .register_test_driver_with_capabilities(
                "test-driver",
                MockDriver::new("test-driver", MockDriverOptions::default()),
                capabilities(true),
            )
            .await;

        assert_eq!(
            registry.get_capabilities(&"test-driver".to_string()).await,
            Some(capabilities(true))
        );
    }

    #[tokio::test]
    async fn legacy_test_driver_capability_is_unknown() {
        let registry = DriverRegistry::new();
        registry
            .register_test_driver(
                "legacy-driver",
                MockDriver::new("legacy-driver", MockDriverOptions::default()),
            )
            .await;

        assert_eq!(
            registry
                .get_capabilities(&"legacy-driver".to_string())
                .await,
            None
        );
    }

    #[tokio::test]
    async fn replacing_registered_metadata_with_legacy_driver_clears_capability() {
        let registry = DriverRegistry::new();
        registry
            .register_test_driver_with_capabilities(
                "test-driver",
                MockDriver::new("test-driver", MockDriverOptions::default()),
                capabilities(true),
            )
            .await;
        registry
            .register_test_driver(
                "test-driver",
                MockDriver::new("test-driver", MockDriverOptions::default()),
            )
            .await;

        assert!(registry
            .get_capabilities(&"test-driver".to_string())
            .await
            .is_none());
    }

    #[test]
    fn capabilities_serialize_using_frontend_camel_case() {
        let value = serde_json::to_value(capabilities(true)).expect("serialize capabilities");
        assert_eq!(value["supportsCancelQuery"], true);
        assert_eq!(value["supportsQueryExecutionCancel"], true);
        assert_eq!(value["supportsExplain"], true);
        assert_eq!(value["supportsStreamingResults"], true);
        assert!(value.get("supports_cancel_query").is_none());
    }
}
