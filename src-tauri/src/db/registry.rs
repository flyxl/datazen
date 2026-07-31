//! Driver registry — resolves `DatabaseType` to a concrete `DatabaseDriver`.

use datazen_driver_api::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::mysql::MysqlDriver;
use super::postgres::PostgresDriver;
use super::redis_driver::RedisDriver;
use super::sqlite::SqliteDriver;

/// Holds all registered drivers.
pub struct DriverRegistry {
    drivers: Arc<RwLock<HashMap<DatabaseType, Arc<dyn DatabaseDriver>>>>,
    kv_drivers: Arc<RwLock<HashMap<DatabaseType, Arc<dyn KeyValueDriver>>>>,
}

impl DriverRegistry {
    fn new() -> Self {
        Self {
            drivers: Arc::new(RwLock::new(HashMap::new())),
            kv_drivers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register(&self, driver: Arc<dyn DatabaseDriver>) {
        let mut drivers = self.drivers.write().await;
        drivers.insert(driver.driver_type(), driver);
    }

    pub async fn register_kv(&self, driver: Arc<dyn KeyValueDriver>) {
        let mut kv_drivers = self.kv_drivers.write().await;
        kv_drivers.insert(driver.driver_type(), driver);
    }

    pub async fn get(&self, db_type: &DatabaseType) -> Option<Arc<dyn DatabaseDriver>> {
        let drivers = self.drivers.read().await;
        drivers.get(db_type).cloned()
    }

    pub async fn supported_types(&self) -> Vec<DatabaseType> {
        let drivers = self.drivers.read().await;
        drivers.keys().cloned().collect()
    }

    pub async fn get_kv_driver(&self, db_type: &DatabaseType) -> Option<Arc<dyn KeyValueDriver>> {
        let kv_drivers = self.kv_drivers.read().await;
        kv_drivers.get(db_type).cloned()
    }
}

/// Registers built-in drivers and discovers plugin drivers via `inventory`.
pub async fn init_drivers() -> DriverRegistry {
    let registry = DriverRegistry::new();

    // Built-in drivers (always compiled into the binary)
    registry
        .register(Arc::new(PostgresDriver::new()))
        .await;
    registry
        .register(Arc::new(MysqlDriver::new(false)))
        .await;
    registry
        .register(Arc::new(MysqlDriver::new(true)))
        .await;
    registry
        .register(Arc::new(SqliteDriver::new()))
        .await;

    let redis_driver = Arc::new(RedisDriver::new());
    registry
        .register(redis_driver.clone() as Arc<dyn DatabaseDriver>)
        .await;
    registry
        .register_kv(redis_driver as Arc<dyn KeyValueDriver>)
        .await;

    // Plugin drivers discovered via inventory at link time.
    // Each plugin crate uses `register_driver!` to submit a factory.
    for factory in iter_driver_factories() {
        let driver = factory.create();
        tracing::info!("Registered plugin driver: {}", factory.driver_id());
        registry.register(driver).await;

        if let Some(kv) = factory.create_kv() {
            registry.register_kv(kv).await;
        }
    }

    registry
}
